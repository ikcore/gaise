use base64::Engine;
use gaise_core::GaiseClient;
use gaise_core::contracts::{
    GaiseAudioFormat, GaiseImageRequest, GaiseMediaSource, GaiseTimestampGranularity,
    GaiseTranscriptionMode, GaiseTranscriptionRequest, GaiseVideoRequest,
};
use gaise_provider_gemini::gemini_client::GaiseClientGemini;
use gaise_provider_gemini::gemini_media::{
    GeminiMediaResponse, gemini_image_request, gemini_transcription_request,
    map_gemini_image_response, map_gemini_transcription_response, media_part,
    merge_generate_content_options, normalize_image_size, parse_duration_seconds,
};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn image_request() -> GaiseImageRequest {
    GaiseImageRequest {
        model: "gemini-nano-banana-2.1".into(),
        prompt: "a lighthouse at dusk".into(),
        ..Default::default()
    }
}

fn transcription_request() -> GaiseTranscriptionRequest {
    GaiseTranscriptionRequest {
        model: "gemini-3.5-transcribe".into(),
        audio: Some(GaiseMediaSource::bytes(
            b"RIFF0000WAVEfmt ".to_vec(),
            "audio/wav",
        )),
        ..Default::default()
    }
}

fn response(value: serde_json::Value) -> GeminiMediaResponse {
    serde_json::from_value(value).unwrap()
}

// ── Images ────────────────────────────────────────────────────────────

#[test]
fn image_request_shape() {
    let request = GaiseImageRequest {
        images: vec![
            GaiseMediaSource::bytes(vec![0x89, b'P', b'N', b'G'], "image/png"),
            GaiseMediaSource::url("https://generativelanguage.googleapis.com/v1beta/files/abc"),
            GaiseMediaSource::Url {
                url: format!("data:image/jpeg;base64,{}", b64(&[0xFF, 0xD8, 0xFF])),
                mime_type: None,
            },
        ],
        aspect_ratio: Some("16:9".into()),
        resolution: Some("2k".into()),
        quality: Some("auto".into()),
        provider_options: Some(json!({
            "thinkingConfig": {"thinkingLevel": "HIGH"},
            "safetySettings": [{"category": "HARM_CATEGORY_HARASSMENT", "threshold": "OFF"}],
        })),
        ..image_request()
    };
    let body = gemini_image_request(&request).unwrap();
    let parts = &body["contents"][0]["parts"];
    assert_eq!(body["contents"][0]["role"], "user");
    assert_eq!(parts[0]["text"], "a lighthouse at dusk");
    assert_eq!(parts[1]["inlineData"]["mimeType"], "image/png");
    assert_eq!(
        parts[1]["inlineData"]["data"],
        b64(&[0x89, b'P', b'N', b'G'])
    );
    assert_eq!(
        parts[2]["fileData"]["fileUri"],
        "https://generativelanguage.googleapis.com/v1beta/files/abc"
    );
    assert_eq!(parts[3]["inlineData"]["mimeType"], "image/jpeg");
    let config = &body["generationConfig"];
    assert_eq!(config["responseModalities"], json!(["TEXT", "IMAGE"]));
    assert_eq!(
        config["imageConfig"],
        json!({"aspectRatio": "16:9", "imageSize": "2K"})
    );
    assert_eq!(config["thinkingConfig"]["thinkingLevel"], "HIGH");
    assert_eq!(body["safetySettings"][0]["threshold"], "OFF");
    assert!(config.get("seed").is_none());
}

#[test]
fn image_request_minimal_has_no_image_config() {
    let body = gemini_image_request(&image_request()).unwrap();
    assert!(body["generationConfig"].get("imageConfig").is_none());
    assert_eq!(body["contents"][0]["parts"].as_array().unwrap().len(), 1);
}

#[test]
fn image_request_seed_only_on_models_that_take_one() {
    let older = GaiseImageRequest {
        model: "gemini-2.5-flash-image".into(),
        seed: Some(7),
        ..image_request()
    };
    assert_eq!(
        gemini_image_request(&older).unwrap()["generationConfig"]["seed"],
        7
    );
    let newer = GaiseImageRequest {
        seed: Some(7),
        ..image_request()
    };
    assert!(gemini_image_request(&newer).is_err());
}

#[test]
fn image_request_rejects_unsupported_options() {
    let cases = [
        GaiseImageRequest {
            n: Some(2),
            ..image_request()
        },
        GaiseImageRequest {
            size: Some("1024x1024".into()),
            ..image_request()
        },
        GaiseImageRequest {
            mask: Some(GaiseMediaSource::url("gs://b/m.png")),
            ..image_request()
        },
        GaiseImageRequest {
            negative_prompt: Some("blur".into()),
            ..image_request()
        },
        GaiseImageRequest {
            quality: Some("high".into()),
            ..image_request()
        },
        GaiseImageRequest {
            output_format: Some("image/webp".into()),
            ..image_request()
        },
        GaiseImageRequest {
            aspect_ratio: Some("7:3".into()),
            ..image_request()
        },
        GaiseImageRequest {
            resolution: Some("1.5K".into()),
            ..image_request()
        },
        GaiseImageRequest {
            images: vec![GaiseMediaSource::url("gs://b/a.png"); 15],
            ..image_request()
        },
        GaiseImageRequest {
            prompt: " ".into(),
            ..image_request()
        },
        GaiseImageRequest {
            provider_options: Some(json!(["not", "an", "object"])),
            ..image_request()
        },
    ];
    for case in cases {
        assert!(gemini_image_request(&case).is_err(), "{case:?}");
    }
    assert!(
        gemini_image_request(&GaiseImageRequest {
            n: Some(1),
            ..image_request()
        })
        .is_ok()
    );
}

#[test]
fn image_size_normalisation() {
    assert_eq!(normalize_image_size("512").unwrap(), "512");
    assert_eq!(normalize_image_size("1k").unwrap(), "1K");
    assert_eq!(normalize_image_size(" 4K ").unwrap(), "4K");
    assert!(normalize_image_size("8K").is_err());
}

#[test]
fn image_response_mapping() {
    let mapped = map_gemini_image_response(response(json!({
        "responseId": "resp-1",
        "candidates": [{"content": {"role": "model", "parts": [
            {"thought": true, "inlineData": {"mimeType": "image/png", "data": b64(b"draft")}},
            {"thought": true, "text": "planning"},
            {"text": "Here you go."},
            {"inlineData": {"mimeType": "image/png", "data": b64(b"final")}}
        ]}, "finishReason": "STOP"}],
        "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 1290, "totalTokenCount": 1300,
            "candidatesTokensDetails": [{"modality": "IMAGE", "tokenCount": 1290}]}
    })))
    .unwrap();
    assert_eq!(mapped.images.len(), 1);
    assert_eq!(mapped.images[0].data.as_deref(), Some(&b"final"[..]));
    assert_eq!(mapped.images[0].mime_type.as_deref(), Some("image/png"));
    assert_eq!(mapped.text.as_deref(), Some("Here you go."));
    assert_eq!(mapped.filtered, None);
    assert_eq!(mapped.external_id.as_deref(), Some("resp-1"));
    let usage = mapped.usage.unwrap();
    assert_eq!(usage.output.unwrap()["image_tokens"], 1290);
}

#[test]
fn image_response_safety_and_errors() {
    let filtered = map_gemini_image_response(response(json!({
        "candidates": [{"finishReason": "IMAGE_SAFETY"}]
    })))
    .unwrap();
    assert!(filtered.images.is_empty());
    assert_eq!(filtered.filtered, Some(1));

    assert!(
        map_gemini_image_response(response(json!({
            "promptFeedback": {"blockReason": "PROHIBITED_CONTENT"}
        })))
        .is_err()
    );
    assert!(
        map_gemini_image_response(response(json!({
            "candidates": [{"content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": "!!"}}]}}]
        })))
        .is_err()
    );
}

// ── Transcription ─────────────────────────────────────────────────────

#[test]
fn transcription_request_shape() {
    let request = GaiseTranscriptionRequest {
        languages: vec!["en-US".into()],
        diarize: true,
        timestamps: Some(GaiseTimestampGranularity::Word),
        mode: Some(GaiseTranscriptionMode::Verbatim),
        provider_options: Some(json!({"mediaResolution": "MEDIA_RESOLUTION_LOW"})),
        ..transcription_request()
    };
    let body = gemini_transcription_request(&request).unwrap();
    assert_eq!(
        body["contents"][0]["parts"][0]["inlineData"]["mimeType"],
        "audio/wav"
    );
    assert_eq!(
        body["generationConfig"]["audioTranscriptionConfig"],
        json!({"languageCodes": ["en-US"], "wordTimestamp": true, "diarization": true, "mode": "VERBATIM"})
    );
    assert_eq!(
        body["generationConfig"]["mediaResolution"],
        "MEDIA_RESOLUTION_LOW"
    );
}

#[test]
fn transcription_request_vocabulary_url_and_pcm() {
    let body = gemini_transcription_request(&GaiseTranscriptionRequest {
        audio: Some(GaiseMediaSource::url("gs://bucket/call.flac")),
        keyterms: vec!["oatmilk".into()],
        mode: Some(GaiseTranscriptionMode::Smart),
        ..transcription_request()
    })
    .unwrap();
    assert_eq!(
        body["contents"][0]["parts"][0]["fileData"],
        json!({"mimeType": "audio/flac", "fileUri": "gs://bucket/call.flac"})
    );
    assert_eq!(
        body["generationConfig"]["audioTranscriptionConfig"],
        json!({"customVocabulary": ["oatmilk"], "mode": "SMART"})
    );

    let pcm = gemini_transcription_request(&GaiseTranscriptionRequest {
        audio: Some(GaiseMediaSource::Bytes {
            data: vec![0, 0, 1, 0],
            mime_type: None,
        }),
        format: Some(GaiseAudioFormat::pcm(16_000)),
        ..transcription_request()
    })
    .unwrap();
    assert_eq!(
        pcm["contents"][0]["parts"][0]["inlineData"]["mimeType"],
        "audio/pcm;rate=16000"
    );
    // An empty config object is still sent so the model knows the task.
    assert_eq!(
        pcm["generationConfig"]["audioTranscriptionConfig"],
        json!({})
    );
}

#[test]
fn transcription_request_rejects_incompatible_options() {
    let cases = [
        GaiseTranscriptionRequest {
            mode: Some(GaiseTranscriptionMode::Smart),
            diarize: true,
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            mode: Some(GaiseTranscriptionMode::Smart),
            timestamps: Some(GaiseTimestampGranularity::Word),
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            keyterms: vec!["x".into()],
            diarize: true,
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            keyterms: vec!["x".into()],
            timestamps: Some(GaiseTimestampGranularity::Word),
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            timestamps: Some(GaiseTimestampGranularity::Character),
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            prompt: Some("context".into()),
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            max_speakers: Some(2),
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            multichannel: true,
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            temperature: Some(0.2),
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            audio: Some(GaiseMediaSource::Bytes {
                data: vec![1, 2],
                mime_type: None,
            }),
            format: Some(GaiseAudioFormat {
                codec: "pcm".into(),
                ..Default::default()
            }),
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            audio: None,
            ..transcription_request()
        },
    ];
    for case in cases {
        assert!(gemini_transcription_request(&case).is_err(), "{case:?}");
    }
}

#[test]
fn transcription_response_with_speakers_and_words() {
    let mapped = map_gemini_transcription_response(response(json!({
        "responseId": "r-9",
        "candidates": [{"content": {"parts": [
            {"text": "Hello there.", "audioTranscription": {"text": "Hello there.", "speakerLabel": "spk_1",
                "words": [{"word": "Hello", "startOffset": "0.100s", "endOffset": "0.400s"},
                          {"word": "there.", "startOffset": "0.450s", "endOffset": "0.900s"}]}},
            {"audioTranscription": {"text": "Hi!", "speakerLabel": "spk_2",
                "words": [{"word": "Hi!", "startOffset": "1.2s", "endOffset": "1.5s"}]}}
        ]}}],
        "usageMetadata": {"promptTokenCount": 40, "candidatesTokenCount": 5, "totalTokenCount": 45,
            "promptTokensDetails": [{"modality": "AUDIO", "tokenCount": 40}]}
    })))
    .unwrap();
    assert_eq!(mapped.text, "Hello there.\nHi!");
    assert_eq!(mapped.segments.len(), 2);
    assert_eq!(mapped.segments[0].speaker.as_deref(), Some("spk_1"));
    assert_eq!(mapped.segments[0].start, Some(0.1));
    assert_eq!(mapped.segments[0].end, Some(0.9));
    assert_eq!(mapped.segments[1].text, "Hi!");
    assert_eq!(mapped.words.len(), 3);
    assert_eq!(mapped.words[2].speaker.as_deref(), Some("spk_2"));
    assert_eq!(mapped.words[2].start, Some(1.2));
    assert_eq!(mapped.external_id.as_deref(), Some("r-9"));
    assert_eq!(mapped.usage.unwrap().input.unwrap()["audio_tokens"], 40);
}

#[test]
fn transcription_response_plain() {
    let mapped = map_gemini_transcription_response(response(json!({
        "candidates": [{"content": {"parts": [{"audioTranscription": {"text": "Just text."}}]}}]
    })))
    .unwrap();
    assert_eq!(mapped.text, "Just text.");
    assert!(mapped.segments.is_empty());
    assert!(mapped.words.is_empty());

    let text_only = map_gemini_transcription_response(response(json!({
        "candidates": [{"content": {"parts": [{"text": "Fallback "}, {"text": "text."}]}}]
    })))
    .unwrap();
    assert_eq!(text_only.text, "Fallback text.");

    assert!(
        map_gemini_transcription_response(response(json!({
            "promptFeedback": {"blockReason": "OTHER"}
        })))
        .is_err()
    );
    assert!(
        map_gemini_transcription_response(response(json!({
            "candidates": [{"finishReason": "SAFETY"}]
        })))
        .is_err()
    );
}

// ── Helpers ───────────────────────────────────────────────────────────

#[test]
fn helpers() {
    assert_eq!(parse_duration_seconds("3.5s"), Some(3.5));
    assert_eq!(parse_duration_seconds("0s"), Some(0.0));
    assert_eq!(parse_duration_seconds("x"), None);

    let sniffed = media_part(
        &GaiseMediaSource::Bytes {
            data: b"RIFF\0\0\0\0WEBPVP8 ".to_vec(),
            mime_type: None,
        },
        "image/png",
    )
    .unwrap();
    assert_eq!(sniffed["inlineData"]["mimeType"], "image/webp");
    assert!(media_part(&GaiseMediaSource::url("data:image/png,raw"), "image/png").is_err());

    let mut body = json!({"contents": []});
    merge_generate_content_options(
        &mut body,
        Some(&json!({"temperature": 0.1, "labels": {"a": "b"}})),
    )
    .unwrap();
    assert_eq!(body["generationConfig"]["temperature"], 0.1);
    assert_eq!(body["labels"]["a"], "b");
}

// ── HTTP ──────────────────────────────────────────────────────────────

/// One-shot HTTP server returning `body`; yields the request it received.
async fn serve_once(body: serde_json::Value) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = socket.read(&mut chunk).await.unwrap();
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some(split) = text.find("\r\n\r\n") {
                let length = text[..split]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if buf.len() >= split + 4 + length {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        let payload = body.to_string();
        let reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            payload.len(),
            payload
        );
        socket.write_all(reply.as_bytes()).await.unwrap();
        String::from_utf8_lossy(&buf).to_string()
    });
    (format!("http://{addr}/v1beta"), handle)
}

#[tokio::test]
async fn generate_images_over_http() {
    let (url, server) = serve_once(json!({
        "candidates": [{"content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": b64(b"img")}}]}}]
    }))
    .await;
    let client = GaiseClientGemini::new(url, "KEY".into());
    let result = client
        .generate_images(&GaiseImageRequest {
            aspect_ratio: Some("1:1".into()),
            ..image_request()
        })
        .await
        .unwrap();
    assert_eq!(result.images[0].data.as_deref(), Some(&b"img"[..]));
    let request = server.await.unwrap();
    assert!(request.starts_with(
        "POST /v1beta/models/gemini-nano-banana-2.1:generateContent?key=KEY HTTP/1.1"
    ));
    assert!(request.contains("\"imageConfig\":{\"aspectRatio\":\"1:1\"}"));
}

#[tokio::test]
async fn transcribe_over_http_and_video_unsupported() {
    let (url, server) = serve_once(json!({
        "candidates": [{"content": {"parts": [{"audioTranscription": {"text": "hello"}}]}}]
    }))
    .await;
    let client = GaiseClientGemini::new(url, "KEY".into());
    let result = client.transcribe(&transcription_request()).await.unwrap();
    assert_eq!(result.text, "hello");
    let request = server.await.unwrap();
    assert!(request.starts_with("POST /v1beta/models/gemini-3.5-transcribe:generateContent"));
    assert!(request.contains("audioTranscriptionConfig"));

    let video = client
        .start_video(&GaiseVideoRequest {
            model: "veo-3.1-generate-preview".into(),
            prompt: Some("waves".into()),
            ..Default::default()
        })
        .await;
    assert!(video.is_err());
}
