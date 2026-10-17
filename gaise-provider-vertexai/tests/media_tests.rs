use base64::Engine;
use gaise_core::contracts::{
    GaiseImageRequest, GaiseJobStatus, GaiseMediaSource, GaiseTimestampGranularity,
    GaiseTranscriptionMode, GaiseTranscriptionRequest, GaiseVideoRequest,
};
use gaise_provider_vertexai::vertexai_media::{
    VertexMediaResponse, VertexVideoOperation, map_vertex_image_response,
    map_vertex_transcription_response, map_vertex_video_operation, map_vertex_video_start,
    model_from_operation_name, veo_media, vertex_image_request, vertex_transcription_request,
    vertex_video_poll_request, vertex_video_request,
};
use serde_json::json;

const OPERATION: &str =
    "projects/p/locations/us-central1/publishers/google/models/veo-3.1-generate-001/operations/123";

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
        audio: Some(GaiseMediaSource::url(
            "gs://cloud-samples-data/generative-ai/audio/coffee_order.wav",
        )),
        ..Default::default()
    }
}

fn video_request() -> GaiseVideoRequest {
    GaiseVideoRequest {
        model: "veo-3.1-generate-001".into(),
        prompt: Some("waves on a beach".into()),
        ..Default::default()
    }
}

fn operation(value: serde_json::Value) -> VertexVideoOperation {
    serde_json::from_value(value).unwrap()
}

fn response(value: serde_json::Value) -> VertexMediaResponse {
    serde_json::from_value(value).unwrap()
}

// ── Images ────────────────────────────────────────────────────────────

#[test]
fn image_request_shape() {
    let body = vertex_image_request(&GaiseImageRequest {
        images: vec![GaiseMediaSource::url("gs://bucket/ref.jpg")],
        aspect_ratio: Some("9:21".into()),
        resolution: Some("4k".into()),
        provider_options: Some(json!({"labels": {"team": "media"}})),
        ..image_request()
    })
    .unwrap();
    assert_eq!(
        body["contents"][0]["parts"][0]["text"],
        "a lighthouse at dusk"
    );
    assert_eq!(
        body["contents"][0]["parts"][1]["fileData"],
        json!({"mimeType": "image/jpeg", "fileUri": "gs://bucket/ref.jpg"})
    );
    assert_eq!(
        body["generationConfig"],
        json!({"responseModalities": ["TEXT", "IMAGE"], "imageConfig": {"aspectRatio": "9:21", "imageSize": "4K"}})
    );
    assert_eq!(body["labels"]["team"], "media");
}

#[test]
fn image_request_errors() {
    for case in [
        GaiseImageRequest {
            n: Some(3),
            ..image_request()
        },
        GaiseImageRequest {
            seed: Some(1),
            ..image_request()
        },
        GaiseImageRequest {
            mask: Some(GaiseMediaSource::url("gs://b/m.png")),
            ..image_request()
        },
        GaiseImageRequest {
            resolution: Some("3K".into()),
            ..image_request()
        },
    ] {
        assert!(vertex_image_request(&case).is_err(), "{case:?}");
    }
    let seeded = vertex_image_request(&GaiseImageRequest {
        model: "gemini-2.5-flash-image".into(),
        seed: Some(1),
        ..image_request()
    })
    .unwrap();
    assert_eq!(seeded["generationConfig"]["seed"], 1);
}

#[test]
fn image_response_mapping() {
    let mapped = map_vertex_image_response(response(json!({
        "responseId": "vx-1",
        "candidates": [{"content": {"role": "model", "parts": [
            {"text": "Step 1"},
            {"inlineData": {"mimeType": "image/png", "data": b64(b"one")}},
            {"inlineData": {"mimeType": "image/png", "data": b64(b"two")}}
        ]}}],
        "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 2580, "totalTokenCount": 2585}
    })))
    .unwrap();
    assert_eq!(mapped.images.len(), 2);
    assert_eq!(mapped.images[1].data.as_deref(), Some(&b"two"[..]));
    assert_eq!(mapped.text.as_deref(), Some("Step 1"));
    assert_eq!(mapped.external_id.as_deref(), Some("vx-1"));
    assert!(mapped.usage.is_some());

    let filtered = map_vertex_image_response(response(json!({
        "candidates": [{"finishReason": "PROHIBITED_CONTENT"}]
    })))
    .unwrap();
    assert_eq!(filtered.filtered, Some(1));
    assert!(
        map_vertex_image_response(response(
            json!({"promptFeedback": {"blockReason": "SAFETY"}})
        ))
        .is_err()
    );
}

// ── Transcription ─────────────────────────────────────────────────────

#[test]
fn transcription_request_shape() {
    // The Vertex model card combines diarization with custom vocabulary.
    let body = vertex_transcription_request(&GaiseTranscriptionRequest {
        diarize: true,
        languages: vec!["en-US".into()],
        keyterms: vec!["oatmilk".into(), "oz".into()],
        ..transcription_request()
    })
    .unwrap();
    assert_eq!(
        body["contents"][0]["parts"][0]["fileData"],
        json!({"mimeType": "audio/wav", "fileUri": "gs://cloud-samples-data/generative-ai/audio/coffee_order.wav"})
    );
    assert_eq!(
        body["generationConfig"]["audioTranscriptionConfig"],
        json!({"languageCodes": ["en-US"], "customVocabulary": ["oatmilk", "oz"], "diarization": true})
    );

    let inline = vertex_transcription_request(&GaiseTranscriptionRequest {
        audio: Some(GaiseMediaSource::bytes(vec![1, 2, 3], "audio/mp3")),
        timestamps: Some(GaiseTimestampGranularity::Word),
        ..transcription_request()
    })
    .unwrap();
    assert_eq!(
        inline["contents"][0]["parts"][0]["inlineData"],
        json!({"mimeType": "audio/mp3", "data": b64(&[1, 2, 3])})
    );
    assert_eq!(
        inline["generationConfig"]["audioTranscriptionConfig"]["wordTimestamp"],
        true
    );
}

#[test]
fn transcription_request_errors() {
    for case in [
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
            timestamps: Some(GaiseTimestampGranularity::Word),
            ..transcription_request()
        },
        GaiseTranscriptionRequest {
            prompt: Some("context".into()),
            ..transcription_request()
        },
    ] {
        assert!(vertex_transcription_request(&case).is_err(), "{case:?}");
    }
}

#[test]
fn transcription_response_mapping() {
    let mapped = map_vertex_transcription_response(response(json!({
        "candidates": [{"content": {"parts": [
            {"audioTranscription": {"text": "One latte.", "speakerLabel": "spk_1"}},
            {"audioTranscription": {"text": "Sure.", "speakerLabel": "spk_2",
                "words": [{"word": "Sure.", "startOffset": "2s", "endOffset": "2.250s"}]}}
        ]}}]
    })))
    .unwrap();
    assert_eq!(mapped.text, "One latte.\nSure.");
    assert_eq!(mapped.segments.len(), 2);
    assert_eq!(mapped.segments[0].start, None);
    assert_eq!(mapped.segments[1].end, Some(2.25));
    assert_eq!(mapped.words[0].speaker.as_deref(), Some("spk_2"));
}

// ── Video ─────────────────────────────────────────────────────────────

#[test]
fn video_request_text_to_video() {
    let body = vertex_video_request(&GaiseVideoRequest {
        n: Some(2),
        duration_seconds: Some(8),
        aspect_ratio: Some("16:9".into()),
        resolution: Some("1080P".into()),
        negative_prompt: Some("text overlays".into()),
        generate_audio: Some(true),
        seed: Some(42),
        output_uri: Some("gs://bucket/out/".into()),
        provider_options: Some(json!({
            "personGeneration": "allow_adult",
            "instance": {"cameraControl": "fixed"}
        })),
        ..video_request()
    })
    .unwrap();
    assert_eq!(
        body,
        json!({
            "instances": [{"prompt": "waves on a beach", "cameraControl": "fixed"}],
            "parameters": {
                "sampleCount": 2, "durationSeconds": 8, "aspectRatio": "16:9",
                "resolution": "1080p", "negativePrompt": "text overlays",
                "generateAudio": true, "seed": 42, "storageUri": "gs://bucket/out/",
                "personGeneration": "allow_adult"
            }
        })
    );
}

#[test]
fn video_request_media_inputs() {
    let frames = vertex_video_request(&GaiseVideoRequest {
        image: Some(GaiseMediaSource::bytes(
            vec![0x89, b'P', b'N', b'G'],
            "image/png",
        )),
        last_frame: Some(GaiseMediaSource::url("gs://bucket/last.jpg")),
        ..video_request()
    })
    .unwrap();
    let instance = &frames["instances"][0];
    assert_eq!(
        instance["image"],
        json!({"bytesBase64Encoded": b64(&[0x89, b'P', b'N', b'G']), "mimeType": "image/png"})
    );
    assert_eq!(
        instance["lastFrame"],
        json!({"gcsUri": "gs://bucket/last.jpg", "mimeType": "image/jpeg"})
    );
    assert!(frames.get("parameters").is_none());

    let references = vertex_video_request(&GaiseVideoRequest {
        reference_images: vec![
            GaiseMediaSource::url("gs://bucket/a.png"),
            GaiseMediaSource::url("gs://bucket/b.png"),
        ],
        ..video_request()
    })
    .unwrap();
    assert_eq!(
        references["instances"][0]["referenceImages"][1],
        json!({"image": {"gcsUri": "gs://bucket/b.png", "mimeType": "image/png"}, "referenceType": "asset"})
    );

    let extend = vertex_video_request(&GaiseVideoRequest {
        prompt: None,
        video: Some(GaiseMediaSource::url("gs://bucket/clip.mp4")),
        ..video_request()
    })
    .unwrap();
    assert_eq!(
        extend["instances"][0],
        json!({"video": {"gcsUri": "gs://bucket/clip.mp4", "mimeType": "video/mp4"}})
    );
}

#[test]
fn video_request_errors() {
    let gs = || Some(GaiseMediaSource::url("gs://bucket/x.png"));
    for case in [
        GaiseVideoRequest {
            image: gs(),
            video: Some(GaiseMediaSource::url("gs://bucket/v.mp4")),
            ..video_request()
        },
        GaiseVideoRequest {
            last_frame: gs(),
            ..video_request()
        },
        GaiseVideoRequest {
            image: gs(),
            reference_images: vec![GaiseMediaSource::url("gs://bucket/r.png")],
            ..video_request()
        },
        GaiseVideoRequest {
            reference_images: vec![GaiseMediaSource::url("gs://bucket/r.png"); 4],
            ..video_request()
        },
        GaiseVideoRequest {
            prompt: None,
            image: gs(),
            reference_images: vec![GaiseMediaSource::url("gs://bucket/r.png")],
            ..video_request()
        },
        GaiseVideoRequest {
            n: Some(5),
            ..video_request()
        },
        GaiseVideoRequest {
            output_uri: Some("s3://bucket/out".into()),
            ..video_request()
        },
        GaiseVideoRequest {
            image: Some(GaiseMediaSource::url("https://example.com/a.png")),
            ..video_request()
        },
        GaiseVideoRequest {
            prompt: None,
            ..video_request()
        },
        GaiseVideoRequest {
            provider_options: Some(json!({"instance": 1})),
            ..video_request()
        },
    ] {
        assert!(vertex_video_request(&case).is_err(), "{case:?}");
    }
    assert!(veo_media(&GaiseMediaSource::bytes(vec![], "image/png"), "image/png").is_err());
}

#[test]
fn video_start_and_poll_mapping() {
    let started = map_vertex_video_start(
        "veo-3.1-generate-001",
        operation(json!({"name": OPERATION})),
    )
    .unwrap();
    assert_eq!(started.id, OPERATION);
    assert_eq!(started.model, "veo-3.1-generate-001");
    assert_eq!(started.status, GaiseJobStatus::Queued);
    assert!(map_vertex_video_start("veo-3.1-generate-001", operation(json!({}))).is_err());

    assert_eq!(
        vertex_video_poll_request(OPERATION),
        json!({"operationName": OPERATION})
    );
    assert_eq!(
        model_from_operation_name(OPERATION),
        Some("veo-3.1-generate-001")
    );
    assert_eq!(model_from_operation_name("operations/1"), None);

    let running = map_vertex_video_operation("", operation(json!({"name": OPERATION}))).unwrap();
    assert_eq!(running.status, GaiseJobStatus::Running);
    assert_eq!(running.model, "veo-3.1-generate-001");

    let done = map_vertex_video_operation(
        "veo-3.1-generate-001",
        operation(json!({
            "name": OPERATION,
            "done": true,
            "response": {
                "@type": "type.googleapis.com/cloud.ai.large_models.vision.GenerateVideoResponse",
                "raiMediaFilteredCount": 1,
                "videos": [
                    {"gcsUri": "gs://bucket/out/sample_0.mp4", "mimeType": "video/mp4"},
                    {"bytesBase64Encoded": b64(b"mp4")}
                ]
            }
        })),
    )
    .unwrap();
    assert_eq!(done.status, GaiseJobStatus::Completed);
    assert_eq!(done.filtered, Some(1));
    assert_eq!(done.progress, Some(100));
    assert_eq!(
        done.videos[0].url.as_deref(),
        Some("gs://bucket/out/sample_0.mp4")
    );
    assert_eq!(done.videos[1].data.as_deref(), Some(&b"mp4"[..]));
    assert_eq!(done.videos[1].mime_type.as_deref(), Some("video/mp4"));
}

#[test]
fn video_failures() {
    let failed = map_vertex_video_operation(
        "veo-3.1-generate-001",
        operation(json!({"name": OPERATION, "done": true,
            "error": {"code": 3, "message": "Invalid duration"}})),
    )
    .unwrap();
    assert_eq!(failed.status, GaiseJobStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("3: Invalid duration"));

    let all_filtered = map_vertex_video_operation(
        "veo-3.1-generate-001",
        operation(json!({"name": OPERATION, "done": true,
            "response": {"raiMediaFilteredCount": 2, "raiMediaFilteredReasons": ["celebrity"]}})),
    )
    .unwrap();
    assert_eq!(all_filtered.status, GaiseJobStatus::Failed);
    assert_eq!(all_filtered.filtered, Some(2));
    assert!(all_filtered.error.unwrap().contains("celebrity"));

    assert!(
        map_vertex_video_operation(
            "m",
            operation(
                json!({"done": true, "response": {"videos": [{"bytesBase64Encoded": "!!"}]}})
            )
        )
        .is_err()
    );
}
