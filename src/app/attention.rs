//! Bound top-level native worker turns while leaving the integrator responsive.
use super::*;
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(super) struct WorkerBudget {
    slots: Arc<Semaphore>,
    limit: usize,
    state: StdMutex<State>,
}
#[derive(Default)]
struct State {
    waiting: HashMap<SessionKey, Instant>,
    active: HashMap<SessionKey, Instant>,
    started: u64,
    peak: usize,
    wait_ms: u128,
    max_wait_ms: u128,
}
struct Waiting {
    budget: Arc<WorkerBudget>,
    key: SessionKey,
}
impl Drop for Waiting {
    fn drop(&mut self) {
        self.budget
            .state
            .lock()
            .expect("budget mutex poisoned")
            .waiting
            .remove(&self.key);
    }
}
pub(super) struct WorkerLease {
    budget: Arc<WorkerBudget>,
    key: SessionKey,
    _permit: OwnedSemaphorePermit,
}
impl Drop for WorkerLease {
    fn drop(&mut self) {
        self.budget
            .state
            .lock()
            .expect("budget mutex poisoned")
            .active
            .remove(&self.key);
    }
}
impl WorkerBudget {
    pub(super) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            slots: Arc::new(Semaphore::new(limit)),
            limit,
            state: StdMutex::new(State::default()),
        })
    }
    pub(super) async fn acquire(
        self: &Arc<Self>,
        key: SessionKey,
        cancel: &CancellationToken,
    ) -> Result<WorkerLease> {
        let since = Instant::now();
        {
            let mut state = self.state.lock().expect("budget mutex poisoned");
            if state.active.contains_key(&key) || state.waiting.contains_key(&key) {
                bail!("Worker already has a capacity reservation");
            }
            state.waiting.insert(key, since);
        }
        let waiting = Waiting {
            budget: self.clone(),
            key,
        };
        let permit = tokio::select! {
            biased;
            _=cancel.cancelled()=>bail!("Worker capacity wait cancelled before native preparation"),
            permit=self.slots.clone().acquire_owned()=>permit.context("Worker capacity unavailable")?,
        };
        if cancel.is_cancelled() {
            bail!("Worker capacity wait cancelled before native preparation");
        }
        {
            let mut state = self.state.lock().expect("budget mutex poisoned");
            state.waiting.remove(&key);
            state.active.insert(key, Instant::now());
            state.started += 1;
            state.peak = state.peak.max(state.active.len());
            let elapsed = since.elapsed().as_millis();
            state.wait_ms += elapsed;
            state.max_wait_ms = state.max_wait_ms.max(elapsed);
        }
        drop(waiting);
        Ok(WorkerLease {
            budget: self.clone(),
            key,
            _permit: permit,
        })
    }
    pub(super) fn snapshot(&self) -> Value {
        let state = self.state.lock().expect("budget mutex poisoned");
        json!({"scope":"Enrolled top-level native turn reservations in this listener instance; not provider-wide model calls, child agents or task completion","limit":self.limit,"active":state.active.len(),"waiting":state.waiting.len(),"started":state.started,"peak":state.peak,"total_wait_ms":state.wait_ms,"max_wait_ms":state.max_wait_ms,"waiting_topics":state.waiting.keys().map(|k|k.thread_id).collect::<Vec<_>>(),"active_topics":state.active.keys().map(|k|k.thread_id).collect::<Vec<_>>(),"oldest_wait_ms":state.waiting.values().map(|t|t.elapsed().as_millis()).max().unwrap_or(0),"currency_cost":"unknown"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn capacity_wait_is_cancelled_and_drop_releases_the_slot() {
        let budget = WorkerBudget::new(1);
        let cancel = CancellationToken::new();
        let first = budget
            .acquire(SessionKey::new(100, Some(7)), &cancel)
            .await
            .unwrap();
        let second_cancel = CancellationToken::new();
        let task = tokio::spawn({
            let b = budget.clone();
            let c = second_cancel.clone();
            async move { b.acquire(SessionKey::new(100, Some(8)), &c).await }
        });
        tokio::task::yield_now().await;
        assert_eq!(budget.snapshot()["waiting"], 1);
        second_cancel.cancel();
        assert!(task.await.unwrap().is_err());
        assert_eq!(budget.snapshot()["active"], 1);
        assert_eq!(budget.snapshot()["waiting"], 0);
        drop(first);
        let third = budget
            .acquire(SessionKey::new(100, Some(9)), &cancel)
            .await
            .unwrap();
        assert_eq!(budget.snapshot()["peak"], 1);
        assert_eq!(budget.snapshot()["started"], 2);
        drop(third);
        assert_eq!(budget.snapshot()["active"], 0);
    }
}
