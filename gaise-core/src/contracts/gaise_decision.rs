//! Typed decisions over shared state. The wire shape follows TypeSafe's
//! System One API; the operation was named `system_one` before 4.0.

use super::{GaiseConnection, GaiseUsage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseDecisionRequest {
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
pub struct GaiseDecisionResponse {
    pub model: String,
    pub answers: BTreeMap<String, GaiseAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<GaiseUsage>,
}

/// Body of the System One wire protocol (`POST /v1/systemone`), shared by
/// TypeSafe and Ollama: routing metadata is left out and only `model`,
/// `state`, and `questions` are sent.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionWireRequest<'a> {
    pub model: &'a str,
    pub state: &'a Value,
    pub questions: &'a BTreeMap<String, GaiseQuestion>,
}

/// Response of the System One wire protocol.
#[derive(Debug, Clone, Deserialize)]
pub struct DecisionWireResponse {
    pub model: String,
    pub answers: BTreeMap<String, GaiseAnswer>,
    #[serde(default)]
    pub usage: Option<DecisionWireUsage>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DecisionWireUsage {
    #[serde(default)]
    pub input_tokens: usize,
    #[serde(default)]
    pub output_tokens: usize,
}

impl From<DecisionWireResponse> for GaiseDecisionResponse {
    fn from(response: DecisionWireResponse) -> Self {
        Self {
            model: response.model,
            answers: response.answers,
            usage: response.usage.map(|usage| GaiseUsage {
                input: Some([("input_tokens".to_string(), usage.input_tokens)].into()),
                output: Some([("output_tokens".to_string(), usage.output_tokens)].into()),
                total: None,
            }),
        }
    }
}

impl GaiseDecisionRequest {
    /// The wire body for this request, addressed to `model`.
    pub fn wire<'a>(&'a self, model: &'a str) -> DecisionWireRequest<'a> {
        DecisionWireRequest {
            model,
            state: &self.state,
            questions: &self.questions,
        }
    }

    /// Check that `answers` has exactly one answer of the matching type for
    /// every question.
    pub fn check_answers(&self, answers: &BTreeMap<String, GaiseAnswer>) -> Result<(), String> {
        let matches = answers.len() == self.questions.len()
            && self.questions.iter().all(|(id, question)| {
                matches!(
                    (question, answers.get(id)),
                    (GaiseQuestion::Noul { .. }, Some(GaiseAnswer::Noul { .. }))
                        | (
                            GaiseQuestion::Choice { .. },
                            Some(GaiseAnswer::Choice { .. })
                        )
                        | (GaiseQuestion::Score { .. }, Some(GaiseAnswer::Score { .. }))
                )
            });
        if matches {
            Ok(())
        } else {
            Err("answer IDs or types do not match the questions".into())
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        fn entry(value: &Value) -> bool {
            matches!(
                value,
                Value::Null | Value::String(_) | Value::Object(_) | Value::Array(_)
            )
        }
        if self.model.trim().is_empty() {
            return Err("Decision model must not be empty".into());
        }
        if !entry(&self.state) {
            return Err("Decision state must be text, an object, an array, or null".into());
        }
        if self.questions.is_empty() {
            return Err("Decision requires at least one question".into());
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
