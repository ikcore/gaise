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

// ── Media: speech-to-text, images, video ─────────────────────────────

use gaise_provider_xai::xai_media::*;

fn wav_bytes() -> Vec<u8> {
    let mut data = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    data.extend_from_slice(&[0u8; 16]);
    data
}

#[test]
fn stt_parts_map_options_and_put_file_last() {
    let req = GaiseTranscriptionRequest {
        model: "grok-voice-transcribe-2.0".into(),
        audio: Some(GaiseMediaSource::Bytes {
            data: vec![0, 1, 2, 3],
            mime_type: None,
        }),
        languages: vec!["en".into(), "fr".into()],
        keyterms: vec!["Grok".into(), "Understand The Universe".into()],
        diarize: true,
        mode: Some(GaiseTranscriptionMode::Smart),
        format: Some(GaiseAudioFormat {
            codec: "ulaw".into(),
            sample_rate: Some(8000),
            channels: Some(2),
        }),
        multichannel: true,
        provider_options: Some(json!({"vad_threshold": 0.2, "diarize": "false"})),
        ..Default::default()
    };
    let parts = build_xai_stt_parts(&req).unwrap();
    let texts: Vec<(String, String)> = parts
        .iter()
        .filter_map(|p| match p {
            XaiSttPart::Text { name, value } => Some((name.clone(), value.clone())),
            _ => None,
        })
        .collect();
    let get = |n: &str| -> Vec<&str> {
        texts
            .iter()
            .filter(|(k, _)| k == n)
            .map(|(_, v)| v.as_str())
            .collect()
    };
    assert_eq!(get("model"), ["grok-voice-transcribe-2.0"]);
    assert_eq!(get("language"), ["en"]);
    assert_eq!(get("format"), ["true"]);
    assert!(get("filler_words").is_empty());
    assert_eq!(get("keyterm"), ["Grok", "Understand The Universe"]);
    assert_eq!(get("multichannel"), ["true"]);
    assert_eq!(get("audio_format"), ["mulaw"]);
    assert_eq!(get("sample_rate"), ["8000"]);
    assert_eq!(get("channels"), ["2"]);
    assert_eq!(get("vad_threshold"), ["0.2"]);
    assert_eq!(
        get("diarize"),
        ["false"],
        "provider_options replace mapped fields"
    );
    let Some(XaiSttPart::File { file_name, .. }) = parts.last() else {
        panic!("file must be the last part");
    };
    assert_eq!(file_name, "audio.mulaw");
}

#[test]
fn stt_parts_handle_urls_verbatim_and_unsupported_fields() {
    let mut req = GaiseTranscriptionRequest {
        model: "grok-voice-transcribe-2.0".into(),
        audio: Some(GaiseMediaSource::url("https://example.com/a.mp3")),
        mode: Some(GaiseTranscriptionMode::Smart),
        ..Default::default()
    };
    let parts = build_xai_stt_parts(&req).unwrap();
    // Smart mode without a language cannot send format=true (a 400 upstream).
    assert!(
        !parts
            .iter()
            .any(|p| matches!(p, XaiSttPart::Text { name, .. } if name == "format"))
    );
    assert!(
        matches!(parts.last(), Some(XaiSttPart::Text { name, value }) if name == "url" && value.ends_with("a.mp3"))
    );

    req.mode = Some(GaiseTranscriptionMode::Verbatim);
    let parts = build_xai_stt_parts(&req).unwrap();
    assert!(parts.contains(&XaiSttPart::Text {
        name: "filler_words".into(),
        value: "true".into()
    }));

    req.prompt = Some("context".into());
    assert!(build_xai_stt_parts(&req).unwrap_err().contains("prompt"));
    req.prompt = None;
    req.format = Some(GaiseAudioFormat {
        codec: "pcm".into(),
        sample_rate: None,
        channels: None,
    });
    assert!(
        build_xai_stt_parts(&req)
            .unwrap_err()
            .contains("sample_rate")
    );
}

#[test]
fn stt_response_maps_words_speakers_channels_and_usage() {
    let parsed: XaiSttResponse = serde_json::from_value(json!({
        "text": "Hello there",
        "language": "en",
        "duration": 1.234,
        "words": [
            {"text": "Hello", "start": 0.0, "end": 0.4, "confidence": 0.9, "speaker": 0},
            {"text": "there", "start": 0.4, "end": 0.8, "speaker": 1}
        ],
        "channels": [
            {"index": 1, "text": "there", "language": "en", "words": [{"text": "there", "start": 0.4, "end": 0.8}]}
        ]
    }))
    .unwrap();
    let mapped = map_xai_stt_response(parsed);
    assert_eq!(mapped.text, "Hello there");
    assert_eq!(mapped.duration_seconds, Some(1.234));
    assert_eq!(mapped.words[0].speaker.as_deref(), Some("0"));
    assert_eq!(mapped.words[1].speaker.as_deref(), Some("1"));
    assert_eq!(mapped.words[0].confidence, Some(0.9));
    assert_eq!(mapped.channels[0].index, 1);
    assert_eq!(mapped.channels[0].words[0].channel, Some(1));
    assert_eq!(
        mapped.usage.unwrap().input.unwrap()["audio_milliseconds"],
        1234
    );
}

#[tokio::test]
async fn transcribe_posts_multipart_with_file_last() {
    use axum::extract::Multipart;
    let app = Router::new().route(
        "/v1/stt",
        post(|headers: HeaderMap, mut form: Multipart| async move {
            assert_eq!(headers["authorization"], "Bearer test-key");
            let mut fields = Vec::new();
            while let Some(field) = form.next_field().await.unwrap() {
                let name = field.name().unwrap().to_string();
                let file_name = field.file_name().map(str::to_string);
                let content_type = field.content_type().map(str::to_string);
                let bytes = field.bytes().await.unwrap();
                fields.push((name, file_name, content_type, bytes.to_vec()));
            }
            let names: Vec<&str> = fields.iter().map(|f| f.0.as_str()).collect();
            assert_eq!(
                names,
                ["model", "language", "diarize", "keyterm", "keyterm", "file"]
            );
            let file = fields.last().unwrap();
            assert_eq!(file.1.as_deref(), Some("audio.wav"));
            assert_eq!(file.2.as_deref(), Some("audio/wav"));
            assert!(file.3.starts_with(b"RIFF"));
            Json(json!({
                "text": "hi",
                "language": "en",
                "duration": 2.5,
                "words": [{"text": "hi", "start": 0.1, "end": 0.3, "speaker": 0}]
            }))
        }),
    );
    let (client, task) = server(app).await;
    let response = client
        .transcribe(&GaiseTranscriptionRequest {
            model: "grok-voice-transcribe-2.0".into(),
            audio: Some(GaiseMediaSource::Bytes {
                data: wav_bytes(),
                mime_type: None,
            }),
            languages: vec!["en".into()],
            keyterms: vec!["a".into(), "b".into()],
            diarize: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(response.text, "hi");
    assert_eq!(response.language.as_deref(), Some("en"));
    assert_eq!(response.words[0].speaker.as_deref(), Some("0"));
    assert_eq!(
        response.usage.unwrap().input.unwrap()["audio_milliseconds"],
        2500
    );
    task.abort();
}

fn image_request(images: Vec<GaiseMediaSource>) -> GaiseImageRequest {
    GaiseImageRequest {
        model: "grok-imagine-image-2.0".into(),
        prompt: "a cat".into(),
        images,
        n: Some(2),
        aspect_ratio: Some("16:9".into()),
        resolution: Some("1.5K".into()),
        quality: Some("Medium".into()),
        ..Default::default()
    }
}

#[test]
fn image_request_selects_generation_or_edit_path() {
    let (path, body) = build_xai_image_request(&image_request(vec![])).unwrap();
    assert_eq!(path, "images/generations");
    assert_eq!(body["model"], "grok-imagine-image-2.0");
    assert_eq!(body["n"], 2);
    assert_eq!(body["aspect_ratio"], "16:9");
    assert_eq!(body["resolution"], "1.5k");
    assert_eq!(body["quality"], "medium");
    assert_eq!(body["response_format"], "b64_json");
    assert!(body.get("image").is_none());

    let png = GaiseMediaSource::Bytes {
        data: b"\x89PNG\r\n".to_vec(),
        mime_type: None,
    };
    let (path, body) = build_xai_image_request(&image_request(vec![png.clone()])).unwrap();
    assert_eq!(path, "images/edits");
    assert!(
        body["image"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert!(body.get("images").is_none());

    let mut req = image_request(vec![png, GaiseMediaSource::url("https://x/y.jpg")]);
    req.provider_options = Some(json!({"user": "u1", "n": 1}));
    let (path, body) = build_xai_image_request(&req).unwrap();
    assert_eq!(path, "images/edits");
    assert_eq!(body["images"][1]["url"], "https://x/y.jpg");
    assert!(body.get("image").is_none());
    assert_eq!(body["user"], "u1");
    assert_eq!(body["n"], 1, "provider_options win");
}

type ImageMutation = Box<dyn Fn(&mut GaiseImageRequest)>;

#[test]
fn image_request_rejects_unsupported_fields() {
    let cases: Vec<(ImageMutation, &str)> = vec![
        (
            Box::new(|r| r.mask = Some(GaiseMediaSource::url("https://m"))),
            "mask",
        ),
        (
            Box::new(|r| r.negative_prompt = Some("dogs".into())),
            "negative_prompt",
        ),
        (Box::new(|r| r.seed = Some(7)), "seed"),
        (Box::new(|r| r.size = Some("1024x1024".into())), "size"),
        (
            Box::new(|r| r.output_format = Some("image/webp".into())),
            "output_format",
        ),
        (Box::new(|r| r.resolution = Some("4K".into())), "resolution"),
        (Box::new(|r| r.quality = Some("high".into())), "quality"),
    ];
    for (mutate, field) in cases {
        let mut req = image_request(vec![]);
        mutate(&mut req);
        let err = build_xai_image_request(&req).unwrap_err();
        assert!(err.contains(field), "{field}: {err}");
    }
}

#[tokio::test]
async fn generate_images_decodes_base64_and_usage() {
    let app = Router::new()
        .route(
            "/v1/images/generations",
            post(|Json(body): Json<Value>| async move {
                assert!(body.get("image").is_none());
                Json(json!({
                    "data": [
                        {"b64_json": "iVBORw==", "mime_type": "image/png"},
                        {"url": "https://imgen.x.ai/a.jpg", "mime_type": "image/jpeg"}
                    ],
                    "usage": {"cost_in_usd_ticks": 400000000, "input_tokens": 12,
                        "input_tokens_details": {"text_tokens": 12, "image_tokens": 0, "cached_tokens": 0}}
                }))
            }),
        )
        .route(
            "/v1/images/edits",
            post(|Json(body): Json<Value>| async move {
                assert!(body["image"]["url"].as_str().unwrap().starts_with("data:"));
                Json(json!({"data": [{"b64_json": "/9j/"}]}))
            }),
        );
    let (client, task) = server(app).await;
    let response = client
        .generate_images(&image_request(vec![]))
        .await
        .unwrap();
    assert_eq!(response.images.len(), 2);
    assert_eq!(
        response.images[0].data.as_deref(),
        Some(&[0x89, b'P', b'N', b'G'][..])
    );
    assert_eq!(
        response.images[1].url.as_deref(),
        Some("https://imgen.x.ai/a.jpg")
    );
    let usage = response.usage.unwrap();
    assert_eq!(usage.total.unwrap()["cost_in_usd_ticks"], 400_000_000);
    assert_eq!(usage.input.unwrap()["input_tokens"], 12);

    let edited = client
        .generate_images(&image_request(vec![GaiseMediaSource::bytes(
            vec![1, 2],
            "image/jpeg",
        )]))
        .await
        .unwrap();
    assert_eq!(
        edited.images[0].data.as_deref(),
        Some(&[0xFF, 0xD8, 0xFF][..])
    );
    assert_eq!(edited.images[0].mime_type.as_deref(), Some("image/jpeg"));
    task.abort();
}

#[test]
fn video_request_maps_generation_edit_and_extension() {
    let req = GaiseVideoRequest {
        model: "grok-imagine-video-1.5".into(),
        prompt: Some("a lake".into()),
        image: Some(GaiseMediaSource::url("https://x/first.png")),
        last_frame: Some(GaiseMediaSource::url("https://x/last.png")),
        reference_images: vec![GaiseMediaSource::bytes(vec![1], "image/png")],
        duration_seconds: Some(6),
        aspect_ratio: Some("16:9".into()),
        resolution: Some("720P".into()),
        generate_audio: Some(false),
        ..Default::default()
    };
    let (path, body) = build_xai_video_request(&req).unwrap();
    assert_eq!(path, "videos/generations");
    assert_eq!(body["image"]["url"], "https://x/first.png");
    assert_eq!(body["last_frame"]["url"], "https://x/last.png");
    assert!(
        body["reference_images"][0]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png")
    );
    assert_eq!(body["duration"], 6);
    assert_eq!(body["resolution"], "720p");
    assert_eq!(body["generate_audio"], false);

    let edit = GaiseVideoRequest {
        model: "grok-imagine-video".into(),
        prompt: Some("add a hat".into()),
        video: Some(GaiseMediaSource::url("https://x/v.mp4")),
        ..Default::default()
    };
    let (path, body) = build_xai_video_request(&edit).unwrap();
    assert_eq!(path, "videos/edits");
    assert_eq!(body["video"]["url"], "https://x/v.mp4");
    assert!(body.get("duration").is_none());

    let extend = GaiseVideoRequest {
        duration_seconds: Some(4),
        ..edit.clone()
    };
    let (path, body) = build_xai_video_request(&extend).unwrap();
    assert_eq!(path, "videos/extensions");
    assert_eq!(body["duration"], 4);

    let bad = GaiseVideoRequest {
        resolution: Some("720p".into()),
        ..edit
    };
    assert!(
        build_xai_video_request(&bad)
            .unwrap_err()
            .contains("resolution")
    );
    let bad = GaiseVideoRequest {
        seed: Some(1),
        ..req.clone()
    };
    assert!(build_xai_video_request(&bad).unwrap_err().contains("seed"));
    let bad = GaiseVideoRequest {
        resolution: Some("4k".into()),
        ..req
    };
    assert!(
        build_xai_video_request(&bad)
            .unwrap_err()
            .contains("resolution")
    );
}

#[test]
fn video_job_maps_pending_done_failed_and_moderated() {
    let pending = map_xai_video_job(
        "r1",
        "grok-imagine-video-1.5",
        &json!({"status": "pending", "progress": 42}),
    );
    assert_eq!(pending.status, GaiseJobStatus::Running);
    assert_eq!(pending.progress, Some(42));

    let done = map_xai_video_job(
        "r1",
        "grok-imagine-video-1.5",
        &json!({
            "status": "done", "progress": 100, "model": "grok-imagine-video-1.5",
            "video": {"url": "https://vidgen.x.ai/v.mp4", "duration": 6, "respect_moderation": true},
            "usage": {"cost_in_usd_ticks": 4800000000u64, "total_tokens": 10}
        }),
    );
    assert_eq!(done.status, GaiseJobStatus::Completed);
    assert_eq!(
        done.videos[0].url.as_deref(),
        Some("https://vidgen.x.ai/v.mp4")
    );
    assert_eq!(done.videos[0].duration_seconds, Some(6.0));
    let total = done.usage.unwrap().total.unwrap();
    assert_eq!(total["cost_in_usd_ticks"], 4_800_000_000);
    assert_eq!(total["total_tokens"], 10);

    let failed = map_xai_video_job(
        "r1",
        "m",
        &json!({"status": "failed", "error": {"code": "invalid_argument", "message": "bad prompt"}}),
    );
    assert_eq!(failed.status, GaiseJobStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("bad prompt"));

    let moderated = map_xai_video_job(
        "r1",
        "m",
        &json!({"status": "done", "video": {"url": "", "duration": 6, "respect_moderation": false}}),
    );
    assert_eq!(moderated.status, GaiseJobStatus::Completed);
    assert_eq!(moderated.filtered, Some(1));
    assert!(moderated.videos.is_empty());
}

#[tokio::test]
async fn video_start_and_poll_over_http() {
    use axum::{extract::Path, routing::get};
    let app = Router::new()
        .route(
            "/v1/videos/generations",
            post(|Json(body): Json<Value>| async move {
                assert_eq!(body["model"], "grok-imagine-video-1.5-lite");
                Json(json!({"request_id": "req-9"}))
            }),
        )
        .route(
            "/v1/videos/:id",
            get(|Path(id): Path<String>| async move {
                assert_eq!(id, "req-9");
                Json(json!({"status": "done", "progress": 100,
                    "video": {"url": "https://vidgen.x.ai/req-9.mp4", "duration": 5, "respect_moderation": true}}))
            }),
        );
    let (client, task) = server(app).await;
    let job = client
        .start_video(&GaiseVideoRequest {
            model: "grok-imagine-video-1.5-lite".into(),
            prompt: Some("waves".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(job.id, "req-9");
    assert_eq!(job.status, GaiseJobStatus::Queued);
    let polled = client
        .get_video(&GaiseVideoJobRequest {
            model: job.model.clone(),
            id: job.id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(polled.status, GaiseJobStatus::Completed);
    assert_eq!(polled.model, "grok-imagine-video-1.5-lite");
    assert_eq!(polled.videos.len(), 1);
    task.abort();
}
