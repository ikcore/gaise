//! Responses API path for GPT-6 Astra and GPT-6.1 Sol function tools.
//!
//! Sources (audited 2026-09-30): the function-calling, reasoning, and
//! latest-model guides and the openai-python `types/responses` definitions.
//! Everything here is hermetic; the HTTP cases run against a local server.

use axum::{Json, Router, http::HeaderMap, routing::post};
use futures_util::StreamExt;
use gaise_core::GaiseClient;
use gaise_core::contracts::{
    GaiseContent, GaiseFunctionCall, GaiseGenerationConfig, GaiseInstructRequest, GaiseMessage,
    GaiseStreamAccumulator, GaiseTool, GaiseToolCall, OneOrMany,
};
use gaise_provider_openai::openai_client::GaiseClientOpenAI;
use gaise_provider_openai::responses::{
    REASONING_SIGNATURE_PREFIX, ResponsesStreamState, map_responses_event, map_responses_response,
    responses_request, uses_responses_api,
};
use serde_json::{Value, json};

fn user(text: &str) -> GaiseMessage {
    GaiseMessage {
        role: "user".into(),
        content: Some(OneOrMany::One(GaiseContent::Text { text: text.into() })),
        ..Default::default()
    }
}

fn request(model: &str, input: Vec<GaiseMessage>, tools: bool) -> GaiseInstructRequest {
    GaiseInstructRequest {
        model: model.into(),
        input: OneOrMany::Many(input),
        tools: tools.then(|| {
            vec![GaiseTool {
                name: "lookup".into(),
                description: Some("Look something up".into()),
                parameters: None,
            }]
        }),
        ..Default::default()
    }
}

fn reasoning_item() -> Value {
    json!({"type": "reasoning", "id": "rs_1", "summary": [{"type": "summary_text", "text": "Need the lookup tool."}], "encrypted_content": "gAAA"})
}

fn response_body() -> Value {
    json!({
        "id": "resp_1", "object": "response", "status": "completed",
        "output": [
            reasoning_item(),
            {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed", "content": [{"type": "output_text", "text": "Checking.", "annotations": []}]},
            {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "lookup", "arguments": "{\"q\":\"x\"}", "status": "completed"}
        ],
        "usage": {"input_tokens": 40, "input_tokens_details": {"cached_tokens": 8}, "output_tokens": 25, "output_tokens_details": {"reasoning_tokens": 12}, "total_tokens": 65}
    })
}

#[test]
fn only_tool_conversations_on_responses_only_tool_models_are_routed() {
    for model in ["gpt-6-astra", "gpt-6.1-sol", "ft:gpt-6-astra:acme::abc"] {
        assert!(
            uses_responses_api(&request(model, vec![user("hi")], true)),
            "{model}"
        );
        assert!(
            !uses_responses_api(&request(model, vec![user("hi")], false)),
            "{model}: tool-free requests stay on Chat Completions"
        );
    }
    // Models whose Chat Completions surface can call tools are never routed.
    for model in ["gpt-6-sol", "gpt-6-luna", "gpt-5.6", "gpt-4.1"] {
        assert!(
            !uses_responses_api(&request(model, vec![user("hi")], true)),
            "{model}"
        );
    }
    // A follow-up without `tools` still belongs to the tool conversation.
    let tool_result = GaiseMessage {
        role: "tool".into(),
        tool_call_id: Some("call_1".into()),
        content: Some(OneOrMany::One(GaiseContent::Text { text: "42".into() })),
        ..Default::default()
    };
    assert!(uses_responses_api(&request(
        "gpt-6-astra",
        vec![user("hi"), tool_result],
        false
    )));
}

#[test]
fn request_uses_the_flat_stateless_responses_shape() {
    let mut req = request("gpt-6.1-sol", vec![user("hi")], true);
    req.generation_config = Some(GaiseGenerationConfig {
        temperature: Some(0.3),
        top_p: Some(0.8),
        max_tokens: Some(2048),
        thinking_effort: Some("none".into()),
        include_thoughts: Some(true),
        ..Default::default()
    });
    let json = serde_json::to_value(responses_request(&req)).unwrap();
    assert_eq!(json["model"], "gpt-6.1-sol");
    assert_eq!(json["store"], false);
    assert_eq!(json["include"], json!(["reasoning.encrypted_content"]));
    assert_eq!(json["max_output_tokens"], 2048);
    assert_eq!(
        json["reasoning"],
        json!({"effort": "low", "summary": "auto"}),
        "none is not accepted by GPT-6.1 Sol"
    );
    assert!(json.get("temperature").is_none() && json.get("top_p").is_none());
    assert!(json.get("messages").is_none());
    assert_eq!(
        json["input"],
        json!([{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}])
    );
    let tool = &json["tools"][0];
    assert_eq!(tool["type"], "function");
    assert_eq!(tool["name"], "lookup");
    assert_eq!(tool["strict"], false);
    assert!(tool.get("function").is_none(), "Responses tools are flat");
    assert_eq!(tool["parameters"]["type"], "object");

    // No effort configured: let OpenAI apply the model default.
    let json = serde_json::to_value(responses_request(&request(
        "gpt-6-astra",
        vec![user("hi")],
        true,
    )))
    .unwrap();
    assert!(json.get("reasoning").is_none());
}

#[test]
fn response_maps_text_tool_calls_reasoning_and_usage() {
    let response = map_responses_response(&response_body()).unwrap();
    assert_eq!(response.external_id.as_deref(), Some("resp_1"));
    let OneOrMany::Many(messages) = &response.output else {
        panic!("expected a message list");
    };
    let message = &messages[0];
    assert_eq!(message.role, "assistant");
    let Some(OneOrMany::Many(content)) = &message.content else {
        panic!("expected reasoning and text");
    };
    let GaiseContent::Reasoning { text, signature } = &content[0] else {
        panic!("reasoning first");
    };
    assert_eq!(text, "Need the lookup tool.");
    let replay: Value = serde_json::from_str(
        signature
            .as_deref()
            .unwrap()
            .strip_prefix(REASONING_SIGNATURE_PREFIX)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(replay, reasoning_item());
    assert_eq!(
        content[1],
        GaiseContent::Text {
            text: "Checking.".into()
        }
    );
    let call = &message.tool_calls.as_ref().unwrap()[0];
    assert_eq!(call.id, "call_1", "the call_id links the tool result");
    assert_eq!(call.function.name, "lookup");
    assert_eq!(call.function.arguments.as_deref(), Some("{\"q\":\"x\"}"));

    // Same key vocabulary as the Chat Completions path.
    let usage = response.usage.unwrap();
    assert_eq!(usage.input.as_ref().unwrap()["prompt_tokens"], 40);
    assert_eq!(usage.input.as_ref().unwrap()["cached_tokens"], 8);
    assert_eq!(usage.output.as_ref().unwrap()["completion_tokens"], 25);
    assert_eq!(usage.output.as_ref().unwrap()["reasoning_tokens"], 12);
    assert_eq!(usage.total.as_ref().unwrap()["total_tokens"], 65);

    let failed = json!({"status": "failed", "error": {"code": "server_error", "message": "boom"}});
    assert!(
        map_responses_response(&failed)
            .unwrap_err()
            .contains("boom")
    );
}

#[test]
fn tool_round_trip_replays_reasoning_call_and_output_in_order() {
    let first = map_responses_response(&response_body()).unwrap();
    let OneOrMany::Many(mut messages) = first.output else {
        panic!("expected a message list");
    };
    let assistant = messages.remove(0);
    let tool_result = GaiseMessage {
        role: "tool".into(),
        tool_call_id: Some("call_1".into()),
        content: Some(OneOrMany::One(GaiseContent::Text { text: "42".into() })),
        ..Default::default()
    };
    let system = GaiseMessage {
        role: "system".into(),
        content: Some(OneOrMany::One(GaiseContent::Text {
            text: "Be brief.".into(),
        })),
        ..Default::default()
    };
    let json = serde_json::to_value(responses_request(&request(
        "gpt-6-astra",
        vec![system, user("hi"), assistant, tool_result],
        true,
    )))
    .unwrap();
    assert_eq!(
        json["input"],
        json!([
            {"role": "system", "content": [{"type": "input_text", "text": "Be brief."}]},
            {"role": "user", "content": [{"type": "input_text", "text": "hi"}]},
            reasoning_item(),
            {"role": "assistant", "content": "Checking."},
            {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"q\":\"x\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "42"}
        ])
    );

    // Reasoning from another provider is dropped rather than sent as an item.
    let foreign = GaiseMessage {
        role: "assistant".into(),
        content: Some(OneOrMany::One(GaiseContent::Reasoning {
            text: "thinking".into(),
            signature: Some("anthropic-signature".into()),
        })),
        tool_calls: Some(vec![GaiseToolCall {
            id: "call_2".into(),
            r#type: "function".into(),
            function: GaiseFunctionCall {
                name: "lookup".into(),
                arguments: None,
            },
            thought_signature: None,
        }]),
        ..Default::default()
    };
    let json = serde_json::to_value(responses_request(&request(
        "gpt-6-astra",
        vec![foreign],
        true,
    )))
    .unwrap();
    assert_eq!(
        json["input"],
        json!([{"type": "function_call", "call_id": "call_2", "name": "lookup", "arguments": "{}"}])
    );
}

fn stream_events() -> Vec<Value> {
    vec![
        json!({"type": "response.created", "response": {"id": "resp_1", "status": "in_progress"}}),
        json!({"type": "response.output_item.added", "output_index": 0, "item": {"type": "reasoning", "id": "rs_1", "summary": []}}),
        json!({"type": "response.output_item.done", "output_index": 0, "item": reasoning_item()}),
        json!({"type": "response.output_item.added", "output_index": 1, "item": {"type": "message", "id": "msg_1", "role": "assistant", "content": []}}),
        json!({"type": "response.output_text.delta", "item_id": "msg_1", "output_index": 1, "content_index": 0, "delta": "Check"}),
        json!({"type": "response.output_text.delta", "item_id": "msg_1", "output_index": 1, "content_index": 0, "delta": "ing."}),
        json!({"type": "response.output_item.added", "output_index": 2, "item": {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "lookup", "arguments": ""}}),
        json!({"type": "response.function_call_arguments.delta", "item_id": "fc_1", "output_index": 2, "delta": "{\"q\":"}),
        json!({"type": "response.function_call_arguments.delta", "item_id": "fc_1", "output_index": 2, "delta": "\"x\"}"}),
        json!({"type": "response.function_call_arguments.done", "item_id": "fc_1", "output_index": 2, "arguments": "{\"q\":\"x\"}"}),
        json!({"type": "response.output_item.added", "output_index": 3, "item": {"type": "function_call", "id": "fc_2", "call_id": "call_2", "name": "lookup", "arguments": ""}}),
        json!({"type": "response.function_call_arguments.delta", "item_id": "fc_2", "output_index": 3, "delta": "{}"}),
        json!({"type": "response.completed", "response": {"id": "resp_1", "status": "completed", "usage": response_body()["usage"]}}),
    ]
}

fn assert_streamed_turn(acc: &GaiseStreamAccumulator) {
    assert_eq!(acc.external_id.as_deref(), Some("resp_1"));
    assert_eq!(acc.text, "Checking.");
    assert!(matches!(
        &acc.content_parts[0],
        GaiseContent::Reasoning { signature: Some(s), .. } if s.starts_with(REASONING_SIGNATURE_PREFIX)
    ));
    assert_eq!(acc.tool_calls.len(), 2);
    assert_eq!(acc.tool_calls[&0].id, "call_1");
    assert_eq!(acc.tool_calls[&0].function.name, "lookup");
    assert_eq!(
        acc.tool_calls[&0].function.arguments.as_deref(),
        Some("{\"q\":\"x\"}")
    );
    assert_eq!(acc.tool_calls[&1].id, "call_2");
    assert_eq!(acc.tool_calls[&1].function.arguments.as_deref(), Some("{}"));
    let usage = acc.usage.as_ref().unwrap();
    assert_eq!(usage.input.as_ref().unwrap()["prompt_tokens"], 40);
    assert_eq!(usage.output.as_ref().unwrap()["completion_tokens"], 25);
}

#[test]
fn stream_events_assemble_into_the_same_turn() {
    let mut state = ResponsesStreamState::default();
    let mut acc = GaiseStreamAccumulator::new();
    for event in stream_events() {
        for mapped in map_responses_event(&mut state, &event) {
            acc.push(&mapped.unwrap());
        }
    }
    assert_streamed_turn(&acc);

    let mut state = ResponsesStreamState::default();
    let error = json!({"type": "error", "code": "server_error", "message": "stream broke"});
    assert!(
        map_responses_event(&mut state, &error)[0]
            .as_ref()
            .unwrap_err()
            .contains("stream broke")
    );
    let failed = json!({"type": "response.failed", "response": {"id": "resp_2", "status": "failed", "error": {"message": "bad"}}});
    assert!(
        map_responses_event(&mut state, &failed)[0]
            .as_ref()
            .unwrap_err()
            .contains("bad")
    );
}

async fn server(app: Router) -> (GaiseClientOpenAI, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (GaiseClientOpenAI::new(url, "test-key".into()), task)
}

#[tokio::test]
async fn instruct_sends_tool_requests_to_responses_and_the_rest_to_chat() {
    let app = Router::new()
        .route(
            "/v1/responses",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                assert_eq!(headers["authorization"], "Bearer test-key");
                assert_eq!(body["stream"], false);
                assert_eq!(body["tools"][0]["name"], "lookup");
                Json(response_body())
            }),
        )
        .route(
            "/v1/chat/completions",
            post(|Json(body): Json<Value>| async move {
                assert!(body.get("tools").is_none());
                Json(json!({"id": "chat_1", "object": "chat.completion", "created": 1, "model": "gpt-6-astra", "choices": [{"index": 0, "message": {"role": "assistant", "content": "hello"}, "finish_reason": "stop"}]}))
            }),
        );
    let (client, task) = server(app).await;

    let with_tools = client
        .instruct(&request("gpt-6-astra", vec![user("hi")], true))
        .await
        .unwrap();
    assert_eq!(with_tools.external_id.as_deref(), Some("resp_1"));
    let OneOrMany::Many(messages) = with_tools.output else {
        panic!("expected a message list");
    };
    assert_eq!(messages[0].tool_calls.as_ref().unwrap()[0].id, "call_1");

    let without_tools = client
        .instruct(&request("gpt-6-astra", vec![user("hi")], false))
        .await
        .unwrap();
    assert_eq!(without_tools.external_id.as_deref(), Some("chat_1"));
    task.abort();
}

#[tokio::test]
async fn instruct_stream_parses_responses_sse_and_surfaces_http_errors() {
    let app = Router::new().route(
        "/v1/responses",
        post(|Json(body): Json<Value>| async move {
            if body["model"] == "gpt-6.1-sol" {
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    "{\"error\":{\"message\":\"bad tool schema\"}}".to_string(),
                );
            }
            assert_eq!(body["stream"], true);
            let sse: String = stream_events()
                .iter()
                .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
                .collect();
            (axum::http::StatusCode::OK, sse)
        }),
    );
    let (client, task) = server(app).await;

    let mut stream = client
        .instruct_stream(&request("gpt-6-astra", vec![user("hi")], true))
        .await
        .unwrap();
    let mut acc = GaiseStreamAccumulator::new();
    while let Some(event) = stream.next().await {
        acc.push(&event.unwrap());
    }
    assert_streamed_turn(&acc);

    let error = client
        .instruct_stream(&request("gpt-6.1-sol", vec![user("hi")], true))
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("bad tool schema"), "{error}");
    task.abort();
}

/// Live two-turn tool call against the real API, streamed then unstreamed.
/// `OPENAI_LIVE_MODEL` overrides the model (default `gpt-6.1-sol`).
#[tokio::test]
#[ignore = "calls the OpenAI API; needs OPENAI_API_KEY"]
async fn live_tool_round_trip() {
    let client = GaiseClientOpenAI::new(
        "https://api.openai.com/v1".into(),
        std::env::var("OPENAI_API_KEY").unwrap(),
    );
    let model = std::env::var("OPENAI_LIVE_MODEL").unwrap_or_else(|_| "gpt-6.1-sol".into());
    let mut req = request(
        &model,
        vec![user(
            "Use the lookup tool once, then tell me what it returned.",
        )],
        true,
    );
    req.generation_config = Some(GaiseGenerationConfig {
        thinking_effort: Some("low".into()),
        ..Default::default()
    });

    let mut stream = client.instruct_stream(&req).await.unwrap();
    let mut acc = GaiseStreamAccumulator::new();
    while let Some(event) = stream.next().await {
        acc.push(&event.unwrap());
    }
    let calls: Vec<GaiseToolCall> = acc.tool_calls.values().cloned().collect();
    assert_eq!(calls[0].function.name, "lookup");
    assert!(acc.usage.is_some());

    let OneOrMany::Many(mut history) = req.input.clone() else {
        unreachable!()
    };
    history.push(GaiseMessage {
        role: "assistant".into(),
        content: (!acc.content_parts.is_empty())
            .then(|| OneOrMany::Many(acc.content_parts.clone())),
        tool_calls: Some(calls.clone()),
        ..Default::default()
    });
    for call in &calls {
        history.push(GaiseMessage {
            role: "tool".into(),
            tool_call_id: Some(call.id.clone()),
            content: Some(OneOrMany::One(GaiseContent::Text {
                text: "The answer is PINEAPPLE-42.".into(),
            })),
            ..Default::default()
        });
    }
    req.input = OneOrMany::Many(history);
    let response = client.instruct(&req).await.unwrap();
    let OneOrMany::Many(messages) = response.output else {
        unreachable!()
    };
    let text = format!("{:?}", messages[0].content);
    assert!(text.contains("PINEAPPLE-42"), "{text}");
}
