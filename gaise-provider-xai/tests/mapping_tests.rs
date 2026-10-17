use axum::{Json, Router, http::HeaderMap, routing::post};
use futures_util::StreamExt;
use gaise_core::contracts::*;
use gaise_core::{GaiseClient, GaiseSpeechClient};
use gaise_provider_xai::contracts::*;
use gaise_provider_xai::xai_client::*;
use serde_json::{Value, json};

fn request(value: Value) -> GaiseInstructRequest {
    serde_json::from_value(value).unwrap()
}

#[test]
fn chat_request_maps_messages_tools_images_and_limits() {
    let req = request(json!({
        "model": "grok-4.7",
        "input": [
            {"role": "system", "content": {"type": "text", "text": "Be brief."}},
            {"role": "user", "content": [
                {"type": "text", "text": "What is this?"},
                {"type": "image", "data": [137, 80, 78, 71], "format": "png"}
            ]},
            {"role": "assistant", "content": [
                {"type": "reasoning", "text": "thinking"},
                {"type": "text", "text": "Calling a tool."}
            ], "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":\"x\"}"}}]},
            {"role": "tool", "tool_call_id": "call_1", "content": {"type": "text", "text": "42"}}
        ],
        "tools": [{"name": "lookup", "description": "Look up", "parameters": {
            "type": "object", "properties": {"q": {"type": "text"}}, "required": ["q"]
        }}],
        "tool_config": {"mode": "any"},
        "generation_config": {
            "max_tokens": 256, "temperature": 0.2, "top_p": 0.9,
            "input_image_detail": "original", "cache_key": "conv-1"
        }
    }));
    let body = serde_json::to_value(xai_chat_request(&req).unwrap()).unwrap();

    assert_eq!(body["model"], "grok-4.7");
    assert_eq!(body["max_completion_tokens"], 256);
    assert!(body.get("max_tokens").is_none());
    assert!((body["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    assert_eq!(body["prompt_cache_key"], "conv-1");
    assert_eq!(body["messages"][0]["content"], "Be brief.");
    let image = &body["messages"][1]["content"][1];
    assert_eq!(image["type"], "image_url");
    assert!(
        image["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert_eq!(
        image["image_url"]["detail"], "high",
        "original degrades to high"
    );
    // Reasoning parts are not replayed on Chat Completions.
    assert_eq!(body["messages"][2]["content"], "Calling a tool.");
    assert_eq!(
        body["messages"][2]["tool_calls"][0]["function"]["name"],
        "lookup"
    );
    assert!(body["messages"][2].get("reasoning_content").is_none());
    assert_eq!(body["messages"][3]["tool_call_id"], "call_1");
    assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
    assert_eq!(
        body["tools"][0]["function"]["parameters"]["properties"]["q"]["type"],
        "string"
    );
    assert_eq!(body["tool_choice"], "required");
    assert_eq!(body["stream"], false);
    assert!(body.get("reasoning_effort").is_none());
}

#[test]
fn audio_input_is_rejected() {
    let req = request(json!({
        "model": "grok-4.7",
        "input": {"role": "user", "content": {"type": "audio", "data": [1, 2], "format": "wav"}}
    }));
    assert!(xai_chat_request(&req).unwrap_err().contains("audio"));
}

#[test]
fn response_maps_reasoning_content_usage_and_tool_calls() {
    let response: XaiChatResponse = serde_json::from_value(json!({
        "id": "chat_1",
        "object": "chat.completion",
        "created": 1,
        "model": "grok-4.7",
        "choices": [{"index": 0, "finish_reason": "stop", "message": {
            "role": "assistant",
            "content": "Paris.",
            "reasoning_content": "The capital of France is Paris.",
            "tool_calls": [{"id": "call_9", "type": "function", "function": {"name": "f", "arguments": "{}"}}]
        }}],
        "usage": {
            "prompt_tokens": 32, "completion_tokens": 9, "total_tokens": 135,
            "prompt_tokens_details": {"text_tokens": 32, "image_tokens": 0, "cached_tokens": 6, "audio_tokens": 0},
            "completion_tokens_details": {"reasoning_tokens": 94},
            "cost_in_usd_ticks": 1500000
        }
    }))
    .unwrap();

    let usage = map_xai_usage(response.usage.as_ref().unwrap());
    assert_eq!(usage.input.as_ref().unwrap()["cached_tokens"], 6);
    assert_eq!(usage.output.as_ref().unwrap()["reasoning_tokens"], 94);
    assert_eq!(
        usage.total.as_ref().unwrap()["cost_in_usd_ticks"],
        1_500_000
    );

    let message = map_xai_message(response.choices.into_iter().next().unwrap().message);
    let OneOrMany::Many(parts) = message.content.unwrap() else {
        panic!("expected reasoning and text");
    };
    assert!(matches!(&parts[0], GaiseContent::Reasoning { text, .. } if text.contains("Paris")));
    assert!(matches!(&parts[1], GaiseContent::Text { text } if text == "Paris."));
    assert_eq!(message.tool_calls.unwrap()[0].id, "call_9");
}

#[test]
fn stream_chunk_emits_reasoning_tool_calls_text_then_usage() {
    let chunk: XaiChatStreamResponse = serde_json::from_value(json!({
        "id": "s1",
        "object": "chat.completion.chunk",
        "choices": [{"index": 0, "delta": {
            "role": "assistant",
            "reasoning_content": "hmm",
            "content": "Hi",
            "tool_calls": [{"index": 0, "id": "call_1", "type": "function", "function": {"name": "f", "arguments": "{\"a\":1}"}}]
        }}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
    }))
    .unwrap();
    let events = map_xai_stream_chunk(chunk);
    assert_eq!(events.len(), 4);
    assert!(matches!(
        &events[0].chunk,
        GaiseStreamChunk::Content(GaiseContent::Reasoning { .. })
    ));
    assert!(
        matches!(&events[1].chunk, GaiseStreamChunk::ToolCall { name, .. } if name.as_deref() == Some("f"))
    );
    assert!(matches!(&events[2].chunk, GaiseStreamChunk::Text(t) if t == "Hi"));
    assert!(matches!(&events[3].chunk, GaiseStreamChunk::Usage(_)));
}

#[test]
fn tts_format_resolution() {
    let f = resolve_tts_format(None, None).unwrap();
    assert_eq!(
        (f.codec, f.mime, f.sample_rate),
        ("mp3", "audio/mpeg", 24_000)
    );
    let f = resolve_tts_format(Some("audio/pcm"), Some(16_000)).unwrap();
    assert_eq!((f.codec, f.sample_rate), ("pcm", 16_000));
    let f = resolve_tts_format(Some("ulaw"), Some(24_000)).unwrap();
    assert_eq!(
        (f.codec, f.mime, f.sample_rate),
        ("mulaw", "audio/basic", 8000)
    );
    assert!(resolve_tts_format(Some("audio/flac"), None).is_err());
    assert!(resolve_tts_format(Some("pcm"), Some(32_000)).is_err());
}

#[test]
fn tts_request_defaults_language_and_maps_voice_settings() {
    let (body, _) = build_tts_request(&GaiseSpeechRequest {
        model: "tts".into(),
        input: "Hello".into(),
        voice: Some("ara".into()),
        voice_settings: Some(GaiseVoiceSettings {
            speed: Some(3.0),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["language"], "auto");
    assert_eq!(json["voice_id"], "ara");
    assert_eq!(json["speed"], 1.5);
    assert_eq!(json["output_format"]["codec"], "mp3");
    assert!(json.get("with_timestamps").is_none());
    assert!(json.get("model").is_none());
}

async fn server(app: Router) -> (GaiseClientXai, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (GaiseClientXai::new(url, "test-key".into()), task)
}

fn user(text: &str) -> GaiseInstructRequest {
    request(json!({
        "model": "grok-4.7",
        "input": {"role": "user", "content": {"type": "text", "text": text}},
        "generation_config": {"thinking_effort": "high"}
    }))
}

#[tokio::test]
async fn instruct_and_stream_over_http() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert_eq!(headers["authorization"], "Bearer test-key");
            assert_eq!(body["reasoning_effort"], "high");
            if body["stream"] == true {
                assert_eq!(body["stream_options"]["include_usage"], true);
                let chunks = [
                    json!({"id": "s", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": {"reasoning_content": "r"}}]}),
                    json!({"id": "s", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": {"content": "Hel"}}]}),
                    json!({"id": "s", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": {"content": "lo"}, "finish_reason": "stop"}]}),
                    json!({"id": "s", "object": "chat.completion.chunk", "choices": [], "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}}),
                ];
                let mut sse: String = chunks.iter().map(|c| format!("data: {c}\n\n")).collect();
                sse.push_str("data: [DONE]\n\n");
                return sse;
            }
            json!({"id": "c1", "object": "chat.completion", "created": 1, "model": "grok-4.7",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "Hello"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}})
            .to_string()
        }),
    );
    let (client, task) = server(app).await;

    let response = client.instruct(&user("hi")).await.unwrap();
    assert_eq!(response.external_id.as_deref(), Some("c1"));

    let mut stream = client.instruct_stream(&user("hi")).await.unwrap();
    let mut acc = GaiseStreamAccumulator::new();
    while let Some(event) = stream.next().await {
        acc.push(&event.unwrap());
    }
    assert_eq!(acc.text, "Hello");
    assert!(acc.usage.is_some());
    task.abort();
}

#[tokio::test]
async fn non_chat_models_fail_fast_without_http() {
    let client = GaiseClientXai::new("http://127.0.0.1:9/v1".into(), "k".into());
    let mut req = user("hi");
    req.model = "grok-4.20-multi-agent".into();
    let err = client.instruct(&req).await.unwrap_err().to_string();
    assert!(err.contains("Responses API"), "{err}");
}

#[tokio::test]
async fn speech_returns_raw_audio_and_timestamped_json() {
    let app = Router::new().route(
        "/v1/tts",
        post(|Json(body): Json<Value>| async move {
            assert_eq!(body["voice_id"], "eve");
            if body["with_timestamps"] == true {
                let payload = json!({
                    "audio": "AQID",
                    "content_type": "audio/mpeg",
                    "duration": 0.5,
                    "audio_timestamps": {"graph_chars": ["H", "i"], "graph_times": [{"start": 0.0, "end": 0.2}, {"start": 0.2, "end": 0.5}]}
                });
                return (
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    payload.to_string().into_bytes(),
                );
            }
            ([(axum::http::header::CONTENT_TYPE, "audio/mpeg")], vec![9u8, 8, 7])
        }),
    );
    let (client, task) = server(app).await;
    let mut req = GaiseSpeechRequest {
        model: "tts".into(),
        input: "Hi".into(),
        voice: Some("eve".into()),
        ..Default::default()
    };

    let raw = client.speech(&req).await.unwrap();
    assert_eq!(raw.audio, vec![9, 8, 7]);
    assert_eq!(raw.format, "audio/mpeg");

    let mut stream = client.speech_stream(&req).await.unwrap();
    let mut streamed = Vec::new();
    while let Some(chunk) = stream.next().await {
        if let GaiseSpeechChunk::Audio { data, .. } = chunk.unwrap().chunk {
            streamed.extend(data);
        }
    }
    assert_eq!(streamed, vec![9, 8, 7]);

    req.include_alignment = true;
    let timed = client.speech(&req).await.unwrap();
    assert_eq!(timed.audio, vec![1, 2, 3]);
    let alignment = timed.alignment.unwrap();
    assert_eq!(alignment.characters, vec!["H", "i"]);
    assert_eq!(alignment.end_seconds, vec![0.2, 0.5]);
    task.abort();
}
