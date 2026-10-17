//! Hermetic `POST /v1/speech-to-text` tests against a local mock server.

use gaise_core::GaiseClient;
use gaise_core::contracts::{
    GaiseMediaSource, GaiseTimestampGranularity, GaiseTranscriptTokenKind,
    GaiseTranscriptionRequest,
};
use gaise_provider_elevenlabs::GaiseClientElevenLabs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Raw captured HTTP request.
struct Captured {
    head: String,
    body: Vec<u8>,
}

/// Serve exactly one request with `status` and a JSON `body`.
async fn serve_once(
    status: u16,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        let head_end = loop {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0, "client closed before sending headers");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        let length: usize = head
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.eq_ignore_ascii_case("content-length")
                    .then(|| v.trim().parse().unwrap())
            })
            .expect("content-length");
        while buf.len() < head_end + length {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            buf.extend_from_slice(&chunk[..n]);
        }
        let response = format!(
            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.ok();
        Captured {
            head,
            body: buf[head_end..head_end + length].to_vec(),
        }
    });
    (format!("http://{addr}"), handle)
}

const SINGLE: &str = r#"{
  "language_code": "eng",
  "language_probability": 0.99,
  "text": "Hi (laughs)",
  "words": [
    {"text": "Hi", "start": 0.0, "end": 0.3, "type": "word", "speaker_id": "speaker_0", "logprob": -0.05},
    {"text": " ", "start": 0.3, "end": 0.4, "type": "spacing", "speaker_id": "speaker_0", "logprob": 0.0},
    {"text": "(laughs)", "start": 0.4, "end": 1.1, "type": "audio_event", "speaker_id": "speaker_0", "logprob": -0.7}
  ],
  "transcription_id": "tr_abc",
  "audio_duration_secs": 1.25
}"#;

#[tokio::test]
async fn transcribe_posts_multipart_and_maps_the_response() {
    let (url, server) = serve_once(200, SINGLE).await;
    let client = GaiseClientElevenLabs::new(url, "test-key".into());
    let request = GaiseTranscriptionRequest {
        model: "scribe_v2".into(),
        audio: Some(GaiseMediaSource::bytes(
            b"RIFF\x00\x00\x00\x00WAVEfmt ".to_vec(),
            "audio/wav",
        )),
        languages: vec!["en".into()],
        keyterms: vec!["GAISe".into(), "Scribe".into()],
        diarize: true,
        timestamps: Some(GaiseTimestampGranularity::Word),
        provider_options: Some(serde_json::json!({"tag_audio_events": true})),
        ..Default::default()
    };
    let response = client.transcribe(&request).await.unwrap();
    let captured = server.await.unwrap();

    let head = captured.head.to_ascii_lowercase();
    assert!(head.starts_with("post /v1/speech-to-text http/1.1"));
    assert!(head.contains("xi-api-key: test-key"));
    let boundary = captured
        .head
        .lines()
        .find_map(|l| l.split_once("boundary=").map(|(_, b)| b.trim().to_string()))
        .expect("multipart boundary");
    let body = String::from_utf8_lossy(&captured.body);
    let field = |name: &str, value: &str| {
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
    };
    assert!(body.contains(&field("model_id", "scribe_v2")));
    assert!(body.contains(&field("language_code", "en")));
    assert!(body.contains(&field("diarize", "true")));
    assert!(body.contains(&field("timestamps_granularity", "word")));
    assert!(body.contains(&field("tag_audio_events", "true")));
    assert!(body.contains(&field("keyterms", "GAISe")));
    assert!(body.contains(&field("keyterms", "Scribe")));
    assert!(
        body.contains(
            "name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFF"
        )
    );
    assert!(!body.contains("webhook"));
    assert!(body.ends_with(&format!("--{boundary}--\r\n")));

    assert_eq!(response.text, "Hi (laughs)");
    assert_eq!(response.language.as_deref(), Some("eng"));
    assert_eq!(response.external_id.as_deref(), Some("tr_abc"));
    assert_eq!(response.words.len(), 3);
    assert_eq!(response.words[2].kind, GaiseTranscriptTokenKind::AudioEvent);
    let confidence = response.words[0].confidence.unwrap();
    assert!(confidence > 0.95 && confidence < 1.0);
    assert_eq!(
        response.usage.unwrap().input.unwrap()["audio_milliseconds"],
        1250
    );
    assert_eq!(response.segments.len(), 1);
}

#[tokio::test]
async fn transcribe_sends_source_url_and_maps_multichannel() {
    let multi = r#"{
      "transcripts": [
        {"language_code": "en", "language_probability": 1.0, "text": "Left", "channel_index": 0,
         "words": [{"text": "Left", "start": 0.2, "end": 0.5, "type": "word", "logprob": 0.0, "channel_index": 0}]},
        {"language_code": "en", "language_probability": 1.0, "text": "Right", "channel_index": 1,
         "words": [{"text": "Right", "start": 0.1, "end": 0.4, "type": "word", "logprob": 0.0, "channel_index": 1}]}
      ],
      "transcription_id": "tr_multi",
      "audio_duration_secs": 0.5
    }"#;
    let (url, server) = serve_once(200, multi).await;
    let client = GaiseClientElevenLabs::new(url, "k".into());
    let request = GaiseTranscriptionRequest {
        model: "scribe_v2".into(),
        audio: Some(GaiseMediaSource::url("https://media.example/call.wav")),
        multichannel: true,
        ..Default::default()
    };
    let response = client.transcribe(&request).await.unwrap();
    let captured = server.await.unwrap();
    let body = String::from_utf8_lossy(&captured.body);
    assert!(body.contains("name=\"source_url\"\r\n\r\nhttps://media.example/call.wav\r\n"));
    assert!(body.contains("name=\"use_multi_channel\"\r\n\r\ntrue\r\n"));
    assert!(!body.contains("name=\"file\""));

    assert_eq!(response.text, "Left\nRight");
    assert_eq!(response.channels.len(), 2);
    assert_eq!(response.channels[1].text, "Right");
    assert_eq!(response.words[0].text, "Right");
    assert_eq!(response.words[0].channel, Some(1));
    assert_eq!(response.external_id.as_deref(), Some("tr_multi"));
}

#[tokio::test]
async fn transcribe_surfaces_api_errors() {
    let (url, server) = serve_once(
        401,
        r#"{"detail":{"status":"invalid_api_key","message":"Invalid API key"}}"#,
    )
    .await;
    let client = GaiseClientElevenLabs::new(url, "bad".into());
    let request = GaiseTranscriptionRequest {
        model: "scribe_v2".into(),
        audio: Some(GaiseMediaSource::bytes(vec![1, 2, 3], "audio/mpeg")),
        ..Default::default()
    };
    let err = client.transcribe(&request).await.unwrap_err().to_string();
    server.await.unwrap();
    assert!(err.contains("Invalid API key"), "{err}");
}

#[tokio::test]
async fn transcribe_rejects_invalid_requests_before_sending() {
    let client = GaiseClientElevenLabs::new("http://127.0.0.1:9".into(), "k".into());
    let request = GaiseTranscriptionRequest {
        model: "scribe_v2".into(),
        audio: Some(GaiseMediaSource::bytes(vec![0; 8], "audio/pcm")),
        format: Some(gaise_core::contracts::GaiseAudioFormat::pcm(8_000)),
        ..Default::default()
    };
    let err = client.transcribe(&request).await.unwrap_err().to_string();
    assert!(err.contains("pcm_s16le_16"), "{err}");
}
