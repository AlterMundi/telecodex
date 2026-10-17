//! Bound top-level native worker turns while leaving the integrator responsive.
use super::*;
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(super) struct WorkerBudget {
    slots: Arc<Semaphore>,
    limit: usize,
    state: StdMutex<State>,
    changed: tokio::sync::Notify,
}
#[derive(Default)]
struct State {
    waiting: HashMap<SessionKey, Instant>,
    active: HashMap<SessionKey, Instant>,
    attached: std::collections::HashSet<SessionKey>,
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
    _permit: Option<OwnedSemaphorePermit>,
}
impl Drop for WorkerLease {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().expect("budget mutex poisoned");
        state.active.remove(&self.key);
        state.attached.remove(&self.key);
        drop(state);
        self.budget.changed.notify_waiters();
    }
}
impl WorkerBudget {
    pub(super) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            slots: Arc::new(Semaphore::new(limit)),
            limit,
            state: StdMutex::new(State::default()),
            changed: tokio::sync::Notify::new(),
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
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let admitted = {
                let mut state = self.state.lock().expect("budget mutex poisoned");
                if state.active.len() < self.limit {
                    state.waiting.remove(&key);
                    state.active.insert(key, Instant::now());
                    state.started += 1;
                    state.peak = state.peak.max(state.active.len());
                    let elapsed = since.elapsed().as_millis();
                    state.wait_ms += elapsed;
                    state.max_wait_ms = state.max_wait_ms.max(elapsed);
                    true
                } else {
                    false
                }
            };
            if admitted {
                break;
            }
            tokio::select! {
                biased;
                _=cancel.cancelled()=>bail!("Worker capacity wait cancelled before native preparation"),
                _=changed=>{},
            }
        }
        drop(waiting);
        Ok(WorkerLease {
            budget: self.clone(),
            key,
            _permit: Some(permit),
        })
    }
    /// Already-running native writers must be collected immediately, even if
    /// their count exceeds this successor's configured kickoff capacity.
    pub(super) fn observe_existing(self: &Arc<Self>, key: SessionKey) -> Result<WorkerLease> {
        let mut state = self.state.lock().expect("budget mutex poisoned");
        if state.active.contains_key(&key) || state.waiting.contains_key(&key) {
            bail!("Worker already has a capacity reservation");
        }
        let permit = self.slots.clone().try_acquire_owned().ok();
        state.active.insert(key, Instant::now());
        state.attached.insert(key);
        state.peak = state.peak.max(state.active.len());
        Ok(WorkerLease {
            budget: self.clone(),
            key,
            _permit: permit,
        })
    }
    pub(super) fn snapshot(&self) -> Value {
        let state = self.state.lock().expect("budget mutex poisoned");
        json!({"scope":"Enrolled top-level native turn reservations in this listener instance; not provider-wide model calls, child agents or task completion","limit":self.limit,"active":state.active.len(),"waiting":state.waiting.len(),"started":state.started,"attached_collectors":state.attached.len(),"peak":state.peak,"total_wait_ms":state.wait_ms,"max_wait_ms":state.max_wait_ms,"waiting_topics":state.waiting.keys().map(|k|k.thread_id).collect::<Vec<_>>(),"active_topics":state.active.keys().map(|k|k.thread_id).collect::<Vec<_>>(),"oldest_wait_ms":state.waiting.values().map(|t|t.elapsed().as_millis()).max().unwrap_or(0),"currency_cost":"unknown"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn existing_collectors_bypass_slots_but_block_new_kickoffs() {
        let budget = WorkerBudget::new(2);
        let first = budget
            .observe_existing(SessionKey::new(100, Some(7)))
            .unwrap();
        let second = budget
            .observe_existing(SessionKey::new(100, Some(8)))
            .unwrap();
        let third = budget
            .observe_existing(SessionKey::new(100, Some(9)))
            .unwrap();
        assert_eq!(budget.snapshot()["active"], 3);
        assert_eq!(budget.snapshot()["attached_collectors"], 3);
        let pending = tokio::spawn({
            let b = budget.clone();
            async move {
                b.acquire(SessionKey::new(100, Some(10)), &CancellationToken::new())
                    .await
            }
        });
        tokio::task::yield_now().await;
        assert!(!pending.is_finished());
        drop(first);
        tokio::task::yield_now().await;
        assert!(!pending.is_finished());
        drop(second);
        let new = tokio::time::timeout(Duration::from_secs(1), pending)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(budget.snapshot()["active"], 2);
        assert_eq!(budget.snapshot()["started"], 1);
        drop(third);
        drop(new);
        assert_eq!(budget.snapshot()["active"], 0);
    }
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
