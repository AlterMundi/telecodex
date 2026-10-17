//! Human interaction contracts shared by the text controller and later adapters.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::models::SessionKey;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionConfig {
    pub human_id: i64,
    pub chat_id: i64,
    pub topic_id: i64,
    #[serde(default)]
    pub targets: Vec<Target>,
    #[serde(default = "default_active_workers")]
    pub max_active_workers: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub name: String,
    pub topic_id: i64,
}

fn default_active_workers() -> usize {
    2
}

impl InteractionConfig {
    pub fn entry(&self) -> SessionKey {
        SessionKey::new(self.chat_id, Some(self.topic_id))
    }

    pub fn validate(&self) -> Result<()> {
        if self.human_id <= 0
            || self.chat_id == 0
            || self.topic_id <= 0
            || self.targets.len() > 64
            || !(1..=64).contains(&self.max_active_workers)
        {
            bail!("Invalid interaction owner, entry, target count or active worker limit");
        }
        let mut ids = std::collections::HashSet::new();
        for target in &self.targets {
            let reserved = target
                .name
                .strip_prefix('t')
                .or_else(|| target.name.strip_prefix('T'))
                .and_then(|id| id.parse::<i64>().ok());
            if reserved.is_some_and(|id| id != target.topic_id) {
                bail!("A stable topic selector cannot label another destination");
            }
            if target.topic_id <= 0
                || target.topic_id == self.topic_id
                || !ids.insert(target.topic_id)
                || target.name.is_empty()
                || target.name.chars().any(|c| {
                    c.is_whitespace()
                        || c.is_control()
                        || matches!(c, '#' | '`' | '"' | '\'' | '<' | '>')
                })
            {
                bail!("Interaction targets require distinct topic IDs and single-token selectors");
            }
        }
        // Duplicate human labels remain resolvable through #t<topic_id>.
        Ok(())
    }

    pub fn target(&self, topic: i64) -> Option<SessionKey> {
        self.targets
            .iter()
            .find(|t| t.topic_id == topic)
            .map(|_| SessionKey::new(self.chat_id, Some(topic)))
    }

    pub fn resolve(&self, selector: &str) -> Vec<SessionKey> {
        if let Some(id) = selector
            .strip_prefix('t')
            .or_else(|| selector.strip_prefix('T'))
            .and_then(|id| id.parse::<i64>().ok())
        {
            return self.target(id).into_iter().collect();
        }
        self.targets
            .iter()
            .filter(|t| t.name.to_lowercase() == selector.to_lowercase())
            .map(|t| SessionKey::new(self.chat_id, Some(t.topic_id)))
            .collect()
    }
}

/// A selector is deliberate only at the beginning of the human's message.
/// Quoted text, code blocks and incidental mentions remain ordinary content.
pub fn explicit_selector(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let selected = text
        .strip_prefix("/self#")
        .or_else(|| text.strip_prefix('#'))?;
    let end = selected.find(char::is_whitespace).unwrap_or(selected.len());
    let selector = &selected[..end];
    if selector.is_empty()
        || selector
            .chars()
            .any(|c| matches!(c, '#' | '`' | '"' | '\'' | '<' | '>'))
    {
        return None;
    }
    Some((selector, selected[end..].trim_start()))
}

/// Original Telegram update or authenticated conversation UUID. The wire type
/// preserves the source namespace; conversation IDs never become fake updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputReference {
    Telegram(i64),
    Conversation(String),
}
impl InputReference {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Telegram(id) if *id >= 0 => Ok(()),
            Self::Conversation(id)
                if uuid::Uuid::parse_str(id)
                    .ok()
                    .is_some_and(|v| v.to_string() == *id) =>
            {
                Ok(())
            }
            _ => bail!("Invalid original input reference"),
        }
    }
    pub fn update(&self) -> Option<i64> {
        if let Self::Telegram(id) = self {
            Some(*id)
        } else {
            None
        }
    }
    pub fn event(&self) -> String {
        match self {
            Self::Telegram(id) => id.to_string(),
            Self::Conversation(id) => id.clone(),
        }
    }
}
impl From<i64> for InputReference {
    fn from(id: i64) -> Self {
        Self::Telegram(id)
    }
}
impl std::fmt::Display for InputReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.event())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnRoute {
    pub chat_id: i64,
    pub thread_id: i64,
    pub message_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription: Option<crate::voice::TranscriptSource>,
    pub channel: String,
    pub event_id: String,
    pub update_id: Option<i64>,
    pub human_id: i64,
    pub target_sessions: Vec<i64>,
    pub origin: ReturnRoute,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "requests", rename_all = "snake_case")]
pub enum RequestRelation {
    New,
    Amends(Vec<String>),
    Context(Vec<String>),
}

impl RequestRelation {
    pub fn requests(&self) -> &[String] {
        match self {
            Self::New => &[],
            Self::Amends(ids) | Self::Context(ids) => ids,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    pub id: String,
    pub input_id: String,
    pub human_id: i64,
    pub target_session: i64,
    pub target: SessionKey,
    pub sequence: i64,
    pub relation: RequestRelation,
    pub requests: Vec<String>,
    pub text: String,
    pub origin: ReturnRoute,
    pub phase: TransportPhase,
    /// Captured for this dispatch attempt; async continuations must not adopt a
    /// later admission generation. Only explicit known-unsent recovery clears it.
    #[serde(default)]
    pub admission_epoch: Option<String>,
    pub local_turn: Option<i64>,
    pub native_thread: Option<String>,
    pub native_turn: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportPhase {
    Queued,
    Submitting,
    Steering,
    Accepted,
    Uncertain,
    Rejected,
    RetainedUnsent,
    Cancelled,
}

impl TransportPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Submitting => "submitting",
            Self::Steering => "steering",
            Self::Accepted => "accepted",
            Self::Uncertain => "uncertain",
            Self::Rejected => "rejected",
            Self::RetainedUnsent => "retained_unsent",
            Self::Cancelled => "cancelled",
        }
    }
}

impl TryFrom<&str> for TransportPhase {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self> {
        Ok(match value {
            "queued" => Self::Queued,
            "submitting" => Self::Submitting,
            "steering" => Self::Steering,
            "accepted" => Self::Accepted,
            "uncertain" => Self::Uncertain,
            "rejected" => Self::Rejected,
            "retained_unsent" => Self::RetainedUnsent,
            "cancelled" => Self::Cancelled,
            _ => bail!("Unknown interaction transport state"),
        })
    }
}

/// This is the worker's declaration, never native proof of incorporation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub requests: Vec<String>,
    pub covered: Vec<String>,
    /// Addressing a request or incorporating its instructions is not completion.
    #[serde(default)]
    pub completed: Vec<String>,
    #[serde(default)]
    pub deferred: Vec<String>,
}

pub fn worker_result(text: &str) -> (&str, Option<Coverage>) {
    const START: &str = "\n<daimon-result>";
    const END: &str = "</daimon-result>";
    let Some((body, tail)) = text.trim_end().rsplit_once(START) else {
        return (text, None);
    };
    let Some(raw) = tail.strip_suffix(END) else {
        return (text, None);
    };
    let coverage = serde_json::from_str::<Coverage>(raw).ok().filter(|c| {
        !body.trim().is_empty()
            && !c.requests.is_empty()
            && c.requests.len() <= 64
            && c.covered.len() <= 128
            && c.completed.len() <= 64
            && c.completed.iter().all(|id| c.requests.contains(id))
            && c.deferred.len() <= 128
            && c.requests
                .iter()
                .chain(&c.covered)
                .chain(&c.completed)
                .chain(&c.deferred)
                .all(|id| !id.is_empty() && id.len() <= 128)
    });
    (body.trim_end(), coverage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_deliberate_top_level_selectors_route() {
        assert_eq!(
            explicit_selector(" #Nexus\nContinue"),
            Some(("Nexus", "Continue"))
        );
        assert_eq!(explicit_selector("#t42"), Some(("t42", "")));
        assert_eq!(
            explicit_selector("/self#Nexus Continue"),
            Some(("Nexus", "Continue"))
        );
        assert_eq!(explicit_selector("/self#t42"), Some(("t42", "")));
        for text in [
            "> #Other do work",
            "```\n#Other\n```",
            "\"#Other do work\"",
            "Discuss #Other",
            "Discuss /self#Other",
            "> /self#Other do work",
            "#",
        ] {
            assert_eq!(explicit_selector(text), None);
        }
    }

    #[test]
    fn duplicate_names_require_stable_selection() {
        let mut cfg = InteractionConfig {
            human_id: 100,
            chat_id: 100,
            topic_id: 1,
            max_active_workers: 2,
            targets: vec![
                Target {
                    name: "Nexus".into(),
                    topic_id: 2,
                },
                Target {
                    name: "Nexus".into(),
                    topic_id: 3,
                },
            ],
        };
        cfg.validate().unwrap();
        for limit in [0, 65, usize::MAX] {
            cfg.max_active_workers = limit;
            assert!(cfg.validate().is_err());
        }
        for limit in [1, 64, 2] {
            cfg.max_active_workers = limit;
            cfg.validate().unwrap();
        }
        assert_eq!(cfg.resolve("nexus").len(), 2);
        assert_eq!(cfg.resolve("t3"), vec![SessionKey::new(100, Some(3))]);
        assert_eq!(cfg.target(1), None);
        assert_eq!(cfg.resolve("T3"), cfg.resolve("t3"));
        cfg.targets[0].name = "t3".into();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn coverage_is_separate_from_canonical_answer_text() {
        let (body, claim) = worker_result(
            "Answer\n<daimon-result>{\"requests\":[\"A\"],\"covered\":[\"C\"]}</daimon-result>",
        );
        assert_eq!(body, "Answer");
        assert_eq!(claim.unwrap().covered, vec!["C"]);
        let (body, claim) = worker_result(
            "Answer\n<daimon-result>{\"requests\":[\"A\"],\"covered\":[],\"recipient\":\"elsewhere\"}</daimon-result>",
        );
        assert_eq!(body, "Answer");
        assert!(claim.is_none());
        assert!(worker_result("Ordinary answer").1.is_none());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub id: String,
    pub request_id: String,
    pub target: SessionKey,
    pub origin: ReturnRoute,
    pub body: String,
    pub covered: Vec<String>,
    pub declared: bool,
    pub completed: bool,
    pub current: bool,
    pub correction_pending: bool,
    pub phase: String,
}
