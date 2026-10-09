//! Ollama System One (`POST /v1/systemone`, Ollama 0.35+) through
//! `GaiseClient::decision`. Hermetic: a local server stands in for Ollama.
//! Contract source: https://docs.ollama.com/api/systemone (2026-09-30).

use axum::{Json, Router, http::StatusCode, routing::post};
use gaise_core::{GaiseClient, contracts::*};
use gaise_provider_ollama::ollama_client::GaiseClientOllama;
use serde_json::{Value, json};

fn request() -> GaiseDecisionRequest {
    serde_json::from_value(json!({
        "model": "nimble",
        "state": {"ticket": "I was charged twice. Please refund the extra payment."},
        "correlation_id": "local-only",
        "questions": {
            "team": {"type": "choice", "instructions": "Which team?", "criteria": {"billing": "Payments and refunds", "technical": "Bugs and integrations"}},
            "refund": {"type": "noul", "instructions": "Does the customer ask for a refund?"},
            "urgency": {"type": "score", "instructions": "How urgent?", "criteria": ["Routine", "Soon", "Urgent"]}
        }
    }))
    .unwrap()
}

// The example response from the Ollama 0.35 announcement.
fn answer() -> Value {
    json!({"model": "nimble", "answers": {
        "team": {"type": "choice", "choice": "billing", "probabilities": {"billing": 0.988, "technical": 0.012}, "confidence": 0.922},
        "refund": {"type": "noul", "noul": 0.997},
        "urgency": {"type": "score", "score": 0.815, "legend": {"0": "Routine", "1": "Soon", "2": "Urgent"}, "probabilities": {"0": 0.378, "1": 0.429, "2": 0.193}, "confidence": 0.046}
    }, "usage": {"input_tokens": 841, "output_tokens": 4}})
}

async fn server(app: Router) -> (GaiseClientOllama, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (GaiseClientOllama::new(url), task)
}

#[tokio::test]
async fn decision_posts_the_system_one_body_and_maps_answers_and_usage() {
    let app = Router::new().route(
        "/v1/systemone",
        post(|Json(body): Json<Value>| async move {
            assert_eq!(body.as_object().unwrap().len(), 3, "no routing metadata");
            assert_eq!(body["model"], "nimble");
            assert_eq!(body["state"], request().state);
            assert_eq!(
                body["questions"],
                serde_json::to_value(request().questions).unwrap()
            );
            Json(answer())
        }),
    );
    let (client, task) = server(app).await;
    let response = client.decision(&request()).await.unwrap();
    assert_eq!(response.model, "nimble");
    assert!(matches!(
        &response.answers["team"],
        GaiseAnswer::Choice { choice, confidence, .. } if choice == "billing" && *confidence == 0.922
    ));
    assert_eq!(
        response.answers["refund"],
        GaiseAnswer::Noul { noul: 0.997 }
    );
    assert!(matches!(
        &response.answers["urgency"],
        GaiseAnswer::Score { score, legend, .. } if *score == 0.815 && legend["2"] == "Urgent"
    ));
    let usage = response.usage.unwrap();
    assert_eq!(usage.input.unwrap()["input_tokens"], 841);
    assert_eq!(usage.output.unwrap()["output_tokens"], 4);
    assert!(usage.total.is_none());
    task.abort();
}

#[tokio::test]
async fn decision_surfaces_ollama_errors_and_mismatched_answers() {
    let app = Router::new().route(
        "/v1/systemone",
        post(|Json(body): Json<Value>| async move {
            if body["model"] == "missing" {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "model 'missing' not found"})),
                );
            }
            let mut wrong = answer();
            wrong["answers"]["refund"] = json!({"type": "choice", "choice": "yes", "probabilities": {"yes": 1.0}, "confidence": 1.0});
            (StatusCode::OK, Json(wrong))
        }),
    );
    let (client, task) = server(app).await;

    let mut missing = request();
    missing.model = "missing".into();
    let error = client.decision(&missing).await.unwrap_err().to_string();
    assert!(
        error.contains("404") && error.contains("not found"),
        "{error}"
    );

    let error = client.decision(&request()).await.unwrap_err().to_string();
    assert!(error.contains("do not match the questions"), "{error}");

    let mut empty = request();
    empty.questions.clear();
    assert!(client.decision(&empty).await.is_err(), "validated locally");
    task.abort();
}

// Clef / Clef Flash (Ollama 0.35.1): `images` is base64 without a data-URL
// prefix, shared by all questions.
#[tokio::test]
async fn decision_with_images_sends_raw_base64_images() {
    let app = Router::new().route(
        "/v1/systemone",
        post(|Json(body): Json<Value>| async move {
            assert_eq!(body["images"], json!(["iVBORw=="]));
            assert_eq!(body.as_object().unwrap().len(), 4);
            let mut reply = answer();
            reply["model"] = json!("clef");
            Json(reply)
        }),
    );
    let (client, task) = server(app).await;
    let mut clef = request();
    clef.model = "clef".into();
    let png = GaiseContent::Image {
        data: vec![137, 80, 78, 71],
        format: Some("png".into()),
    };
    let response = client.decision_with_images(&clef, &[png]).await.unwrap();
    assert_eq!(response.model, "clef");

    let text = GaiseContent::Text { text: "no".into() };
    assert!(client.decision_with_images(&clef, &[text]).await.is_err());
    task.abort();
}
