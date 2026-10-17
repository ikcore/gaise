//! Hermetic tests for the Bedrock image and video builders and mappers.

use gaise_core::contracts::{
    GaiseImageRequest, GaiseJobStatus, GaiseMediaSource, GaiseVideoRequest,
};
use gaise_provider_bedrock::media::*;
use serde_json::json;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 1, 2, 3];

fn image_request(model: &str) -> GaiseImageRequest {
    GaiseImageRequest {
        model: model.into(),
        prompt: "A car made out of vegetables.".into(),
        ..Default::default()
    }
}

fn video_request() -> GaiseVideoRequest {
    GaiseVideoRequest {
        model: "luma.ray-v2:0".into(),
        prompt: Some("A tiger walking in snow".into()),
        output_uri: Some("s3://bucket/videos".into()),
        ..Default::default()
    }
}

#[test]
fn sd35_text_to_image_body() {
    let request = GaiseImageRequest {
        aspect_ratio: Some("16:9".into()),
        negative_prompt: Some("blurry".into()),
        seed: Some(42),
        output_format: Some("image/webp".into()),
        provider_options: Some(json!({"extra": true})),
        ..image_request("stability.sd3-5-large-v1:0")
    };
    let body = stability_image_body(&request).unwrap();
    assert_eq!(
        body,
        json!({
            "prompt": "A car made out of vegetables.",
            "aspect_ratio": "16:9",
            "negative_prompt": "blurry",
            "seed": 42,
            "output_format": "webp",
            "extra": true
        })
    );
    assert_eq!(stability_output_mime(&body), "image/webp");
    assert!(body.get("mode").is_none());
}

#[test]
fn sd35_image_to_image_body() {
    let request = GaiseImageRequest {
        images: vec![GaiseMediaSource::bytes(PNG.to_vec(), "image/png")],
        provider_options: Some(json!({"strength": 0.7})),
        ..image_request("us.stability.sd3-5-large-v1:0")
    };
    let body = stability_image_body(&request).unwrap();
    assert_eq!(body["mode"], "image-to-image");
    assert_eq!(body["image"], "iVBORwECAw==");
    assert_eq!(body["strength"], 0.7);
    assert!(body.get("aspect_ratio").is_none());
}

#[test]
fn ultra_image_to_image_has_no_mode_and_default_strength() {
    let url = GaiseMediaSource::url("data:image/png;base64,iVBORwECAw==");
    let request = GaiseImageRequest {
        images: vec![url],
        ..image_request("stability.stable-image-ultra-v1:1")
    };
    let body = stability_image_body(&request).unwrap();
    assert!(body.get("mode").is_none());
    assert_eq!(body["image"], "iVBORwECAw==");
    assert_eq!(body["strength"], 0.35);
}

#[test]
fn stability_rejects_what_it_cannot_honour() {
    let png = || GaiseMediaSource::bytes(PNG.to_vec(), "image/png");
    let cases = [
        GaiseImageRequest {
            mask: Some(png()),
            images: vec![png()],
            ..image_request("stability.sd3-5-large-v1:0")
        },
        GaiseImageRequest {
            images: vec![png()],
            ..image_request("stability.stable-image-core-v1:1")
        },
        GaiseImageRequest {
            images: vec![png()],
            aspect_ratio: Some("1:1".into()),
            ..image_request("stability.sd3-5-large-v1:0")
        },
        GaiseImageRequest {
            aspect_ratio: Some("7:3".into()),
            ..image_request("stability.sd3-5-large-v1:0")
        },
        GaiseImageRequest {
            output_format: Some("image/webp".into()),
            ..image_request("stability.stable-image-ultra-v1:1")
        },
        GaiseImageRequest {
            seed: Some(4_294_967_295),
            ..image_request("stability.sd3-5-large-v1:0")
        },
        GaiseImageRequest {
            resolution: Some("2K".into()),
            ..image_request("stability.sd3-5-large-v1:0")
        },
        GaiseImageRequest {
            images: vec![GaiseMediaSource::url("https://example.com/a.png")],
            ..image_request("stability.sd3-5-large-v1:0")
        },
    ];
    for request in cases {
        assert!(stability_image_body(&request).is_err(), "{request:?}");
    }
    let canvas = stability_image_body(&image_request("amazon.nova-canvas-v1:0")).unwrap_err();
    assert!(canvas.contains("end of life"), "{canvas}");
}

#[test]
fn size_maps_to_aspect_ratio() {
    let request = GaiseImageRequest {
        size: Some("1920x1080".into()),
        ..image_request("stability.stable-image-core-v1:1")
    };
    assert_eq!(
        stability_image_body(&request).unwrap()["aspect_ratio"],
        "16:9"
    );
    let bad = GaiseImageRequest {
        size: Some("1000x700".into()),
        ..image_request("stability.stable-image-core-v1:1")
    };
    assert!(stability_image_body(&bad).is_err());
}

#[test]
fn seed_is_offset_per_call() {
    let body = json!({"prompt": "p", "seed": 7});
    assert_eq!(stability_body_for_index(&body, 0)["seed"], 7);
    assert_eq!(stability_body_for_index(&body, 2)["seed"], 9);
    let random = json!({"prompt": "p"});
    assert!(stability_body_for_index(&random, 3).get("seed").is_none());
}

#[test]
fn maps_stability_images_and_filters() {
    let ok = json!({"seeds": [2130420379u64], "finish_reasons": [null], "images": ["AQID"]});
    let response = map_stability_image_response(&ok, "image/png").unwrap();
    assert_eq!(response.images.len(), 1);
    assert_eq!(response.images[0].data.as_deref(), Some(&[1u8, 2, 3][..]));
    assert_eq!(response.images[0].seed, Some(2130420379));
    assert_eq!(response.images[0].mime_type.as_deref(), Some("image/png"));
    assert_eq!(response.filtered, None);

    let filtered = json!({"finish_reasons": ["Filter reason: prompt"]});
    let response = map_stability_image_response(&filtered, "image/png").unwrap();
    assert!(response.images.is_empty());
    assert_eq!(response.filtered, Some(1));

    let failed = json!({"finish_reasons": ["Inference error"]});
    assert!(
        map_stability_image_response(&failed, "image/png")
            .unwrap_err()
            .contains("Inference error")
    );

    let merged = merge_image_responses(vec![
        map_stability_image_response(&ok, "image/png").unwrap(),
        map_stability_image_response(&filtered, "image/png").unwrap(),
    ]);
    assert_eq!(merged.images.len(), 1);
    assert_eq!(merged.filtered, Some(1));
}

#[test]
fn luma_text_to_video_input() {
    let request = GaiseVideoRequest {
        aspect_ratio: Some("16:9".into()),
        duration_seconds: Some(9),
        resolution: Some("720P".into()),
        provider_options: Some(json!({"loop": true})),
        ..video_request()
    };
    assert_eq!(
        luma_model_input(&request).unwrap(),
        json!({
            "prompt": "A tiger walking in snow",
            "aspect_ratio": "16:9",
            "duration": "9s",
            "resolution": "720p",
            "loop": true
        })
    );
    assert_eq!(video_output_uri(&request).unwrap(), "s3://bucket/videos");
}

#[test]
fn luma_keyframes() {
    let request = GaiseVideoRequest {
        image: Some(GaiseMediaSource::bytes(
            vec![0xFF, 0xD8, 0xFF, 0],
            "image/jpeg",
        )),
        last_frame: Some(GaiseMediaSource::Bytes {
            data: PNG.to_vec(),
            mime_type: None,
        }),
        ..video_request()
    };
    let input = luma_model_input(&request).unwrap();
    assert_eq!(
        input["keyframes"]["frame0"],
        json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "/9j/AA=="}})
    );
    assert_eq!(
        input["keyframes"]["frame1"]["source"]["media_type"],
        "image/png"
    );
}

#[test]
fn luma_rejections_and_output_uri() {
    let cases = [
        GaiseVideoRequest {
            duration_seconds: Some(6),
            ..video_request()
        },
        GaiseVideoRequest {
            resolution: Some("1080p".into()),
            ..video_request()
        },
        GaiseVideoRequest {
            aspect_ratio: Some("2:3".into()),
            ..video_request()
        },
        GaiseVideoRequest {
            seed: Some(1),
            ..video_request()
        },
        GaiseVideoRequest {
            generate_audio: Some(true),
            ..video_request()
        },
        GaiseVideoRequest {
            model: "amazon.nova-reel-v1:1".into(),
            ..video_request()
        },
    ];
    for request in cases {
        assert!(luma_model_input(&request).is_err(), "{request:?}");
    }
    let missing = GaiseVideoRequest {
        output_uri: None,
        ..video_request()
    };
    assert!(
        video_output_uri(&missing)
            .unwrap_err()
            .contains("output_uri")
    );
    let gcs = GaiseVideoRequest {
        output_uri: Some("gs://bucket".into()),
        ..video_request()
    };
    assert!(video_output_uri(&gcs).is_err());
}

#[test]
fn maps_async_invoke_status() {
    assert_eq!(
        map_async_invoke_status("InProgress"),
        GaiseJobStatus::Running
    );
    assert_eq!(
        map_async_invoke_status("Completed"),
        GaiseJobStatus::Completed
    );
    assert_eq!(map_async_invoke_status("Failed"), GaiseJobStatus::Failed);
    assert_eq!(map_async_invoke_status("Submitted"), GaiseJobStatus::Queued);

    let arn = "arn:aws:bedrock:us-west-2:123456789012:async-invoke/abc123";
    let done = map_async_invoke(
        arn,
        "luma.ray-v2:0",
        "Completed",
        None,
        Some("s3://bucket/videos/abc123"),
    );
    assert_eq!(done.id, arn);
    assert_eq!(done.status, GaiseJobStatus::Completed);
    assert_eq!(
        done.videos[0].url.as_deref(),
        Some("s3://bucket/videos/abc123/output.mp4")
    );
    assert_eq!(done.videos[0].mime_type.as_deref(), Some("video/mp4"));
    assert_eq!(
        async_video_uri("s3://bucket/videos/", arn),
        "s3://bucket/videos/abc123/output.mp4"
    );

    let running = map_async_invoke(arn, "luma.ray-v2:0", "InProgress", None, Some("s3://b"));
    assert!(running.videos.is_empty());
    assert!(running.error.is_none());

    let failed = map_async_invoke(
        arn,
        "luma.ray-v2:0",
        "Failed",
        Some("The generated content has been blocked by our content filters."),
        None,
    );
    assert_eq!(failed.status, GaiseJobStatus::Failed);
    assert!(failed.error.unwrap().contains("blocked"));
    assert_eq!(failed.filtered, Some(1));
}
