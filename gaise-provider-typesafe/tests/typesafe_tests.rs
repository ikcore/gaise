use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use gaise_core::{GaiseClient, contracts::*};
use gaise_provider_typesafe::GaiseClientTypeSafe;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn request() -> GaiseDecisionRequest {
    serde_json::from_value(json!({
        "model": "jev", "state": {"ticket": "Charged twice"},
        "correlation_id": "local-only",
        "connection": {"api_key": "never-forward-this"},
        "questions": {
            "urgent": {"type": "noul", "instructions": "Urgent?", "criteria": {"true": "Yes", "false": "No"}},
            "team": {"type": "choice", "instructions": {"task": "Route"}, "criteria": {"billing": null, "tech": ["Technical"]}},
            "severity": {"type": "score", "criteria": [null, "High"]}
        }
    })).unwrap()
}

fn answer() -> Value {
    json!({"model": "jev-1.13.0", "answers": {
        "urgent": {"type": "noul", "noul": 0.93},
        "team": {"type": "choice", "choice": "billing", "probabilities": {"billing": 0.9, "tech": 0.1}, "confidence": 0.7},
        "severity": {"type": "score", "score": 0.75, "probabilities": {"0": 0.25, "1": 0.75}, "legend": {"0": null, "1": "High"}, "confidence": 0.4}
    }, "usage": {"input_tokens": 123, "output_tokens": 0}})
}

async fn server(app: Router) -> (GaiseClientTypeSafe, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (GaiseClientTypeSafe::new(url, "test-key".into()), task)
}

#[tokio::test]
async fn maps_mixed_questions_answers_usage_and_catalog_over_http() {
    let app = Router::new().route("/v1/systemone", post(|headers: HeaderMap, Json(body): Json<Value>| async move {
        assert_eq!(headers["authorization"], "Bearer test-key");
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(body.as_object().unwrap().len(), 3);
        assert_eq!(body["model"], "jev-latest");
        assert_eq!(body["state"], request().state);
        assert_eq!(body["questions"], serde_json::to_value(request().questions).unwrap());
        Json(answer())
    })).route("/v1/models", get(|headers: HeaderMap| async move {
        assert_eq!(headers["authorization"], "Bearer test-key");
        Json(json!({"models": [{"name": "jev-latest", "description": "Decisions", "release_date": "2026-09-15", "extra": 42}]}))
    }));
    let (client, task) = server(app).await;
    let response = client.decision(&request()).await.unwrap();
    assert_eq!(response.model, "jev-1.13.0");
    assert_eq!(
        serde_json::to_value(response.answers).unwrap(),
        answer()["answers"]
    );
    let usage = response.usage.unwrap();
    assert_eq!(usage.input.unwrap()["input_tokens"], 123);
    assert_eq!(usage.output.unwrap()["output_tokens"], 0);
    assert!(usage.total.is_none());
    let models = client
        .list_models(&GaiseListModelsRequest {
            include_raw: true,
            operation: Some(GaiseOperation::Decision),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(models.models[0].id, "jev");
    assert_eq!(
        models.models[0].created_at.as_deref(),
        Some("2026-09-15T00:00:00Z")
    );
    assert_eq!(models.models[0].raw.as_ref().unwrap()["extra"], 42);
    let models = client
        .list_models(&GaiseListModelsRequest {
            operation: Some(GaiseOperation::Instruct),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(models.models.is_empty());
    task.abort();
}

#[tokio::test]
async fn retries_overload_and_preserves_non_retryable_status_without_key_leak() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let app = Router::new().route(
        "/v1/systemone",
        post(move || {
            let count = count.clone();
            async move {
                let attempt = count.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    (
                        StatusCode::from_u16(529).unwrap(),
                        [("retry-after", "0")],
                        Json(json!({"error": "overloaded"})),
                    )
                } else {
                    (StatusCode::OK, [("retry-after", "0")], Json(answer()))
                }
            }
        }),
    );
    let (client, task) = server(app).await;
    client.decision(&request()).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    task.abort();
    let (client, task) = server(Router::new().route(
        "/v1/systemone",
        post(|| async { (StatusCode::UNAUTHORIZED, "invalid test-key") }),
    ))
    .await;
    let error = client.decision(&request()).await.unwrap_err().to_string();
    assert!(error.contains("401"));
    assert!(error.contains("invalid ***"));
    assert!(!error.contains("test-key"));
    task.abort();
}

#[tokio::test]
async fn rejects_invalid_and_mismatched_responses_and_unsupported_operations() {
    for body in [
        json!({"model": "x", "answers": {}, "usage": {"input_tokens": 1, "output_tokens": 0}}),
        json!({"unexpected": true}),
    ] {
        let (client, task) = server(Router::new().route(
            "/v1/systemone",
            post(move || {
                let body = body.clone();
                async move { Json(body) }
            }),
        ))
        .await;
        assert!(client.decision(&request()).await.is_err());
        task.abort();
    }
    let client = GaiseClientTypeSafe::new("http://127.0.0.1:1".into(), "key".into());
    assert!(
        client
            .instruct(&GaiseInstructRequest::default())
            .await
            .unwrap_err()
            .to_string()
            .contains("use decision")
    );
    assert!(
        client
            .instruct_stream(&GaiseInstructRequest::default())
            .await
            .is_err()
    );
    assert!(
        client
            .embeddings(&GaiseEmbeddingsRequest::default())
            .await
            .is_err()
    );
}

fn png() -> GaiseContent {
    GaiseContent::Image {
        data: vec![0x89, b'P', b'N', b'G'],
        format: None,
    }
}

fn jpeg() -> GaiseContent {
    GaiseContent::Image {
        data: vec![0xFF, 0xD8, 0xFF, 0xE0],
        format: Some("image/jpeg".into()),
    }
}

// TypeSafe images (2026-10-14): a top-level `assets` map of base64 PNG/JPEG
// keyed by references that must appear in the request, behind an opt-in header.
#[tokio::test]
async fn decision_with_images_lists_references_in_state_and_sends_assets() {
    let app = Router::new().route(
        "/v1/systemone",
        post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert_eq!(headers["typesafe-danger-zone-images"], "true");
            assert!(!headers.contains_key("typesafe-danger-zone-long-context"));
            assert_eq!(body.as_object().unwrap().len(), 4);
            assert_eq!(body["model"], "jev-latest");
            assert_eq!(body["state"]["ticket"], "Charged twice");
            let refs = body["state"]["images"].as_array().unwrap().clone();
            assert_eq!(refs.len(), 2);
            let assets = body["assets"].as_object().unwrap();
            assert_eq!(assets.len(), 2);
            assert_eq!(
                assets[refs[0].as_str().unwrap()],
                json!({"type": "image", "format": "png", "data_base64": "iVBORw=="})
            );
            assert_eq!(
                assets[refs[1].as_str().unwrap()],
                json!({"type": "image", "format": "jpg", "data_base64": "/9j/4A=="})
            );
            Json(answer())
        }),
    );
    let (client, task) = server(app).await;
    let response = client
        .decision_with_images(&request(), &[png(), jpeg()])
        .await
        .unwrap();
    assert_eq!(response.model, "jev-1.13.0");
    task.abort();

    // A non-object state is wrapped so the original stays reachable.
    let app = Router::new().route(
        "/v1/systemone",
        post(|Json(body): Json<Value>| async move {
            assert_eq!(body["state"]["state"], "Charged twice");
            let reference = body["state"]["images"][0].as_str().unwrap().to_string();
            assert!(body["assets"][&reference].is_object());
            Json(answer())
        }),
    );
    let (client, task) = server(app).await;
    let mut text_state = request();
    text_state.state = json!("Charged twice");
    client
        .decision_with_images(&text_state, &[png()])
        .await
        .unwrap();
    task.abort();
}

#[tokio::test]
async fn decision_with_assets_keeps_caller_references_and_opt_in_headers() {
    let app = Router::new().route(
        "/v1/systemone",
        post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert_eq!(headers["typesafe-danger-zone-images"], "true");
            assert_eq!(headers["typesafe-danger-zone-long-context"], "true");
            assert_eq!(body["model"], "jev-preview");
            assert_eq!(body["state"], json!({"door": "door-photo"}));
            assert_eq!(body["assets"]["door-photo"]["format"], "png");
            Json(answer())
        }),
    );
    let (client, task) = server(app).await;
    let client = client.with_long_context(true);
    let mut preview = request();
    preview.model = "jev-preview".into();
    preview.state = json!({"door": "door-photo"});
    let assets = std::collections::BTreeMap::from([("door-photo".to_string(), png())]);
    client
        .decision_with_assets(&preview, &assets)
        .await
        .unwrap();
    task.abort();

    // Without images the request is unchanged: no assets, no images header.
    let app = Router::new().route(
        "/v1/systemone",
        post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert!(!headers.contains_key("typesafe-danger-zone-images"));
            assert_eq!(body.as_object().unwrap().len(), 3);
            Json(answer())
        }),
    );
    let (client, task) = server(app).await;
    client.decision_with_images(&request(), &[]).await.unwrap();
    task.abort();
}

#[tokio::test]
async fn rejects_unsupported_images_before_sending() {
    let client = GaiseClientTypeSafe::new("http://127.0.0.1:1".into(), "key".into());
    let gif = GaiseContent::Image {
        data: b"GIF89a".to_vec(),
        format: Some("gif".into()),
    };
    let unknown = GaiseContent::Image {
        data: b"RIFF".to_vec(),
        format: None,
    };
    let text = GaiseContent::Text { text: "no".into() };
    for image in [gif, unknown, text] {
        let error = client
            .decision_with_images(&request(), &[image])
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("TypeSafe"), "{error}");
    }
    let blank = std::collections::BTreeMap::from([(" ".to_string(), png())]);
    assert!(
        client
            .decision_with_assets(&request(), &blank)
            .await
            .is_err()
    );
}

#[test]
fn validates_request_shapes() {
    let mut req = request();
    req.validate().unwrap();
    req.questions.insert(
        "severity".into(),
        GaiseQuestion::Score {
            instructions: None,
            criteria: vec![json!("only")],
        },
    );
    assert!(req.validate().unwrap_err().contains("two ordered"));
    req.questions.clear();
    assert!(req.validate().is_err());
    let mut req = request();
    req.state = json!(42);
    assert!(req.validate().is_err());
    req.state = Value::Null;
    req.validate().unwrap();
    assert!(serde_json::from_value::<GaiseQuestion>(json!({"type": "unknown"})).is_err());
    assert!(
        serde_json::from_value::<GaiseQuestion>(
            json!({"type": "score", "criteria": {"0": "wrong shape"}})
        )
        .is_err()
    );
}
