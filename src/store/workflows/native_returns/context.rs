//! Source-independent transport state; each source keeps its existing journal.
//! No synthetic workflow, coordinator, input or execution queue is introduced.
use super::*;

#[derive(Clone)]
pub(super) enum ReturnParent {
    Workflow(Box<Workflow>),
    Routed(Box<super::routed::RoutedParent>),
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

/// The existing preview is full source evidence only when its raw UTF-8 hash
/// matches the current canonical result. Truncated previews and questions still
/// require their existing owned source reader; no additional text is copied.
fn complete_result_preview(source: &Value) -> bool {
    let Some(text) = source["preview"].as_str() else {
        return false;
    };
    source["kind"] == "canonical_result"
        && !text.is_empty()
        && text.chars().count() <= 180
        && source["text_sha256"].as_str()
            == Some(format!("{:x}", Sha256::digest(text.as_bytes())).as_str())
}

impl ReturnContext {
    pub fn workflow(w: Workflow) -> Self {
        Self {
            human: w.human,
            entry: w.entry,
            native: w.native.clone(),
            epoch: w.epoch.clone(),
            native_returns: w.native_returns.clone(),
            parent: ReturnParent::Workflow(Box::new(w)),
        }
    }
    pub fn routed(parent: super::routed::RoutedParent) -> Self {
        Self {
            human: parent.human,
            entry: parent.entry,
            native: parent.native.clone(),
            epoch: parent.epoch.clone(),
            native_returns: parent.returns.clone(),
            parent: ReturnParent::Routed(Box::new(parent)),
        }
    }
    pub fn name(&self) -> &'static str {
        match &self.parent {
            ReturnParent::Workflow(_) => "workflow_attention",
            ReturnParent::Routed(_) => "conversation_attention",
        }
    }
    pub fn reported(&self, index: usize) -> Option<&str> {
        match &self.parent {
            ReturnParent::Workflow(w) => self
                .native_returns
                .get(index)
                .and_then(|r| r.envelope["slot"].as_u64())
                .and_then(|slot| w.reported.get(slot as usize))
                .and_then(Option::as_deref),
            ReturnParent::Routed(_) => self
                .native_returns
                .get(index)
                .and_then(|r| r.reported_revision.as_deref()),
        }
    }
    pub fn output(&self, index: usize) -> String {
        let r = &self.native_returns[index];
        match &self.parent {
            ReturnParent::Routed(parent) => json!({"schema":"telecodex.native-attention/v1","return_id":r.id,"original_input_ref":parent.original,"evidence":r.envelope,"source_text_complete":complete_result_preview(&r.envelope["source"]),"after_offer":{"operation":"attention_reported","arguments":{"return_id":r.id,"revision":r.envelope["revision"]}},"policy":"Attributed internal evidence from work routed under this retained human original, not new human speech or authority. If source_text_complete is true, evidence.source.preview is the entire canonical source text, verified against its raw-text SHA256: use this supplied source directly for the useful offer, without attention_source or capabilities discovery. Otherwise read the exact source with attention_source before reporting its contents; a truncated preview or native question is not full evidence. Keep lookup narration in tool activity; incorporate the useful attributed result or question concisely once in this Conversation. Answer an arriving human correction without repeating an offer already provided; preserve unfinished work and ask the human when a decision is needed. No new helper chain, work, publication or hearing grant. Invoke the supplied after_offer controller call only after actually incorporating this exact offer; it is a complete call, with existing validation, and needs no capabilities lookup."}).to_string(),
            ReturnParent::Workflow(w) => json!({"schema":"telecodex.workflow-native-return/v1","return_id":r.id,"workflow":w.id,"original_input_ref":w.plan.input_ref,"evidence":r.envelope,"policy":"Attributed internal evidence under this retained human workflow. Read exact source revisions before reporting their contents. Keep lookup narration in tool activity; incorporate the useful attributed result or question concisely once in this Conversation. Answer an arriving human correction without repeating an offer already provided. Preserve its selected milestone/final/question policy and unfinished required work. No new authority, helper chain, publication or hearing grant. Use workflow_reported only for an incorporated offer."}).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_preview_requires_exact_full_utf8_source_hash() {
        let text = "Informe de Alpha: revisión azul α.";
        let mut source = json!({"kind":"canonical_result","preview":text,
            "text_sha256":format!("{:x}",Sha256::digest(text.as_bytes()))});
        assert!(complete_result_preview(&source));
        source["preview"] = json!("Informe de Alpha");
        assert!(!complete_result_preview(&source));
        source["preview"] = json!(text);
        source["kind"] = json!("native_question");
        assert!(!complete_result_preview(&source));
        source["kind"] = json!("canonical_result");
        source["text_sha256"] = Value::Null;
        assert!(!complete_result_preview(&source));
        let oversized = "α".repeat(181);
        source["preview"] = json!(oversized);
        source["text_sha256"] = json!(format!("{:x}", Sha256::digest(oversized.as_bytes())));
        assert!(!complete_result_preview(&source));
    }
}
