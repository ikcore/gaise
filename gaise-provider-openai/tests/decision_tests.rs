//! OpenAI Decisions API (`POST /v1/decisions`, public beta 2026-10-06) through
//! `GaiseClient::decision`. Hermetic: a local server stands in for OpenAI.
//! Contract source: https://developers.openai.com/api/docs/guides/decisions
//! and the `decisions.create` reference (audited 2026-10-09).

use axum::{Json, Router, http::HeaderMap, http::StatusCode, routing::post};
use gaise_core::{GaiseClient, contracts::*};
use gaise_provider_openai::decisions::{
    decision_request, decision_request_with_images, map_decision_usage,
};
use gaise_provider_openai::openai_client::GaiseClientOpenAI;
use serde_json::{Value, json};

fn request() -> GaiseDecisionRequest {
    serde_json::from_value(json!({
        "model": "gpt-6-luna",
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

// Answers in the reference's order-preserving array shape, deliberately not in
// question order: GAISe matches them by name.
fn answer() -> Value {
    json!({"model": "gpt-6-luna", "answers": [
        {"type": "score", "name": "urgency", "score": 1.1, "probabilities": [
            {"value": 0, "label": "Routine", "probability": 0.1},
            {"value": 1, "label": "Soon", "probability": 0.7},
            {"value": 2, "label": "Urgent", "probability": 0.2}], "confidence": 0.55},
        {"type": "predicate", "name": "refund", "probability": 0.95},
        {"type": "choice", "name": "team", "choice": "billing", "probabilities": [
            {"value": "billing", "probability": 0.97}, {"value": "technical", "probability": 0.03}], "confidence": 0.93}
    ], "usage": {"input_tokens": 42, "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
        "output_tokens": 0, "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 42}})
}

async fn server(app: Router) -> (GaiseClientOpenAI, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (GaiseClientOpenAI::new(url, "sk-test".into()), task)
}

#[test]
fn request_maps_questions_to_named_predicate_choice_and_score() {
    let body = decision_request(&request()).unwrap();
    assert_eq!(
        body,
        json!({
            "model": "gpt-6-luna",
            "input": "{\"ticket\":\"I was charged twice. Please refund the extra payment.\"}",
            "questions": [
                {"type": "predicate", "name": "refund", "instructions": "Does the customer ask for a refund?\nTrue when: A refund is requested\nFalse when: No refund is requested"},
                {"type": "choice", "name": "team", "instructions": "Which team?", "choices": [
                    {"value": "billing", "description": "Payments and refunds"}, {"value": "technical"}]},
                {"type": "score", "name": "urgency", "instructions": "How urgent?", "levels": [
                    {"label": "Routine"}, {"label": "Soon", "description": "Within a day"}, {"label": "Urgent"}]}
            ]
        }),
        "no correlation_id, connection, or safety_identifier is sent"
    );

    let mut text_state = request();
    text_state.state = json!("plain text");
    assert_eq!(
        decision_request(&text_state).unwrap()["input"],
        "plain text"
    );
}

#[test]
fn request_rejects_what_the_decisions_api_cannot_take() {
    let mut no_state = request();
    no_state.state = Value::Null;
    assert!(decision_request(&no_state).is_err());

    let mut no_instructions = request();
    no_instructions.questions.insert(
        "bare".into(),
        GaiseQuestion::Noul {
            instructions: None,
            criteria: None,
        },
    );
    let error = decision_request(&no_instructions).unwrap_err().to_string();
    assert!(
        error.contains("bare") && error.contains("instructions"),
        "{error}"
    );

    let mut one_choice = request();
    one_choice.questions.insert(
        "single".into(),
        GaiseQuestion::Choice {
            instructions: Some(json!("Pick")),
            criteria: [("only".to_string(), Value::Null)].into(),
        },
    );
    let error = decision_request(&one_choice).unwrap_err().to_string();
    assert!(error.contains("between 2 and 255"), "{error}");
}

fn png() -> GaiseContent {
    GaiseContent::Image {
        data: vec![137, 80, 78, 71],
        format: Some("png".into()),
    }
}

#[test]
fn images_follow_the_state_text_as_data_urls_in_one_user_message() {
    let body = decision_request_with_images(&request(), &[png()]).unwrap();
    assert_eq!(
        body["input"],
        json!([{"type": "message", "role": "user", "content": [
            {"type": "input_text", "text": "{\"ticket\":\"I was charged twice. Please refund the extra payment.\"}"},
            {"type": "input_image", "image_url": "data:image/png;base64,iVBORw=="}
        ]}])
    );
    assert_eq!(
        body["questions"],
        decision_request(&request()).unwrap()["questions"]
    );

    let text = GaiseContent::Text {
        text: "not an image".into(),
    };
    assert!(decision_request_with_images(&request(), &[text]).is_err());
    let too_many = vec![png(); 129];
    let error = decision_request_with_images(&request(), &too_many)
        .unwrap_err()
        .to_string();
    assert!(error.contains("128"), "{error}");
}

#[test]
fn usage_keeps_the_decision_vocabulary_and_the_reported_total() {
    let usage = map_decision_usage(&answer()["usage"]).unwrap();
    let input = usage.input.unwrap();
    assert_eq!(input["input_tokens"], 42);
    assert_eq!(input["cached_tokens"], 0);
    assert_eq!(input["cache_write_tokens"], 0);
    let output = usage.output.unwrap();
    assert_eq!(output["output_tokens"], 0);
    assert_eq!(output["reasoning_tokens"], 0);
    assert_eq!(usage.total.unwrap()["total_tokens"], 42);
}

#[tokio::test]
async fn decision_posts_to_v1_decisions_and_maps_answers_by_name() {
    let app = Router::new().route(
        "/v1/decisions",
        post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert_eq!(headers["authorization"], "Bearer sk-test");
            assert_eq!(body, decision_request(&request()).unwrap());
            Json(answer())
        }),
    );
    let (client, task) = server(app).await;
    let response = client.decision(&request()).await.unwrap();
    assert_eq!(response.model, "gpt-6-luna");
    assert_eq!(response.answers["refund"], GaiseAnswer::Noul { noul: 0.95 });
    assert!(matches!(
        &response.answers["team"],
        GaiseAnswer::Choice { choice, probabilities, confidence }
            if choice == "billing" && probabilities["technical"] == 0.03 && *confidence == 0.93
    ));
    assert!(matches!(
        &response.answers["urgency"],
        GaiseAnswer::Score { score, legend, probabilities, confidence }
            if *score == 1.1
                && legend["0"] == "Routine"
                && legend["1"] == json!({"label": "Soon", "description": "Within a day"})
                && probabilities["1"] == 0.7
                && *confidence == 0.55
    ));
    assert_eq!(response.usage.unwrap().total.unwrap()["total_tokens"], 42);
    task.abort();
}

#[tokio::test]
async fn decision_with_images_posts_the_image_message() {
    let app = Router::new().route(
        "/v1/decisions",
        post(|Json(body): Json<Value>| async move {
            assert_eq!(
                body,
                decision_request_with_images(&request(), &[png()]).unwrap()
            );
            Json(answer())
        }),
    );
    let (client, task) = server(app).await;
    let response = client
        .decision_with_images(&request(), &[png()])
        .await
        .unwrap();
    assert_eq!(response.answers.len(), 3);
    task.abort();
}

#[tokio::test]
async fn decision_surfaces_refusals_errors_and_mismatched_answers() {
    let app = Router::new().route(
        "/v1/decisions",
        post(|Json(body): Json<Value>| async move {
            let mut reply = answer();
            match body["model"].as_str() {
                Some("missing") => {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(json!({"error": {"message": "The model `missing` does not exist", "type": "invalid_request_error"}})),
                    );
                }
                Some("refuse") => reply["answers"][1] = json!({"type": "refusal", "name": "refund"}),
                _ => reply["answers"][1] = json!({"type": "choice", "name": "refund", "choice": "yes", "probabilities": [{"value": "yes", "probability": 1.0}, {"value": "no", "probability": 0.0}], "confidence": 1.0}),
            }
            (StatusCode::OK, Json(reply))
        }),
    );
    let (client, task) = server(app).await;

    let mut missing = request();
    missing.model = "missing".into();
    let error = client.decision(&missing).await.unwrap_err().to_string();
    assert!(
        error.contains("404") && error.contains("does not exist"),
        "{error}"
    );

    let mut refuse = request();
    refuse.model = "refuse".into();
    let error = client.decision(&refuse).await.unwrap_err().to_string();
    assert!(
        error.contains("refused") && error.contains("refund"),
        "{error}"
    );

    let error = client.decision(&request()).await.unwrap_err().to_string();
    assert!(error.contains("do not match the questions"), "{error}");

    let mut empty = request();
    empty.questions.clear();
    assert!(client.decision(&empty).await.is_err(), "validated locally");
    task.abort();
}

#[tokio::test]
#[ignore = "calls the OpenAI API; needs OPENAI_API_KEY"]
async fn live_decision() {
    let client = GaiseClientOpenAI::new(
        "https://api.openai.com/v1".into(),
        std::env::var("OPENAI_API_KEY").unwrap(),
    );
    let response = client.decision(&request()).await.unwrap();
    assert!(
        response.model.starts_with("gpt-6-luna"),
        "{}",
        response.model
    );
    request().check_answers(&response.answers).unwrap();
}
