#![cfg(feature = "live")]
//! Parallel function calls in a speech-to-speech session: `response.create`
//! must follow the last `function_call_output`, never an earlier one.

use futures_util::{SinkExt, StreamExt};
use gaise_core::GaiseLiveClient;
use gaise_core::contracts::*;
use gaise_provider_xai::xai_live_client::*;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

#[test]
fn observer_releases_only_after_every_parallel_output() {
    let mut gate = LiveToolCallGate::new();
    assert!(!observe_tool_calls(
        &mut gate,
        &json!({"type": "response.created"})
    ));
    for id in ["call_a", "call_b"] {
        assert!(!observe_tool_calls(
            &mut gate,
            &json!({"type": "response.function_call_arguments.done", "call_id": id, "name": "f"})
        ));
    }
    assert!(!observe_tool_calls(
        &mut gate,
        &json!({"type": "response.done"})
    ));
    assert!(!gate.output_submitted("call_a"));
    assert!(gate.output_submitted("call_b"));
}

#[test]
fn observer_releases_on_response_done_when_outputs_came_early() {
    let mut gate = LiveToolCallGate::new();
    observe_tool_calls(&mut gate, &json!({"type": "response.created"}));
    observe_tool_calls(
        &mut gate,
        &json!({"type": "response.function_call_arguments.done", "call_id": "a"}),
    );
    assert!(!gate.output_submitted("a"), "response still streaming");
    assert!(observe_tool_calls(
        &mut gate,
        &json!({"type": "response.done"})
    ));
}

#[tokio::test]
async fn session_sends_one_response_create_after_all_parallel_outputs() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (frames_tx, mut frames_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let send = |v: Value| Message::Text(v.to_string().into());
        ws.send(send(json!({"type": "conversation.created"})))
            .await
            .unwrap();
        ws.send(send(json!({"type": "session.created"})))
            .await
            .unwrap();
        // session.update
        let _ = ws.next().await;
        ws.send(send(json!({"type": "session.updated"})))
            .await
            .unwrap();
        // One response issuing two calls in parallel.
        ws.send(send(json!({"type": "response.created"})))
            .await
            .unwrap();
        for id in ["call_a", "call_b"] {
            ws.send(send(json!({
                "type": "response.function_call_arguments.done",
                "call_id": id, "name": "lookup", "arguments": "{}"
            })))
            .await
            .unwrap();
        }
        ws.send(send(json!({"type": "response.done", "response": {}})))
            .await
            .unwrap();
        while let Some(Ok(msg)) = ws.next().await {
            if let Message::Text(text) = msg {
                let frame: Value = serde_json::from_str(&text).unwrap();
                let done = frame["type"] == "response.create";
                let _ = frames_tx.send(frame);
                if done {
                    break;
                }
            }
        }
    });

    let client = GaiseClientXaiLive::new(format!("http://{addr}/v1"), "test-key".into());
    let mut session = client
        .live_connect(&GaiseLiveConfig {
            model: "grok-voice-3.0".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    // Wait for both tool calls before answering.
    let mut calls = Vec::new();
    while calls.len() < 2 {
        match session.rx.next().await.unwrap().unwrap() {
            GaiseLiveEvent::ToolCall { id, .. } => calls.push(id),
            _ => continue,
        }
    }
    for id in &calls {
        session
            .tx
            .send(GaiseLiveInput::ToolResponse {
                call_id: id.clone(),
                name: "lookup".into(),
                result: json!({"ok": true}),
            })
            .await
            .unwrap();
    }

    let mut types = Vec::new();
    while let Some(frame) =
        tokio::time::timeout(std::time::Duration::from_secs(5), frames_rx.recv())
            .await
            .unwrap()
    {
        let kind = frame["type"].as_str().unwrap().to_string();
        let item_kind = frame["item"]["type"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        types.push(if item_kind.is_empty() {
            kind.clone()
        } else {
            item_kind
        });
        if kind == "response.create" {
            break;
        }
    }
    assert_eq!(
        types,
        vec![
            "function_call_output",
            "function_call_output",
            "response.create"
        ],
        "exactly one response.create, after both outputs"
    );
    server.await.unwrap();
}
