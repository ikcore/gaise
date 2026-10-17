#![cfg(feature = "live")]

use gaise_core::contracts::*;
use gaise_provider_xai::xai_live_client::*;
use serde_json::json;

#[test]
fn session_update_uses_xai_fields() {
    let config: GaiseLiveConfig = serde_json::from_value(json!({
        "model": "grok-voice-3.0",
        "system_instruction": "You are helpful.",
        "voice": "Eve",
        "tools": [{"name": "get_weather", "description": "Weather", "parameters": {
            "type": "object", "properties": {"city": {"type": "text"}}, "required": ["city"]
        }}],
        "generation_config": {"thinking_effort": "none"},
        "transcription": {"input": true}
    }))
    .unwrap();
    let update = build_xai_session_update(&config);
    let session = &update["session"];
    assert_eq!(update["type"], "session.update");
    assert_eq!(session["instructions"], "You are helpful.");
    assert_eq!(session["voice"], "eve");
    assert_eq!(session["turn_detection"]["type"], "server_vad");
    assert_eq!(session["audio"]["input"]["format"]["rate"], 24_000);
    assert_eq!(session["audio"]["output"]["format"]["type"], "audio/pcm");
    assert_eq!(
        session["audio"]["input"]["transcription"]["model"],
        "grok-transcribe"
    );
    assert_eq!(session["reasoning"]["effort"], "none");
    assert_eq!(session["tools"][0]["type"], "function");
    assert_eq!(session["tools"][0]["name"], "get_weather");
    assert_eq!(
        session["tools"][0]["parameters"]["properties"]["city"]["type"],
        "string"
    );
    // OpenAI-only fields are never sent.
    assert!(session.get("type").is_none());
    assert!(session.get("output_modalities").is_none());
}

#[test]
fn manual_turns_send_null_turn_detection_and_effort_maps_to_high() {
    let config: GaiseLiveConfig = serde_json::from_value(json!({
        "model": "grok-voice-latest",
        "vad_config": {"enabled": false},
        "generation_config": {"thinking_effort": "medium"}
    }))
    .unwrap();
    let update = build_xai_session_update(&config);
    assert!(update["session"]["turn_detection"].is_null());
    assert!(
        update["session"]
            .as_object()
            .unwrap()
            .contains_key("turn_detection")
    );
    assert_eq!(update["session"]["reasoning"]["effort"], "high");
    assert_eq!(xai_live_reasoning_effort("auto"), None);
}

#[test]
fn realtime_url_handles_v1_and_bare_hosts() {
    assert_eq!(
        realtime_url("https://api.x.ai/v1", "grok-voice-3.0"),
        "wss://api.x.ai/v1/realtime?model=grok-voice-3.0"
    );
    assert_eq!(
        realtime_url("https://api.x.ai/", "grok-voice-latest"),
        "wss://api.x.ai/v1/realtime?model=grok-voice-latest"
    );
}

#[test]
fn server_events_map_to_live_events() {
    let audio =
        map_xai_live_event(&json!({"type": "response.output_audio.delta", "delta": "AQID"}));
    assert!(
        matches!(&audio[0], GaiseLiveEvent::Audio { data, sample_rate: 24_000 } if data == &vec![1, 2, 3])
    );
    let legacy = map_xai_live_event(&json!({"type": "response.audio.delta", "delta": "AQID"}));
    assert_eq!(legacy.len(), 1);
    let text = map_xai_live_event(&json!({"type": "response.text.delta", "delta": "hi"}));
    assert!(matches!(&text[0], GaiseLiveEvent::Text { text } if text == "hi"));
    let transcript = map_xai_live_event(
        &json!({"type": "response.output_audio_transcript.delta", "delta": "yo"}),
    );
    assert!(
        matches!(&transcript[0], GaiseLiveEvent::Transcript { role, .. } if role == "assistant")
    );
    assert!(
        map_xai_live_event(&json!({"type": "conversation.item.input_audio_transcription.updated", "transcript": "partial"}))
            .is_empty()
    );
    let user = map_xai_live_event(
        &json!({"type": "conversation.item.input_audio_transcription.completed", "transcript": "hello"}),
    );
    assert!(
        matches!(&user[0], GaiseLiveEvent::Transcript { role, text } if role == "user" && text == "hello")
    );
    let call = map_xai_live_event(
        &json!({"type": "response.function_call_arguments.done", "call_id": "c1", "name": "f", "arguments": "{}"}),
    );
    assert!(
        matches!(&call[0], GaiseLiveEvent::ToolCall { id, function } if id == "c1" && function.name == "f")
    );
    let done = map_xai_live_event(
        &json!({"type": "response.done", "response": {"usage": {"input_tokens": 5, "output_tokens": 7, "total_tokens": 12}}}),
    );
    assert!(
        matches!(&done[0], GaiseLiveEvent::Usage(u) if u.total.as_ref().unwrap()["total_tokens"] == 12)
    );
    assert!(matches!(done[1], GaiseLiveEvent::TurnComplete));
    let done_no_usage = map_xai_live_event(&json!({"type": "response.done", "response": {}}));
    assert_eq!(done_no_usage.len(), 1);
    assert!(matches!(
        map_xai_live_event(&json!({"type": "input_audio_buffer.speech_started"}))[0],
        GaiseLiveEvent::Interrupted
    ));
    let err = map_xai_live_event(&json!({"type": "error", "error": {"message": "bad"}}));
    assert!(matches!(&err[0], GaiseLiveEvent::Error { message } if message == "bad"));
}

// ── Streaming speech-to-text ──────────────────────────────────────────

#[test]
fn stt_url_defaults_to_16k_pcm() {
    let config = GaiseTranscriptionSessionConfig {
        model: "grok-voice-transcribe-2.0".into(),
        ..Default::default()
    };
    assert_eq!(
        build_xai_stt_url("https://api.x.ai/v1", &config).unwrap(),
        "wss://api.x.ai/v1/stt?model=grok-voice-transcribe-2.0&encoding=pcm&sample_rate=16000&interim_results=false"
    );
}

#[test]
fn stt_url_maps_options_and_encodes_keyterms() {
    let config = GaiseTranscriptionSessionConfig {
        model: "grok-voice-transcribe-2.0".into(),
        audio_format: Some(GaiseAudioFormat {
            codec: "mulaw".into(),
            sample_rate: Some(8000),
            channels: Some(2),
        }),
        languages: vec!["en".into()],
        keyterms: vec!["Understand The Universe".into(), "Grok".into()],
        diarize: true,
        interim_results: true,
        silence_duration_ms: Some(700),
        commit_strategy: Some(GaiseCommitStrategy::TurnPrediction),
        provider_options: Some(json!({"format": true, "smart_turn": 0.7})),
        ..Default::default()
    };
    let url = build_xai_stt_url("https://api.x.ai", &config).unwrap();
    assert!(url.starts_with("wss://api.x.ai/v1/stt?"), "{url}");
    for part in [
        "encoding=mulaw",
        "sample_rate=8000",
        "multichannel=true",
        "channels=2",
        "interim_results=true",
        "language=en",
        "diarize=true",
        "keyterm=Understand+The+Universe",
        "keyterm=Grok",
        "endpointing=700",
        "format=true",
        "smart_turn=0.7",
    ] {
        assert!(url.contains(part), "{part} missing from {url}");
    }
    assert!(
        !url.contains("smart_turn=0.5"),
        "provider_options replace mapped params"
    );

    let opus = GaiseTranscriptionSessionConfig {
        audio_format: Some(GaiseAudioFormat {
            codec: "opus".into(),
            sample_rate: Some(48_000),
            channels: None,
        }),
        commit_strategy: Some(GaiseCommitStrategy::TurnPrediction),
        ..Default::default()
    };
    let url = build_xai_stt_url("https://api.x.ai/v1", &opus).unwrap();
    assert!(url.contains("encoding=opus"));
    assert!(!url.contains("sample_rate"), "{url}");
    assert!(url.contains("smart_turn=0.5"));
    assert!(!url.contains("model="), "empty model is omitted: {url}");
}

#[test]
fn stt_url_rejects_unsupported_settings() {
    let manual = GaiseTranscriptionSessionConfig {
        commit_strategy: Some(GaiseCommitStrategy::Manual),
        ..Default::default()
    };
    assert!(build_xai_stt_url("https://api.x.ai/v1", &manual).is_err());
    let mp3 = GaiseTranscriptionSessionConfig {
        audio_format: Some(GaiseAudioFormat {
            codec: "mp3".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(
        build_xai_stt_url("https://api.x.ai/v1", &mp3)
            .unwrap_err()
            .contains("mp3")
    );
}

#[test]
fn stt_control_frames() {
    assert_eq!(
        xai_stt_control_frame(&GaiseTranscriptionInput::Commit).unwrap(),
        json!({"type": "finalize"})
    );
    assert_eq!(
        xai_stt_control_frame(&GaiseTranscriptionInput::Close).unwrap(),
        json!({"type": "audio.done"})
    );
    assert!(xai_stt_control_frame(&GaiseTranscriptionInput::Audio { data: vec![1] }).is_none());
}

#[test]
fn stt_events_map_to_transcription_events() {
    let partial = map_xai_stt_event(&json!({
        "type": "transcript.partial",
        "text": "hello world",
        "is_final": true,
        "speech_final": true,
        "start": 1.5,
        "duration": 0.75,
        "channel_index": 1,
        "words": [
            {"text": "hello", "start": 1.5, "end": 1.9, "confidence": 0.8, "speaker": 2},
            {"text": "world", "start": 1.9, "end": 2.25, "speaker": 2}
        ]
    }));
    let GaiseTranscriptionEvent::Transcript(t) = &partial[0] else {
        panic!("expected a transcript");
    };
    assert_eq!(t.text, "hello world");
    assert!(t.is_final && t.utterance_end);
    assert_eq!(t.start, Some(1.5));
    assert_eq!(t.end, Some(2.25));
    assert_eq!(t.channel, Some(1));
    assert_eq!(t.speaker.as_deref(), Some("2"));
    assert_eq!(t.words.len(), 2);
    assert_eq!(t.words[0].channel, Some(1));

    let interim = map_xai_stt_event(&json!({
        "type": "transcript.partial", "text": "hel", "is_final": false, "speech_final": false,
        "words": [{"text": "hel", "speaker": 0}, {"text": "lo", "speaker": 1}]
    }));
    let GaiseTranscriptionEvent::Transcript(t) = &interim[0] else {
        panic!("expected a transcript");
    };
    assert!(!t.is_final && !t.utterance_end);
    assert_eq!(
        t.speaker, None,
        "mixed speakers leave the segment unlabeled"
    );

    let done = map_xai_stt_event(
        &json!({"type": "transcript.done", "text": "hello world", "duration": 3.2}),
    );
    assert!(
        matches!(&done[0], GaiseTranscriptionEvent::Usage(u) if u.input.as_ref().unwrap()["audio_milliseconds"] == 3200)
    );
    let err = map_xai_stt_event(&json!({"type": "error", "message": "undecodable frame"}));
    assert!(
        matches!(&err[0], GaiseTranscriptionEvent::Error { message } if message == "undecodable frame")
    );
    assert!(map_xai_stt_event(&json!({"type": "transcript.created"})).is_empty());
}

#[tokio::test]
#[allow(clippy::result_large_err)] // the tungstenite handshake callback signature
async fn transcribe_connect_streams_binary_audio_and_ends_after_done() {
    use futures_util::{SinkExt, StreamExt};
    use gaise_core::GaiseLiveClient;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_hdr_async(tcp, |req: &Request, resp: Response| {
            assert_eq!(req.headers()["authorization"], "Bearer k");
            let query = req.uri().query().unwrap_or_default().to_string();
            assert!(query.contains("encoding=pcm"), "{query}");
            assert!(query.contains("sample_rate=16000"), "{query}");
            assert!(query.contains("interim_results=true"), "{query}");
            Ok(resp)
        })
        .await
        .unwrap();
        ws.send(Message::Text(
            json!({"type": "transcript.created"}).to_string().into(),
        ))
        .await
        .unwrap();
        let mut frames = Vec::new();
        while let Some(Ok(msg)) = ws.next().await {
            match msg {
                Message::Binary(data) => {
                    frames.push(format!("audio:{}", data.len()));
                    ws.send(Message::Text(
                        json!({"type": "transcript.partial", "text": "hi", "is_final": false, "speech_final": false, "start": 0.0, "duration": 0.1})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                }
                Message::Text(text) => {
                    let event: serde_json::Value = serde_json::from_str(&text).unwrap();
                    let kind = event["type"].as_str().unwrap().to_string();
                    frames.push(kind.clone());
                    if kind == "audio.done" {
                        ws.send(Message::Text(
                            json!({"type": "transcript.done", "text": "hi", "duration": 0.5})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                        ws.close(None).await.unwrap();
                        break;
                    }
                }
                _ => {}
            }
        }
        frames
    });

    let client = GaiseClientXaiLive::new(format!("http://{addr}/v1"), "k".into());
    let mut session = client
        .transcribe_connect(&GaiseTranscriptionSessionConfig {
            model: "grok-voice-transcribe-2.0".into(),
            interim_results: true,
            ..Default::default()
        })
        .await
        .unwrap();
    session
        .tx
        .send(GaiseTranscriptionInput::Audio {
            data: vec![0; 3200],
        })
        .await
        .unwrap();
    session
        .tx
        .send(GaiseTranscriptionInput::Commit)
        .await
        .unwrap();
    session
        .tx
        .send(GaiseTranscriptionInput::Close)
        .await
        .unwrap();

    let mut events = Vec::new();
    while let Some(event) = session.rx.next().await {
        let event = event.unwrap();
        let ended = matches!(event, GaiseTranscriptionEvent::Ended { .. });
        events.push(event);
        if ended {
            break;
        }
    }
    assert!(
        matches!(&events[0], GaiseTranscriptionEvent::Started { model, .. } if model == "grok-voice-transcribe-2.0")
    );
    assert!(events.iter().any(
        |e| matches!(e, GaiseTranscriptionEvent::Transcript(t) if t.text == "hi" && !t.is_final)
    ));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GaiseTranscriptionEvent::Usage(u)
        if u.input.as_ref().unwrap()["audio_milliseconds"] == 500))
    );
    assert!(matches!(
        events.last(),
        Some(GaiseTranscriptionEvent::Ended { .. })
    ));
    assert_eq!(
        server.await.unwrap(),
        ["audio:3200", "finalize", "audio.done"]
    );
}
