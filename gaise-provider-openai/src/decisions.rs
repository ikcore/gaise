//! Decisions API (`POST /v1/decisions`, public beta since 2026-10-06) mapping
//! for [`GaiseClient::decision`](gaise_core::GaiseClient::decision).
//!
//! Sources (audited 2026-10-09): https://developers.openai.com/api/docs/guides/decisions
//! and the `decisions.create` API reference.
//!
//! GAISe's decision shape follows TypeSafe's System One protocol, so this
//! module translates it:
//! - `state` becomes the text `input` (objects and arrays as JSON text);
//! - question ids become question `name`s, and answers are matched by name;
//! - `noul` becomes `predicate`, with `true` / `false` criteria folded into the
//!   instructions;
//! - `choice` criteria become `choices` (2 to 255 values);
//! - `score` criteria become ordered `levels`, and the score `legend` is rebuilt
//!   from the request criteria.
//!
//! Images passed to [`GaiseClient::decision_with_images`](gaise_core::GaiseClient::decision_with_images)
//! are sent as base64 data URLs after the state text in one user message (the
//! guide does not accept hosted URLs or file ids). `safety_identifier` has no
//! GAISe field and is not sent.
//! A refused question fails the whole call, because `GaiseAnswer` has no
//! refusal variant.

use base64::Engine;
use gaise_core::contracts::{
    GaiseAnswer, GaiseContent, GaiseDecisionRequest, GaiseQuestion, GaiseUsage, image_media_type,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Choice questions accept between 2 and 255 choices.
pub const MAX_DECISION_CHOICES: usize = 255;
/// Images allowed across all input messages of one request.
pub const MAX_DECISION_IMAGES: usize = 128;

/// JSON values become text: strings as-is, anything else as compact JSON.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn instructions(id: &str, value: &Option<Value>) -> Result<String, BoxError> {
    match value {
        Some(Value::Null) | None => {
            Err(format!("{id}: OpenAI decisions require instructions for every question").into())
        }
        Some(value) => Ok(text(value)),
    }
}

fn level(index: usize, criterion: &Value) -> Value {
    match criterion {
        Value::String(label) => json!({ "label": label }),
        Value::Object(object) if object.get("label").is_some_and(Value::is_string) => {
            let mut level = json!({ "label": object["label"] });
            if let Some(description) = object.get("description").filter(|v| !v.is_null()) {
                level["description"] = text(description).into();
            }
            level
        }
        Value::Null => json!({ "label": index.to_string() }),
        other => json!({ "label": other.to_string() }),
    }
}

/// The `POST /v1/decisions` body for `request`, addressed to `request.model`.
pub fn decision_request(request: &GaiseDecisionRequest) -> Result<Value, BoxError> {
    decision_request_with_images(request, &[])
}

/// [`decision_request`] with `images` ([`GaiseContent::Image`] only) shared by
/// every question. With images, `input` becomes one user message whose parts
/// are the state text followed by the images in order.
pub fn decision_request_with_images(
    request: &GaiseDecisionRequest,
    images: &[GaiseContent],
) -> Result<Value, BoxError> {
    let state = match &request.state {
        Value::Null => return Err("OpenAI decisions require a non-null state".into()),
        state => text(state),
    };
    if images.len() > MAX_DECISION_IMAGES {
        return Err(format!(
            "OpenAI decisions accept at most {MAX_DECISION_IMAGES} images per request"
        )
        .into());
    }
    let input = if images.is_empty() {
        Value::String(state)
    } else {
        let mut parts = vec![json!({ "type": "input_text", "text": state })];
        for image in images {
            let GaiseContent::Image { data, format } = image else {
                return Err(
                    "OpenAI decisions accept only image content alongside the state".into(),
                );
            };
            let url = format!(
                "data:{};base64,{}",
                image_media_type(format.as_deref()),
                base64::engine::general_purpose::STANDARD.encode(data)
            );
            parts.push(json!({ "type": "input_image", "image_url": url }));
        }
        json!([{ "type": "message", "role": "user", "content": parts }])
    };
    let mut questions = Vec::with_capacity(request.questions.len());
    for (id, question) in &request.questions {
        let question = match question {
            GaiseQuestion::Noul {
                instructions: given,
                criteria,
            } => {
                let mut prompt = instructions(id, given)?;
                if let Some(criteria) = criteria {
                    for (label, value) in
                        [("True when", &criteria.yes), ("False when", &criteria.no)]
                    {
                        if let Some(value) = value.as_ref().filter(|v| !v.is_null()) {
                            prompt.push_str(&format!("\n{label}: {}", text(value)));
                        }
                    }
                }
                json!({ "type": "predicate", "name": id, "instructions": prompt })
            }
            GaiseQuestion::Choice {
                instructions: given,
                criteria,
            } => {
                if !(2..=MAX_DECISION_CHOICES).contains(&criteria.len()) {
                    return Err(format!(
                        "{id}: OpenAI choice questions need between 2 and {MAX_DECISION_CHOICES} options"
                    )
                    .into());
                }
                let choices: Vec<Value> = criteria
                    .iter()
                    .map(|(value, description)| {
                        let mut choice = json!({ "value": value });
                        if !description.is_null() {
                            choice["description"] = text(description).into();
                        }
                        choice
                    })
                    .collect();
                json!({
                    "type": "choice",
                    "name": id,
                    "instructions": instructions(id, given)?,
                    "choices": choices,
                })
            }
            GaiseQuestion::Score {
                instructions: given,
                criteria,
            } => json!({
                "type": "score",
                "name": id,
                "instructions": instructions(id, given)?,
                "levels": criteria.iter().enumerate().map(|(i, c)| level(i, c)).collect::<Vec<_>>(),
            }),
        };
        questions.push(question);
    }
    Ok(json!({
        "model": request.model,
        "input": input,
        "questions": questions,
    }))
}

fn number(value: &Value, field: &str) -> Result<f64, BoxError> {
    value
        .as_f64()
        .ok_or_else(|| format!("OpenAI decision answer is missing {field}").into())
}

/// Choice values are strings or booleans; booleans map to `"true"` / `"false"`.
fn choice_value(value: &Value) -> Result<String, BoxError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Bool(b) => Ok(b.to_string()),
        other => Err(format!("unexpected OpenAI choice value {other}").into()),
    }
}

fn probabilities(
    answer: &Value,
    key: impl Fn(&Value) -> Result<String, BoxError>,
) -> Result<BTreeMap<String, f64>, BoxError> {
    answer["probabilities"]
        .as_array()
        .ok_or("OpenAI decision answer is missing probabilities")?
        .iter()
        .map(|entry| {
            Ok((
                key(&entry["value"])?,
                number(&entry["probability"], "probability")?,
            ))
        })
        .collect()
}

fn score_index(value: &Value) -> Result<String, BoxError> {
    value
        .as_u64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|f| f.fract() == 0.0 && *f >= 0.0)
                .map(|f| f as u64)
        })
        .map(|i| i.to_string())
        .ok_or_else(|| format!("unexpected OpenAI score level {value}").into())
}

/// Decision usage keeps the decision vocabulary (`input_tokens`,
/// `output_tokens`) shared with TypeSafe and Ollama, plus OpenAI's cache and
/// reasoning details and its total.
pub fn map_decision_usage(usage: &Value) -> Option<GaiseUsage> {
    if !usage.is_object() {
        return None;
    }
    let count = |v: &Value| v.as_u64().map(|n| n as usize);
    let collect = |pairs: &[(&str, &Value)]| -> HashMap<String, usize> {
        pairs
            .iter()
            .filter_map(|(key, v)| count(v).map(|n| (key.to_string(), n)))
            .collect()
    };
    Some(GaiseUsage {
        input: Some(collect(&[
            ("input_tokens", &usage["input_tokens"]),
            (
                "cached_tokens",
                &usage["input_tokens_details"]["cached_tokens"],
            ),
            (
                "cache_write_tokens",
                &usage["input_tokens_details"]["cache_write_tokens"],
            ),
        ])),
        output: Some(collect(&[
            ("output_tokens", &usage["output_tokens"]),
            (
                "reasoning_tokens",
                &usage["output_tokens_details"]["reasoning_tokens"],
            ),
        ])),
        total: count(&usage["total_tokens"])
            .map(|n| HashMap::from([("total_tokens".to_string(), n)])),
    })
}

/// Map a Decisions API `answers` array onto GAISe answers keyed by question
/// id. Refusals, unnamed answers, and answers for unknown questions are errors.
pub fn map_decision_answers(
    request: &GaiseDecisionRequest,
    answers: &Value,
) -> Result<BTreeMap<String, GaiseAnswer>, BoxError> {
    let answers = answers
        .as_array()
        .ok_or("OpenAI decision response is missing answers")?;
    let refused: Vec<&str> = answers
        .iter()
        .filter(|a| a["type"] == "refusal")
        .map(|a| a["name"].as_str().unwrap_or("(unnamed)"))
        .collect();
    if !refused.is_empty() {
        return Err(format!("OpenAI refused to answer: {}", refused.join(", ")).into());
    }
    let mut mapped = BTreeMap::new();
    for answer in answers {
        let name = answer["name"]
            .as_str()
            .ok_or("OpenAI decision answer has no name")?;
        let question = request
            .questions
            .get(name)
            .ok_or_else(|| format!("OpenAI answered an unknown question {name}"))?;
        let gaise = match (answer["type"].as_str(), question) {
            (Some("predicate"), _) => GaiseAnswer::Noul {
                noul: number(&answer["probability"], "probability")?,
            },
            (Some("choice"), _) => GaiseAnswer::Choice {
                choice: choice_value(&answer["choice"])?,
                probabilities: probabilities(answer, choice_value)?,
                confidence: number(&answer["confidence"], "confidence")?,
            },
            (Some("score"), GaiseQuestion::Score { criteria, .. }) => GaiseAnswer::Score {
                score: number(&answer["score"], "score")?,
                legend: criteria
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i.to_string(), c.clone()))
                    .collect(),
                probabilities: probabilities(answer, score_index)?,
                confidence: number(&answer["confidence"], "confidence")?,
            },
            (kind, _) => {
                return Err(format!(
                    "{name}: unexpected OpenAI answer type {}",
                    kind.unwrap_or("(missing)")
                )
                .into());
            }
        };
        mapped.insert(name.to_string(), gaise);
    }
    Ok(mapped)
}
