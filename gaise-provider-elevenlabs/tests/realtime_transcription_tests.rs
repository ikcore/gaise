//! Hermetic Scribe realtime session test against a local WebSocket server.
#![cfg(feature = "live")]
// The tungstenite handshake callback signature returns a large `Err` type.
#![allow(clippy::result_large_err)]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use gaise_core::GaiseLiveClient;
use gaise_core::contracts::{
    GaiseAudioFormat, GaiseCommitStrategy, GaiseTimestampGranularity, GaiseTranscriptionEvent,
    GaiseTranscriptionInput, GaiseTranscriptionSessionConfig,
};
use gaise_provider_elevenlabs::GaiseClientElevenLabs;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

#[tokio::test]
async fn realtime_session_streams_audio_and_maps_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut uri = String::new();
        let mut key = String::new();
        let mut ws =
            tokio_tungstenite::accept_hdr_async(stream, |req: &Request, resp: Response| {
                uri = req.uri().to_string();
                key = req
                    .headers()
                    .get("xi-api-key")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                Ok(resp)
            })
            .await
            .unwrap();
        ws.send(Message::Text(
            r#"{"message_type":"session_started","session_id":"sess_1","config":{"model_id":"scribe_v2_realtime"}}"#.into(),
        ))
        .await
        .unwrap();

        let mut frames = Vec::new();
        while let Some(Ok(msg)) = ws.next().await {
            match msg {
                Message::Text(text) => {
                    let frame: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                    let commit = frame["commit"] == true;
                    frames.push(frame);
                    if frames.len() == 1 {
                        ws.send(Message::Text(
                            r#"{"message_type":"partial_transcript","text":"hello"}"#.into(),
                        ))
                        .await
                        .unwrap();
                    }
                    if commit {
                        ws.send(Message::Text(
                            r#"{"message_type":"committed_transcript","text":"hello world"}"#
                                .into(),
                        ))
                        .await
                        .unwrap();
                        ws.send(Message::Text(
                            r#"{"message_type":"committed_transcript_with_timestamps","text":"hello world","language_code":"en","words":[{"text":"hello","start":0.0,"end":0.4,"type":"word","logprob":-0.1},{"text":" ","start":0.4,"end":0.5,"type":"spacing","logprob":0.0},{"text":"world","start":0.5,"end":0.9,"type":"word","logprob":-0.2}]}"#.into(),
                        ))
                        .await
                        .unwrap();
                    }
                }
                Message::Close(_) => {}
                _ => {}
            }
        }
        (uri, key, frames)
    });

    let client = GaiseClientElevenLabs::new(format!("http://{addr}"), "rt-key".into());
    let config = GaiseTranscriptionSessionConfig {
        model: "scribe_v2_realtime".into(),
        audio_format: Some(GaiseAudioFormat::pcm(16_000)),
        languages: vec!["en".into()],
        prompt: Some("Earlier text".into()),
        interim_results: true,
        timestamps: Some(GaiseTimestampGranularity::Word),
        commit_strategy: Some(GaiseCommitStrategy::Manual),
        ..Default::default()
    };
    let mut session = client.transcribe_connect(&config).await.unwrap();
    session
        .tx
        .send(GaiseTranscriptionInput::Audio {
            data: vec![0, 1, 2, 3],
        })
        .await
        .unwrap();
    session
        .tx
        .send(GaiseTranscriptionInput::Audio { data: vec![4, 5] })
        .await
        .unwrap();
    session
        .tx
        .send(GaiseTranscriptionInput::Close)
        .await
        .unwrap();

    let mut events = Vec::new();
    while let Ok(Some(event)) =
        tokio::time::timeout(Duration::from_secs(10), session.rx.next()).await
    {
        let event = event.unwrap();
        let ended = matches!(event, GaiseTranscriptionEvent::Ended { .. });
        events.push(event);
        if ended {
            break;
        }
    }
    let (uri, key, frames) = server.await.unwrap();

    assert_eq!(
        uri,
        "/v1/speech-to-text/realtime?model_id=scribe_v2_realtime&audio_format=pcm_16000&language_code=en&commit_strategy=manual&include_timestamps=true"
    );
    assert_eq!(key, "rt-key");
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0]["message_type"], "input_audio_chunk");
    assert_eq!(frames[0]["audio_base_64"], "AAECAw==");
    assert_eq!(frames[0]["sample_rate"], 16_000);
    assert_eq!(frames[0]["commit"], false);
    assert_eq!(frames[0]["previous_text"], "Earlier text");
    assert!(frames[1].get("previous_text").is_none());
    assert_eq!(frames[2]["commit"], true);
    assert_eq!(frames[2]["audio_base_64"], "");

    assert!(
        matches!(&events[0], GaiseTranscriptionEvent::Started { session_id, .. } if session_id == "sess_1")
    );
    assert!(
        matches!(&events[1], GaiseTranscriptionEvent::Transcript(t) if !t.is_final && t.text == "hello")
    );
    let finals: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            GaiseTranscriptionEvent::Transcript(t) if t.is_final => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(finals.len(), 1, "{events:?}");
    assert_eq!(finals[0].text, "hello world");
    assert_eq!(finals[0].words.len(), 3);
    assert_eq!(finals[0].end, Some(0.9));
    assert!(matches!(
        events.last(),
        Some(GaiseTranscriptionEvent::Ended { .. })
    ));
}

#[tokio::test]
async fn realtime_rejects_unsupported_formats_without_connecting() {
    let client = GaiseClientElevenLabs::new("http://127.0.0.1:9".into(), "k".into());
    let config = GaiseTranscriptionSessionConfig {
        model: "scribe_v2_realtime".into(),
        audio_format: Some(GaiseAudioFormat::pcm(32_000)),
        ..Default::default()
    };
    let err = match client.transcribe_connect(&config).await {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("32000"), "{err}");
}
