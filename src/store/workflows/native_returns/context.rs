//! Source-independent transport state; each source keeps its existing journal.
//! No synthetic workflow, coordinator, input or execution queue is introduced.
use super::*;

#[derive(Clone)]
pub(super) enum ReturnParent {
    Workflow(Workflow),
}

#[derive(Clone)]
pub(super) struct ReturnContext {
    pub human: i64,
    pub entry: SessionKey,
    pub native: String,
    pub epoch: String,
    pub native_returns: Vec<WorkflowReturn>,
    pub parent: ReturnParent,
}

impl ReturnContext {
    pub fn workflow(w: Workflow) -> Self {
        Self {
            human: w.human,
            entry: w.entry,
            native: w.native.clone(),
            epoch: w.epoch.clone(),
            native_returns: w.native_returns.clone(),
            parent: ReturnParent::Workflow(w),
        }
    }
    pub fn name(&self) -> &'static str {
        match &self.parent {
            ReturnParent::Workflow(_) => "workflow_attention",
        }
    }
    pub fn reported(&self, slot: usize) -> Option<&str> {
        match &self.parent {
            ReturnParent::Workflow(w) => w.reported.get(slot).and_then(Option::as_deref),
        }
    }
    pub fn output(&self, index: usize) -> String {
        let r = &self.native_returns[index];
        match &self.parent {
            ReturnParent::Workflow(w) => json!({"schema":"telecodex.workflow-native-return/v1","return_id":r.id,"workflow":w.id,"original_input_ref":w.plan.input_ref,"evidence":r.envelope,"policy":"Attributed internal evidence under this retained human workflow. Inspect exact source revisions on demand; preserve its selected milestone/final/question policy and unfinished required work. No new authority, helper chain, publication or hearing grant. Use workflow_reported only for an incorporated offer."}).to_string(),
        }
    }
}
