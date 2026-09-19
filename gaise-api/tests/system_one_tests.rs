use axum::{
    Json, Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode},
    routing::{get, post},
};
use gaise_api::{AppState, create_app};
use gaise_client::{GaiseClientConfig, GaiseClientService};
use gaise_core::{GaiseClient, contracts::*, logging::IGaiseLogger};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[derive(Debug, Default)]
struct Logger(Mutex<Vec<Value>>);
impl IGaiseLogger for Logger {
    fn log_request(&self, _: Option<&str>, _: &str, _: &str, value: Value) {
        self.0.lock().unwrap().push(value);
    }
    fn log_response(&self, _: Option<&str>, _: &str, _: &str, _: Value, _: Option<Value>) {}
    fn log_stream_chunk(&self, _: Option<&str>, _: &str, _: &str, _: Value) {}
}

#[tokio::test]
async fn api_routes_typesafe_with_overrides_catalog_and_redacted_logging() {
    let upstream = Router::new().route("/v1/systemone", post(|headers: HeaderMap, Json(body): Json<Value>| async move {
        assert_eq!(headers["authorization"], "Bearer override-key");
        assert_eq!(body, json!({"model": "jev-latest", "state": "Delivered today", "questions": {"delivered": {"type": "noul", "instructions": "Delivered?"}}}));
        Json(json!({"model": "jev-1.13.0", "answers": {"delivered": {"type": "noul", "noul": 0.98}}, "usage": {"input_tokens": 10, "output_tokens": 0}}))
    })).route("/v1/models", get(|| async { Json(json!({"models": [{"name": "jev-latest", "description": "Jev", "release_date": "2026-09-15"}]})) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, upstream).await.unwrap();
    });
    let logger = Arc::new(Logger::default());
    let service = GaiseClientService::new(GaiseClientConfig {
        typesafe_api_url: Some(url.clone()),
        typesafe_api_key: Some("configured-key".into()),
        logger: Some(logger.clone()),
        ..Default::default()
    });
    assert!(
        service
            .configured_providers()
            .await
            .contains(&"typesafe".into())
    );
    let connection = GaiseConnection {
        api_url: Some(url.clone()),
        api_key: Some("override-key".into()),
        ..Default::default()
    };
    let a = service
        .get_client_with("typesafe", Some(&connection))
        .await
        .unwrap();
    let b = service
        .get_client_with("typesafe", Some(&connection))
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert!(!Arc::ptr_eq(
        &a,
        &service.get_client("typesafe").await.unwrap()
    ));
    let app = create_app(Arc::new(AppState {
        client_service: service,
    }));
    let body = json!({"model": "typesafe::jev", "state": "Delivered today", "questions": {"delivered": {"type": "noul", "instructions": "Delivered?"}}, "connection": connection, "correlation_id": "trace-1"});
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/systemone")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["answers"]["delivered"]["noul"], 0.98);
    assert_eq!(body["usage"]["input"]["input_tokens"], 10);
    assert_eq!(body["usage"]["output"]["output_tokens"], 0);
    {
        let logs = logger.0.lock().unwrap();
        assert_eq!(logs[0]["connection"]["api_key"], "***");
        assert_eq!(logs[0]["correlation_id"], "trace-1");
    }
    let response = app
        .clone()
        .oneshot(
            Request::get("/v1/models?provider=typesafe&operation=system_one")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["models"][0]["id"], "typesafe::jev");
    assert_eq!(
        body["models"][0]["capabilities"]["operations"],
        json!(["system_one"])
    );
    let response = app
        .oneshot(
            Request::post("/v1/systemone")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"model": "typesafe::jev", "state": "x", "questions": {}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    task.abort();
}

#[tokio::test]
async fn missing_credentials_fail_and_connection_can_supply_them() {
    let service = GaiseClientService::new(GaiseClientConfig::default());
    assert!(
        service
            .get_client("typesafe")
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("API Key not configured")
    );
    let connection = GaiseConnection {
        api_key: Some("key".into()),
        ..Default::default()
    };
    assert!(
        service
            .get_client_with("typesafe", Some(&connection))
            .await
            .is_ok()
    );
    let request: GaiseSystemOneRequest = serde_json::from_value(
        json!({"model": "jev-latest", "state": "x", "questions": {"q": {"type": "noul"}}}),
    )
    .unwrap();
    assert!(
        service
            .system_one(&request)
            .await
            .unwrap_err()
            .to_string()
            .contains("provider::model")
    );
}
