//! Inert, explicitly receiver-selected per-being expression vocabulary.
use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionPack {
    pub schema: String,
    pub id: String,
    pub revision: String,
    pub authorship: String,
    pub defaults: BTreeMap<String, ClientDefault>,
    pub expressions: BTreeMap<String, Expression>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientDefault {
    pub expression: String,
    pub variant: String,
    pub columns: usize,
    pub rows: usize,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Expression {
    pub alt: String,
    pub variants: BTreeMap<String, Frames>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Frames {
    pub frames: Vec<String>,
    pub frame_ms: usize,
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}
fn label(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
impl ExpressionPack {
    pub fn fingerprint(&self) -> Result<String> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema != "daimon.ascii-expression/v1"
            || !identifier(&self.id)
            || !identifier(&self.revision)
            || !label(&self.authorship, 1000)
            || self.expressions.is_empty()
            || self.expressions.len() > 64
            || serde_json::to_vec(self)?.len() > 262144
        {
            bail!("Invalid bounded expression pack");
        }
        for (id, expression) in &self.expressions {
            if !identifier(id)
                || !label(&expression.alt, 1000)
                || expression.variants.is_empty()
                || expression.variants.len() > 8
            {
                bail!("Invalid expression vocabulary");
            }
            for (name, variant) in &expression.variants {
                if !identifier(name)
                    || variant.frames.is_empty()
                    || variant.frames.len() > 32
                    || !(40..=2000).contains(&variant.frame_ms)
                    || variant.frames.len() * variant.frame_ms > 10000
                {
                    bail!("Invalid finite expression sequence");
                }
                for frame in &variant.frames {
                    if frame.is_empty()
                        || frame.lines().count() > 100
                        || frame.split('\n').any(|line| line.len() > 240)
                        || !frame.bytes().all(|c| c == b'\n' || (32..=126).contains(&c))
                    {
                        bail!("Expression frames require bounded printable ASCII");
                    }
                }
            }
        }
        if self.defaults.len() != 3 {
            bail!("Require cli, telegram and app expression defaults");
        }
        for client in ["cli", "telegram", "app"] {
            let Some(default) = self.defaults.get(client) else {
                bail!("Missing expression client default");
            };
            if !(1..=240).contains(&default.columns)
                || !(1..=100).contains(&default.rows)
                || !self
                    .expressions
                    .get(&default.expression)
                    .is_some_and(|e| e.variants.contains_key(&default.variant))
            {
                bail!("Invalid expression default");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_vocabulary_and_inert_frames() {
        let mut pack: ExpressionPack =
            serde_json::from_str(include_str!("../tests/fixtures/source-intervals.json")).unwrap();
        pack.validate().unwrap();
        let expression = pack.expressions.remove("presence").unwrap();
        pack.expressions
            .insert("interval-opening".into(), expression);
        for default in pack.defaults.values_mut() {
            if default.expression == "presence" {
                default.expression = "interval-opening".into();
            }
        }
        pack.validate().unwrap();
        pack.expressions
            .get_mut("interval-opening")
            .unwrap()
            .variants
            .get_mut("spark")
            .unwrap()
            .frames[0] = "\u{1b}[2J".into();
        assert!(pack.validate().is_err());
    }
}
