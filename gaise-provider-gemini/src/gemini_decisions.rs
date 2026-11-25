//! Gemini Decisions API (`POST /v1beta/decisions`, released 2026-11-11,
//! `gemini-decision-1`, Gemini Developer API only) mapping for
//! [`GaiseClient::decision`](gaise_core::GaiseClient::decision).
//!
//! Source (audited 2026-11-25): https://aistudio.google.com/docs/decisions-api
//! and the 2026-11-11 announcement on blog.google.
//!
//! The Gemini question vocabulary (`noul`, `choice`, `score`, keyed by
//! question id) matches GAISe's System One-shaped decision contract, so the
//! translation is small:
//! - `state` becomes the text `input` (objects and arrays as JSON text);
//! - `noul` criteria `true` / `false` become `true_description` /
//!   `false_description`;
//! - `choice` criteria become the option map (a null description becomes
//!   `""`, which the API reads as a self-explanatory key);
//! - `score` criteria become the ordered level strings (`{label,
//!   description}` objects become `"label: description"`), and the score
//!   `legend` is rebuilt from the request criteria, not Gemini's strings.
//!
//! Media passed to [`GaiseClient::decision_with_images`](gaise_core::GaiseClient::decision_with_images)
//! follows the state text as inline base64 content blocks: images, audio, and
//! [`GaiseContent::File`] PDFs (`document`) or videos (by file extension).
//! Gemini reports no model version, so responses carry `request.model`.

use base64::Engine;
use gaise_core::contracts::{
    GaiseAnswer, GaiseContent, GaiseDecisionRequest, GaiseQuestion, GaiseUsage, audio_media_type,
    file_media_type, image_media_type,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Questions accepted per request.
pub const MAX_DECISION_QUESTIONS: usize = 1000;
/// Choice options and score levels accepted per question.
pub const MAX_DECISION_OPTIONS: usize = 255;
/// Inline media bytes (before base64) accepted per request.
pub const MAX_DECISION_INLINE_BYTES: usize = 100 * 1024 * 1024;

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
            Err(format!("{id}: Gemini decisions require instructions for every question").into())
        }
        Some(value) => Ok(text(value)),
    }
}

fn check_question_key(id: &str) -> Result<(), BoxError> {
    let valid = (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if valid {
        Ok(())
    } else {
        Err(
            format!("{id}: Gemini question keys must be 1 to 64 characters of [A-Za-z0-9_-]")
                .into(),
        )
    }
}

fn check_option_key(id: &str, key: &str) -> Result<(), BoxError> {
    let chars = key.chars().count();
    let valid = (1..=128).contains(&chars)
        && key.len() <= 512
        && key.trim() == key
        && !key.chars().any(char::is_control);
    if valid {
        Ok(())
    } else {
        Err(format!(
            "{id}: Gemini option key {key:?} must be 1 to 128 characters with no surrounding spaces or control characters"
        )
        .into())
    }
}

fn option_count(id: &str, kind: &str, count: usize) -> Result<(), BoxError> {
    if (2..=MAX_DECISION_OPTIONS).contains(&count) {
        Ok(())
    } else {
        Err(
            format!("{id}: Gemini {kind} questions need between 2 and {MAX_DECISION_OPTIONS}")
                .into(),
        )
    }
}

fn level(index: usize, criterion: &Value) -> String {
    match criterion {
        Value::String(label) => label.clone(),
        Value::Object(object) if object.get("label").is_some_and(Value::is_string) => {
            let label = text(&object["label"]);
            match object.get("description").filter(|v| !v.is_null()) {
                Some(description) => format!("{label}: {}", text(description)),
                None => label,
            }
        }
        Value::Null => index.to_string(),
        other => other.to_string(),
    }
}

fn media_block(kind: &str, mime_type: &str, data: &[u8]) -> Value {
    json!({
        "type": kind,
        "mime_type": mime_type,
        "data": base64::engine::general_purpose::STANDARD.encode(data),
    })
}

/// Video MIME types the Decisions API lists, by file extension.
fn video_media_type(name: Option<&str>) -> Option<&'static str> {
    let extension = name
        .and_then(|name| Path::new(name).extension())
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    Some(match extension.as_str() {
        "mp4" => "video/mp4",
        "mpeg" | "mpg" => "video/mpeg",
        "mov" => "video/mov",
        "avi" => "video/avi",
        "webm" => "video/webm",
        "wmv" => "video/wmv",
        "flv" => "video/x-flv",
        "3gp" | "3gpp" => "video/3gpp",
        _ => return None,
    })
}

fn content_block(content: &GaiseContent) -> Result<(Value, usize), BoxError> {
    Ok(match content {
        GaiseContent::Text { text } => (json!({ "type": "text", "text": format!("\n{text}") }), 0),
        GaiseContent::Image { data, format } => (
            media_block("image", &image_media_type(format.as_deref()), data),
            data.len(),
        ),
        GaiseContent::Audio { data, format } => {
            let mime_type = match audio_media_type(format.as_deref()).as_str() {
                // The Decisions API lists audio/m4a, not audio/mp4.
                "audio/mp4" => "audio/m4a".to_string(),
                other => other.to_string(),
            };
            (media_block("audio", &mime_type, data), data.len())
        }
        GaiseContent::File { data, name } => {
            let block = if file_media_type(name.as_deref()) == "application/pdf" {
                media_block("document", "application/pdf", data)
            } else if let Some(mime_type) = video_media_type(name.as_deref()) {
                media_block("video", mime_type, data)
            } else {
                return Err(format!(
                    "Gemini decisions accept PDF and video files only, not {}",
                    name.as_deref().unwrap_or("an unnamed file")
                )
                .into());
            };
            (block, data.len())
        }
        _ => {
            return Err(
                "Gemini decisions accept text, image, audio, and file content alongside the state"
                    .into(),
            );
        }
    })
}

/// The `POST /v1beta/decisions` body for `request`, addressed to `request.model`.
pub fn decision_request(request: &GaiseDecisionRequest) -> Result<Value, BoxError> {
    decision_request_with_media(request, &[])
}

/// [`decision_request`] with `media` shared by every question. With media,
/// `input` becomes an array of content blocks: the state text, then the media
/// in order.
pub fn decision_request_with_media(
    request: &GaiseDecisionRequest,
    media: &[GaiseContent],
) -> Result<Value, BoxError> {
    let state = match &request.state {
        Value::Null => return Err("Gemini decisions require a non-null state".into()),
        state => text(state),
    };
    if state.is_empty() && media.is_empty() {
        return Err("Gemini decisions require a non-empty input".into());
    }
    if !(1..=MAX_DECISION_QUESTIONS).contains(&request.questions.len()) {
        return Err(format!(
            "Gemini decisions accept between 1 and {MAX_DECISION_QUESTIONS} questions per request"
        )
        .into());
    }
    let input = if media.is_empty() {
        Value::String(state)
    } else {
        let mut blocks = vec![json!({ "type": "text", "text": state })];
        let mut inline_bytes = 0;
        for content in media {
            let (block, bytes) = content_block(content)?;
            inline_bytes += bytes;
            blocks.push(block);
        }
        if inline_bytes > MAX_DECISION_INLINE_BYTES {
            return Err(
                "Gemini decisions accept at most 100 MB of inline media per request".into(),
            );
        }
        Value::Array(blocks)
    };
    let mut questions = Map::new();
    for (id, question) in &request.questions {
        check_question_key(id)?;
        let question = match question {
            GaiseQuestion::Noul {
                instructions: given,
                criteria,
            } => {
                let mut question =
                    json!({ "type": "noul", "instructions": instructions(id, given)? });
                let mut descriptions = Map::new();
                if let Some(criteria) = criteria {
                    for (key, value) in [
                        ("true_description", &criteria.yes),
                        ("false_description", &criteria.no),
                    ] {
                        if let Some(value) = value.as_ref().filter(|v| !v.is_null()) {
                            descriptions.insert(key.into(), text(value).into());
                        }
                    }
                }
                if !descriptions.is_empty() {
                    question["criteria"] = Value::Object(descriptions);
                }
                question
            }
            GaiseQuestion::Choice {
                instructions: given,
                criteria,
            } => {
                option_count(id, "choice", criteria.len())?;
                let mut options = Map::new();
                for (key, description) in criteria {
                    check_option_key(id, key)?;
                    let description = match description {
                        Value::Null => String::new(),
                        other => text(other),
                    };
                    options.insert(key.clone(), description.into());
                }
                json!({
                    "type": "choice",
                    "instructions": instructions(id, given)?,
                    "criteria": options,
                })
            }
            GaiseQuestion::Score {
                instructions: given,
                criteria,
            } => {
                option_count(id, "score", criteria.len())?;
                let levels: Vec<String> = criteria
                    .iter()
                    .enumerate()
                    .map(|(i, c)| level(i, c))
                    .collect();
                if levels.iter().any(String::is_empty) {
                    return Err(format!("{id}: Gemini score levels must not be empty").into());
                }
                json!({
                    "type": "score",
                    "instructions": instructions(id, given)?,
                    "criteria": levels,
                })
            }
        };
        questions.insert(id.clone(), question);
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
        .ok_or_else(|| format!("Gemini decision answer is missing {field}").into())
}

fn probabilities(answer: &Value) -> Result<BTreeMap<String, f64>, BoxError> {
    answer["probabilities"]
        .as_object()
        .ok_or("Gemini decision answer is missing probabilities")?
        .iter()
        .map(|(key, p)| Ok((key.clone(), number(p, "probability")?)))
        .collect()
}

/// Decision usage keeps the decision vocabulary (`input_tokens`,
/// `output_tokens`) shared with the other decision providers, plus Gemini's
/// per-modality input split (`text_tokens`, `image_tokens`, ...) and total.
pub fn map_decision_usage(usage: &Value) -> Option<GaiseUsage> {
    if !usage.is_object() {
        return None;
    }
    let count = |v: &Value| v.as_u64().map(|n| n as usize);
    let mut input = HashMap::new();
    if let Some(n) = count(&usage["total_input_tokens"]) {
        input.insert("input_tokens".to_string(), n);
    }
    for entry in usage["input_tokens_by_modality"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let (Some(modality), Some(n)) = (entry["modality"].as_str(), count(&entry["tokens"])) {
            *input
                .entry(format!("{}_tokens", modality.to_ascii_lowercase()))
                .or_insert(0) += n;
        }
    }
    let output = count(&usage["total_output_tokens"])
        .map(|n| HashMap::from([("output_tokens".to_string(), n)]));
    Some(GaiseUsage {
        input: (!input.is_empty()).then_some(input),
        output,
        total: count(&usage["total_tokens"])
            .map(|n| HashMap::from([("total_tokens".to_string(), n)])),
    })
}

/// Map a Decisions API `answers` object onto GAISe answers keyed by question
/// id. Answers for unknown questions and unexpected types are errors.
pub fn map_decision_answers(
    request: &GaiseDecisionRequest,
    answers: &Value,
) -> Result<BTreeMap<String, GaiseAnswer>, BoxError> {
    let answers = answers
        .as_object()
        .ok_or("Gemini decision response is missing answers")?;
    let mut mapped = BTreeMap::new();
    for (id, answer) in answers {
        let question = request
            .questions
            .get(id)
            .ok_or_else(|| format!("Gemini answered an unknown question {id}"))?;
        let gaise = match (answer["type"].as_str(), question) {
            (Some("noul"), GaiseQuestion::Noul { .. }) => GaiseAnswer::Noul {
                noul: number(&answer["noul"], "noul")?,
            },
            (Some("choice"), GaiseQuestion::Choice { .. }) => GaiseAnswer::Choice {
                choice: answer["choice"]
                    .as_str()
                    .ok_or("Gemini decision answer is missing choice")?
                    .to_string(),
                probabilities: probabilities(answer)?,
                confidence: number(&answer["confidence"], "confidence")?,
            },
            (Some("score"), GaiseQuestion::Score { criteria, .. }) => GaiseAnswer::Score {
                score: number(&answer["score"], "score")?,
                legend: criteria
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i.to_string(), c.clone()))
                    .collect(),
                probabilities: probabilities(answer)?,
                confidence: number(&answer["confidence"], "confidence")?,
            },
            (kind, _) => {
                return Err(format!(
                    "{id}: unexpected Gemini answer type {}",
                    kind.unwrap_or("(missing)")
                )
                .into());
            }
        };
        mapped.insert(id.clone(), gaise);
    }
    Ok(mapped)
}
