//! Typed decisions over shared state. The initial wire shape follows TypeSafe's
//! System One API.

use super::{GaiseConnection, GaiseUsage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseSystemOneRequest {
    /// Routable `provider::model` in the router; bare model in an adapter.
    pub model: String,
    pub state: Value,
    pub questions: BTreeMap<String, GaiseQuestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<GaiseConnection>,
}

/// Instructions and rubric descriptions accept text, objects, arrays, or null,
/// matching the official TypeSafe SDK. Nested JSON is preserved verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GaiseQuestion {
    Noul {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        criteria: Option<GaiseNoulCriteria>,
    },
    Choice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<Value>,
        criteria: BTreeMap<String, Value>,
    },
    Score {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<Value>,
        criteria: Vec<Value>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaiseNoulCriteria {
    #[serde(rename = "true", default, skip_serializing_if = "Option::is_none")]
    pub yes: Option<Value>,
    #[serde(rename = "false", default, skip_serializing_if = "Option::is_none")]
    pub no: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GaiseAnswer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Score {
        /// Expected score; fractional values are preserved.
        score: f64,
        legend: BTreeMap<String, Value>,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaiseSystemOneResponse {
    pub model: String,
    pub answers: BTreeMap<String, GaiseAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<GaiseUsage>,
}

impl GaiseSystemOneRequest {
    pub fn validate(&self) -> Result<(), String> {
        fn entry(value: &Value) -> bool {
            matches!(
                value,
                Value::Null | Value::String(_) | Value::Object(_) | Value::Array(_)
            )
        }
        if self.model.trim().is_empty() {
            return Err("System One model must not be empty".into());
        }
        if !entry(&self.state) {
            return Err("System One state must be text, an object, an array, or null".into());
        }
        if self.questions.is_empty() {
            return Err("System One requires at least one question".into());
        }
        for (id, question) in &self.questions {
            let (instructions, descriptions): (_, Vec<&Value>) = match question {
                GaiseQuestion::Noul {
                    instructions,
                    criteria,
                } => (
                    instructions,
                    criteria
                        .as_ref()
                        .map(|c| c.yes.iter().chain(c.no.iter()).collect())
                        .unwrap_or_default(),
                ),
                GaiseQuestion::Choice {
                    instructions,
                    criteria,
                } => {
                    if criteria.is_empty() {
                        return Err(format!("{id}: choice requires at least one option"));
                    }
                    (instructions, criteria.values().collect())
                }
                GaiseQuestion::Score {
                    instructions,
                    criteria,
                } => {
                    if criteria.len() < 2 {
                        return Err(format!("{id}: score requires at least two ordered levels"));
                    }
                    (instructions, criteria.iter().collect())
                }
            };
            if instructions.as_ref().is_some_and(|v| !entry(v))
                || descriptions.iter().any(|v| !entry(v))
            {
                return Err(format!(
                    "{id}: instructions and criteria descriptions must be text, objects, arrays, or null"
                ));
            }
        }
        Ok(())
    }
}
