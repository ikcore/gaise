//! Gemini Decisions API (`POST /v1beta/decisions`, `gemini-decision-1`,
//! released 2026-11-11) through `GaiseClient::decision`. Hermetic: a local
//! one-shot server stands in for Google.
//! Contract source: https://aistudio.google.com/docs/decisions-api (audited 2026-11-25).

use base64::Engine;
use gaise_core::{GaiseClient, contracts::*};
use gaise_provider_gemini::gemini_client::GaiseClientGemini;
use gaise_provider_gemini::gemini_decisions::{
    decision_request, decision_request_with_media, map_decision_usage,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn request() -> GaiseDecisionRequest {
    serde_json::from_value(json!({
        "model": "gemini-decision-1",
        "state": {"ticket": "I was charged twice. Please refund the extra payment."},
        "correlation_id": "local-only",
        "questions": {
            "team": {"type": "choice", "instructions": "Which team?", "criteria": {"billing": "Payments and refunds", "technical": null}},
            "refund": {"type": "noul", "instructions": "Does the customer ask for a refund?", "criteria": {"true": "A refund is requested", "false": "No refund is requested"}},
            "urgency": {"type": "score", "instructions": "How urgent?", "criteria": ["Routine", {"label": "Soon", "description": "Within a day"}, "Urgent"]}
        }
    }))
    .unwrap()
}

fn answer() -> Value {
    json!({
        "answers": {
            "refund": {"type": "noul", "noul": 0.95, "confidence": 0.95},
            "team": {"type": "choice", "choice": "billing", "probabilities": {"billing": 0.97, "technical": 0.03}, "confidence": 0.97},
            "urgency": {"type": "score", "score": 1.1, "legend": {"0": "Routine", "1": "Soon: Within a day", "2": "Urgent"},
                "probabilities": {"0": 0.1, "1": 0.7, "2": 0.2}, "confidence": 0.7}
        },
        "usage": {"total_input_tokens": 120, "input_tokens_by_modality": [
            {"modality": "text", "tokens": 40}, {"modality": "image", "tokens": 80}],
            "total_output_tokens": 0, "total_tokens": 120}
    })
}

#[test]
fn request_maps_questions_by_key() {
    assert_eq!(
        decision_request(&request()).unwrap(),
        json!({
            "model": "gemini-decision-1",
            "input": "{\"ticket\":\"I was charged twice. Please refund the extra payment.\"}",
            "questions": {
                "refund": {"type": "noul", "instructions": "Does the customer ask for a refund?",
                    "criteria": {"true_description": "A refund is requested", "false_description": "No refund is requested"}},
                "team": {"type": "choice", "instructions": "Which team?",
                    "criteria": {"billing": "Payments and refunds", "technical": ""}},
                "urgency": {"type": "score", "instructions": "How urgent?",
                    "criteria": ["Routine", "Soon: Within a day", "Urgent"]}
            }
        }),
        "no correlation_id or connection is sent"
    );

    let mut plain = request();
    plain.state = json!("plain text");
    plain.questions =
        serde_json::from_value(json!({"ok": {"type": "noul", "instructions": "Fine?"}})).unwrap();
    assert_eq!(
        decision_request(&plain).unwrap(),
        json!({"model": "gemini-decision-1", "input": "plain text",
            "questions": {"ok": {"type": "noul", "instructions": "Fine?"}}}),
        "noul criteria are omitted when absent"
    );
}

#[test]
fn request_rejects_what_the_decisions_api_cannot_take() {
    let mut null_state = request();
    null_state.state = Value::Null;
    assert!(decision_request(&null_state).is_err());

    let mut no_instructions = request();
    no_instructions.questions = serde_json::from_value(json!({"ok": {"type": "noul"}})).unwrap();
    assert!(decision_request(&no_instructions).is_err());

    let mut bad_key = request();
    bad_key.questions =
        serde_json::from_value(json!({"has space": {"type": "noul", "instructions": "x"}}))
            .unwrap();
    assert!(decision_request(&bad_key).is_err());

    let mut one_option = request();
    one_option.questions = serde_json::from_value(
        json!({"c": {"type": "choice", "instructions": "x", "criteria": {"only": null}}}),
    )
    .unwrap();
    assert!(decision_request(&one_option).is_err());

    let mut padded_option = request();
    padded_option.questions = serde_json::from_value(
        json!({"c": {"type": "choice", "instructions": "x", "criteria": {" a": null, "b": null}}}),
    )
    .unwrap();
    assert!(decision_request(&padded_option).is_err());

    let mut too_many = request();
    too_many.questions = (0..1001)
        .map(|i| {
            (
                format!("q{i}"),
                serde_json::from_value(json!({"type": "noul", "instructions": "x"})).unwrap(),
            )
        })
        .collect();
    assert!(decision_request(&too_many).is_err());

    let text_file = GaiseContent::File {
        data: b"x".to_vec(),
        name: Some("notes.txt".into()),
    };
    assert!(decision_request_with_media(&request(), &[text_file]).is_err());
}

#[test]
fn media_follows_the_state_as_inline_blocks() {
    let b64 = |d: &[u8]| base64::engine::general_purpose::STANDARD.encode(d);
    let media = [
        GaiseContent::Image {
            data: b"img".to_vec(),
            format: Some("png".into()),
        },
        GaiseContent::Audio {
            data: b"aud".to_vec(),
            format: Some("m4a".into()),
        },
        GaiseContent::File {
            data: b"pdf".to_vec(),
            name: Some("receipt.pdf".into()),
        },
        GaiseContent::File {
            data: b"vid".to_vec(),
            name: Some("unboxing.MOV".into()),
        },
    ];
    let mut text_state = request();
    text_state.state = json!("Ticket text");
    let body = decision_request_with_media(&text_state, &media).unwrap();
    assert_eq!(
        body["input"],
        json!([
            {"type": "text", "text": "Ticket text"},
            {"type": "image", "mime_type": "image/png", "data": b64(b"img")},
            {"type": "audio", "mime_type": "audio/m4a", "data": b64(b"aud")},
            {"type": "document", "mime_type": "application/pdf", "data": b64(b"pdf")},
            {"type": "video", "mime_type": "video/mov", "data": b64(b"vid")}
        ])
    );
}

#[test]
fn usage_keeps_decision_vocabulary_and_modalities() {
    let usage = map_decision_usage(&answer()["usage"]).unwrap();
    let input = usage.input.unwrap();
    assert_eq!(input["input_tokens"], 120);
    assert_eq!(input["text_tokens"], 40);
    assert_eq!(input["image_tokens"], 80);
    assert_eq!(usage.output.unwrap()["output_tokens"], 0);
    assert_eq!(usage.total.unwrap()["total_tokens"], 120);
    assert!(map_decision_usage(&Value::Null).is_none());
}

/// One-shot HTTP server returning `status` and `body`; yields the request it received.
async fn serve_once(
    status: &'static str,
    body: Value,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = socket.read(&mut chunk).await.unwrap();
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some(split) = text.find("\r\n\r\n") {
                let length = text[..split]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if buf.len() >= split + 4 + length {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        let payload = body.to_string();
        let reply = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            payload.len(),
            payload
        );
        socket.write_all(reply.as_bytes()).await.unwrap();
        String::from_utf8_lossy(&buf).to_string()
    });
    (format!("http://{addr}/v1beta"), handle)
}

#[tokio::test]
async fn decision_over_http() {
    let (url, server) = serve_once("200 OK", answer()).await;
    let client = GaiseClientGemini::new(url, "KEY".into());
    let response = client.decision(&request()).await.unwrap();
    let raw = server.await.unwrap();
    let head = raw.split("\r\n\r\n").next().unwrap().to_ascii_lowercase();
    assert!(
        head.starts_with("post /v1beta/decisions http/1.1"),
        "{head}"
    );
    assert!(head.contains("x-goog-api-key: key"), "{head}");
    assert!(!head.contains("?key="), "the key goes in the header");

    assert_eq!(response.model, "gemini-decision-1");
    assert_eq!(response.answers["refund"], GaiseAnswer::Noul { noul: 0.95 });
    let GaiseAnswer::Choice {
        choice, confidence, ..
    } = &response.answers["team"]
    else {
        panic!("team is a choice");
    };
    assert_eq!((choice.as_str(), *confidence), ("billing", 0.97));
    let GaiseAnswer::Score {
        score,
        legend,
        probabilities,
        ..
    } = &response.answers["urgency"]
    else {
        panic!("urgency is a score");
    };
    assert_eq!(*score, 1.1);
    assert_eq!(
        legend["1"],
        json!({"label": "Soon", "description": "Within a day"}),
        "the legend comes from the request criteria"
    );
    assert_eq!(probabilities["1"], 0.7);
    assert_eq!(response.usage.unwrap().input.unwrap()["input_tokens"], 120);
}

#[tokio::test]
async fn safety_errors_and_missing_answers_fail() {
    let (url, _server) = serve_once(
        "400 Bad Request",
        json!({"error": {"code": "safety", "message": "Request blocked due to safety violations"}}),
    )
    .await;
    let error = GaiseClientGemini::new(url, "KEY".into())
        .decision(&request())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("400") && error.contains("safety"), "{error}");

    let mut partial = answer();
    partial["answers"].as_object_mut().unwrap().remove("team");
    let (url, _server) = serve_once("200 OK", partial).await;
    assert!(
        GaiseClientGemini::new(url, "KEY".into())
            .decision(&request())
            .await
            .is_err()
    );

    let mut unknown = answer();
    unknown["answers"]["extra"] = json!({"type": "noul", "noul": 0.5, "confidence": 0.5});
    let (url, _server) = serve_once("200 OK", unknown).await;
    assert!(
        GaiseClientGemini::new(url, "KEY".into())
            .decision(&request())
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "calls the Gemini API; needs GEMINI_API_KEY"]
async fn live_decision() {
    let client = GaiseClientGemini::new(
        "https://generativelanguage.googleapis.com/v1beta".into(),
        std::env::var("GEMINI_API_KEY").unwrap(),
    );
    let response = client.decision(&request()).await.unwrap();
    request().check_answers(&response.answers).unwrap();
}
