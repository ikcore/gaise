//! Transcription and image generation (`/v1/audio/transcriptions`,
//! `/v1/images/generations`, `/v1/images/edits`). Hermetic: the HTTP cases
//! run against a local server.

use axum::{
    Json, Router,
    body::Bytes,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use base64::Engine;
use gaise_core::GaiseClient;
use gaise_core::contracts::{
    GaiseAudioFormat, GaiseImageRequest, GaiseMediaSource, GaiseTimestampGranularity,
    GaiseTranscriptionMode, GaiseTranscriptionRequest,
};
use gaise_provider_openai::media::{
    OpenAIImageEndpoint, OpenAITranscriptionFamily, decode_data_url, image_request,
    map_image_response, map_transcription_response, openai_audio_file_type, openai_image_size,
    openai_transcription_family, transcription_form,
};
use gaise_provider_openai::openai_client::GaiseClientOpenAI;
use serde_json::{Value, json};

fn wav() -> Vec<u8> {
    let mut data = b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec();
    data.extend_from_slice(&[0u8; 32]);
    data
}

fn transcription(model: &str) -> GaiseTranscriptionRequest {
    GaiseTranscriptionRequest {
        model: model.into(),
        audio: Some(GaiseMediaSource::Bytes {
            data: wav(),
            mime_type: None,
        }),
        ..Default::default()
    }
}

fn image(model: &str) -> GaiseImageRequest {
    GaiseImageRequest {
        model: model.into(),
        prompt: "a lighthouse at dusk".into(),
        ..Default::default()
    }
}

// ── Transcription request building ────────────────────────────────────

#[test]
fn families_follow_the_model_id() {
    use OpenAITranscriptionFamily::*;
    assert_eq!(openai_transcription_family("gpt-transcribe"), GptTranscribe);
    assert_eq!(
        openai_transcription_family("gpt-4o-transcribe-diarize"),
        Diarize
    );
    assert_eq!(openai_transcription_family("whisper-1"), Whisper);
    assert_eq!(openai_transcription_family("gpt-4o-transcribe"), Standard);
    assert_eq!(
        openai_transcription_family("gpt-4o-mini-transcribe-2025-12-15"),
        Standard
    );
}

#[test]
fn gpt_transcribe_sends_languages_keywords_and_prompt() {
    let mut request = transcription("gpt-transcribe");
    request.languages = vec!["en-US".into(), "fr".into(), "zh-CN".into()];
    request.keyterms = vec!["AC-42".into(), "premium plan".into()];
    request.prompt = Some("A billing call.".into());
    request.temperature = Some(0.2);
    let form = transcription_form(&request).unwrap();
    assert_eq!(form.value("model"), Some("gpt-transcribe"));
    assert_eq!(form.values("languages[]"), vec!["en", "fr", "zh-cn"]);
    assert!(form.value("language").is_none(), "never both");
    assert_eq!(form.values("keywords[]"), vec!["AC-42", "premium plan"]);
    assert_eq!(form.value("prompt"), Some("A billing call."));
    assert_eq!(form.response_format(), "json");
    assert_eq!(form.value("temperature"), Some("0.2"));
    assert_eq!(form.file_name, "audio.wav");
    assert_eq!(form.file_mime, "audio/wav");
}

#[test]
fn keywords_with_forbidden_characters_are_rejected() {
    let mut request = transcription("gpt-transcribe");
    request.keyterms = vec!["<tag>".into()];
    assert!(transcription_form(&request).unwrap_err().contains("'<'"));
    request.keyterms = vec!["two\nlines".into()];
    assert!(transcription_form(&request).is_err());
}

#[test]
fn single_language_models_take_one_iso_639_1_code() {
    let mut request = transcription("gpt-4o-transcribe");
    request.languages = vec!["de-AT".into()];
    let form = transcription_form(&request).unwrap();
    assert_eq!(form.value("language"), Some("de"));
    assert!(form.values("languages[]").is_empty());
    assert_eq!(form.response_format(), "json");

    request.languages = vec!["de".into(), "en".into()];
    assert!(
        transcription_form(&request)
            .unwrap_err()
            .contains("one language")
    );
    let mut keywords = transcription("whisper-1");
    keywords.keyterms = vec!["x".into()];
    assert!(
        transcription_form(&keywords)
            .unwrap_err()
            .contains("keywords")
    );
}

#[test]
fn whisper_uses_verbose_json_and_timestamp_granularities() {
    let mut request = transcription("whisper-1");
    let form = transcription_form(&request).unwrap();
    assert_eq!(form.response_format(), "verbose_json");
    assert!(form.values("timestamp_granularities[]").is_empty());

    request.timestamps = Some(GaiseTimestampGranularity::Word);
    let form = transcription_form(&request).unwrap();
    assert_eq!(
        form.values("timestamp_granularities[]"),
        vec!["word", "segment"]
    );
    request.timestamps = Some(GaiseTimestampGranularity::Segment);
    let form = transcription_form(&request).unwrap();
    assert_eq!(form.values("timestamp_granularities[]"), vec!["segment"]);
    request.timestamps = Some(GaiseTimestampGranularity::Character);
    assert!(transcription_form(&request).is_err());
    request.timestamps = None;
    request.diarize = true;
    assert!(
        transcription_form(&request)
            .unwrap_err()
            .contains("gpt-4o-transcribe-diarize")
    );
}

#[test]
fn diarize_model_requests_diarized_json_with_auto_chunking() {
    let mut request = transcription("gpt-4o-transcribe-diarize");
    request.diarize = true;
    let form = transcription_form(&request).unwrap();
    assert_eq!(form.response_format(), "diarized_json");
    assert_eq!(form.value("chunking_strategy"), Some("auto"));

    let plain = transcription_form(&transcription("gpt-4o-transcribe-diarize")).unwrap();
    assert_eq!(plain.response_format(), "json");
    assert!(plain.value("chunking_strategy").is_none());

    request.prompt = Some("hint".into());
    assert!(transcription_form(&request).unwrap_err().contains("prompt"));
    request.prompt = None;
    request.timestamps = Some(GaiseTimestampGranularity::Word);
    assert!(
        transcription_form(&request)
            .unwrap_err()
            .contains("whisper-1")
    );
}

#[test]
fn models_that_cannot_diarize_or_timestamp_fail_clearly() {
    for model in [
        "gpt-transcribe",
        "gpt-4o-transcribe",
        "gpt-4o-mini-transcribe",
    ] {
        let mut request = transcription(model);
        request.diarize = true;
        assert!(
            transcription_form(&request)
                .unwrap_err()
                .contains("cannot label speakers"),
            "{model}"
        );
        request.diarize = false;
        request.timestamps = Some(GaiseTimestampGranularity::Segment);
        assert!(
            transcription_form(&request)
                .unwrap_err()
                .contains("no timestamps"),
            "{model}"
        );
        request.timestamps = Some(GaiseTimestampGranularity::None);
        assert!(transcription_form(&request).is_ok(), "{model}");
    }
}

#[test]
fn unsupported_transcription_inputs_are_rejected() {
    let mut url = transcription("gpt-transcribe");
    url.audio = Some(GaiseMediaSource::url("https://example.com/a.mp3"));
    assert!(
        transcription_form(&url)
            .unwrap_err()
            .contains("inline audio bytes")
    );

    let mut data_url = transcription("gpt-transcribe");
    data_url.audio = Some(GaiseMediaSource::url(format!(
        "data:audio/flac;base64,{}",
        base64::prelude::BASE64_STANDARD.encode(b"fLaC....")
    )));
    let form = transcription_form(&data_url).unwrap();
    assert_eq!(form.file_name, "audio.flac");
    assert_eq!(form.file, b"fLaC....");

    let mut pcm = transcription("gpt-transcribe");
    pcm.format = Some(GaiseAudioFormat::pcm(16_000));
    assert!(transcription_form(&pcm).unwrap_err().contains("headerless"));

    let mut unknown = transcription("gpt-transcribe");
    unknown.audio = Some(GaiseMediaSource::Bytes {
        data: vec![1, 2, 3, 4],
        mime_type: None,
    });
    assert!(
        transcription_form(&unknown)
            .unwrap_err()
            .contains("audio format")
    );

    let mut big = transcription("gpt-transcribe");
    big.audio = Some(GaiseMediaSource::bytes(
        vec![0; 25 * 1024 * 1024 + 1],
        "audio/wav",
    ));
    assert!(transcription_form(&big).unwrap_err().contains("25 MB"));

    let mut multichannel = transcription("gpt-transcribe");
    multichannel.multichannel = true;
    assert!(transcription_form(&multichannel).is_err());
    let mut mode = transcription("gpt-transcribe");
    mode.mode = Some(GaiseTranscriptionMode::Smart);
    assert!(transcription_form(&mode).is_err());
    let mut speakers = transcription("gpt-4o-transcribe-diarize");
    speakers.diarize = true;
    speakers.max_speakers = Some(2);
    assert!(transcription_form(&speakers).is_err());
}

#[test]
fn audio_types_come_from_mime_codec_or_magic_bytes() {
    assert_eq!(
        openai_audio_file_type(&[], Some("audio/mpeg"), None),
        Some(("mp3", "audio/mpeg"))
    );
    assert_eq!(
        openai_audio_file_type(&[], None, Some("webm")),
        Some(("webm", "audio/webm"))
    );
    assert_eq!(
        openai_audio_file_type(b"ID3\x04", None, None),
        Some(("mp3", "audio/mpeg"))
    );
    assert_eq!(
        openai_audio_file_type(b"OggS\x00", None, None),
        Some(("ogg", "audio/ogg"))
    );
    assert_eq!(
        openai_audio_file_type(b"\x00\x00\x00\x20ftypM4A ", None, None),
        Some(("m4a", "audio/mp4"))
    );
    assert_eq!(openai_audio_file_type(b"nope", None, None), None);
    assert_eq!(
        decode_data_url("data:audio/wav;base64,AQID"),
        Some((vec![1, 2, 3], Some("audio/wav".into())))
    );
    assert_eq!(decode_data_url("https://x"), None);
}

#[test]
fn provider_options_replace_and_extend_form_fields() {
    let mut request = transcription("gpt-4o-transcribe");
    request.provider_options = Some(json!({
        "include": ["logprobs"],
        "response_format": "text",
        "temperature": 0,
        "language": null
    }));
    request.languages = vec!["en".into()];
    let form = transcription_form(&request).unwrap();
    assert_eq!(form.values("include[]"), vec!["logprobs"]);
    assert_eq!(form.values("response_format"), vec!["text"]);
    assert_eq!(form.value("temperature"), Some("0"));
    assert!(form.value("language").is_none());

    request.provider_options = Some(json!("not an object"));
    assert!(transcription_form(&request).is_err());
}

#[test]
fn multipart_encoding_carries_fields_and_file() {
    let mut request = transcription("gpt-transcribe");
    request.languages = vec!["en".into()];
    let form = transcription_form(&request).unwrap();
    let body = form.encode("XYZ");
    let parts = parse_multipart(&body, "XYZ");
    assert_eq!(field(&parts, "model"), Some("gpt-transcribe".into()));
    assert_eq!(field(&parts, "languages[]"), Some("en".into()));
    let file = parts.iter().find(|p| p.name == "file").unwrap();
    assert_eq!(file.filename.as_deref(), Some("audio.wav"));
    assert_eq!(file.data, wav());
}

// ── Transcription response mapping ────────────────────────────────────

#[test]
fn maps_json_response_with_token_usage_and_languages() {
    let body = json!({
        "text": "Bonjour.",
        "languages": [{"code": "fr"}],
        "usage": {"type": "tokens", "input_tokens": 14, "input_token_details": {"text_tokens": 4, "audio_tokens": 10}, "output_tokens": 3, "total_tokens": 17}
    })
    .to_string();
    let mapped = map_transcription_response(&body, "json").unwrap();
    assert_eq!(mapped.text, "Bonjour.");
    assert_eq!(mapped.language.as_deref(), Some("fr"));
    let usage = mapped.usage.unwrap();
    let input = usage.input.unwrap();
    assert_eq!(input["input_tokens"], 14);
    assert_eq!(input["text_tokens"], 4);
    assert_eq!(input["audio_tokens"], 10);
    assert_eq!(usage.output.unwrap()["output_tokens"], 3);
    assert_eq!(usage.total.unwrap()["total_tokens"], 17);
}

#[test]
fn maps_duration_usage_to_audio_milliseconds() {
    let body = json!({"text": "hi", "usage": {"type": "duration", "seconds": 2.5}}).to_string();
    let mapped = map_transcription_response(&body, "json").unwrap();
    assert_eq!(mapped.duration_seconds, Some(2.5));
    let usage = mapped.usage.unwrap();
    assert_eq!(usage.input.unwrap()["audio_milliseconds"], 2500);
    assert!(usage.output.is_none());
}

#[test]
fn maps_verbose_json_words_and_segments() {
    let body = json!({
        "task": "transcribe", "language": "english", "duration": 1.2, "text": "Hello there.",
        "words": [{"word": "Hello", "start": 0.0, "end": 0.4}, {"word": "there", "start": 0.5, "end": 0.9}],
        "segments": [{"id": 0, "seek": 0, "start": 0.0, "end": 1.2, "text": " Hello there.", "tokens": [1], "temperature": 0.0, "avg_logprob": -0.1, "compression_ratio": 1.0, "no_speech_prob": 0.01}],
        "usage": {"type": "duration", "seconds": 1.2}
    })
    .to_string();
    let mapped = map_transcription_response(&body, "verbose_json").unwrap();
    assert_eq!(mapped.language.as_deref(), Some("english"));
    assert_eq!(mapped.duration_seconds, Some(1.2));
    assert_eq!(mapped.words.len(), 2);
    assert_eq!(mapped.words[1].text, "there");
    assert_eq!(mapped.words[1].start, Some(0.5));
    assert_eq!(mapped.segments[0].text, "Hello there.");
    assert_eq!(mapped.segments[0].end, Some(1.2));
    assert_eq!(
        mapped.usage.unwrap().input.unwrap()["audio_milliseconds"],
        1200
    );
}

#[test]
fn maps_diarized_segments_with_speakers() {
    let body = json!({
        "task": "transcribe", "duration": 4.0, "text": "Hi. Hello.",
        "segments": [
            {"type": "transcript.text.segment", "id": "seg_0", "start": 0.0, "end": 1.0, "text": "Hi.", "speaker": "A"},
            {"type": "transcript.text.segment", "id": "seg_1", "start": 1.5, "end": 3.0, "text": "Hello.", "speaker": "agent"}
        ],
        "usage": {"type": "tokens", "input_tokens": 20, "output_tokens": 4, "total_tokens": 24}
    })
    .to_string();
    let mapped = map_transcription_response(&body, "diarized_json").unwrap();
    assert_eq!(mapped.segments.len(), 2);
    assert_eq!(mapped.segments[0].speaker.as_deref(), Some("A"));
    assert_eq!(mapped.segments[1].speaker.as_deref(), Some("agent"));
    assert_eq!(mapped.segments[1].start, Some(1.5));
    assert_eq!(mapped.duration_seconds, Some(4.0));
}

#[test]
fn text_formats_are_returned_verbatim_and_bad_json_errors() {
    let mapped = map_transcription_response("plain words\n", "text").unwrap();
    assert_eq!(mapped.text, "plain words");
    assert!(map_transcription_response("{oops", "json").is_err());
    assert!(map_transcription_response("{}", "json").is_err());
}

// ── Images ────────────────────────────────────────────────────────────

#[test]
fn generation_body_maps_size_quality_and_format() {
    let mut request = image("gpt-image-2.5-sunburst");
    request.n = Some(2);
    request.aspect_ratio = Some("3:2".into());
    request.quality = Some("XHIGH".into());
    request.output_format = Some("image/webp".into());
    request.provider_options = Some(json!({"background": "transparent", "output_compression": 80}));
    let (endpoint, body) = image_request(&request).unwrap();
    assert_eq!(endpoint, OpenAIImageEndpoint::Generations);
    assert_eq!(endpoint.path(), "images/generations");
    assert_eq!(body["model"], "gpt-image-2.5-sunburst");
    assert_eq!(body["prompt"], "a lighthouse at dusk");
    assert_eq!(body["n"], 2);
    assert_eq!(body["size"], "1536x1024");
    assert_eq!(body["quality"], "xhigh");
    assert_eq!(body["output_format"], "webp");
    assert_eq!(body["background"], "transparent");
    assert_eq!(body["output_compression"], 80);
    assert!(body.get("images").is_none());

    // An explicit size wins over the aspect ratio.
    request.size = Some("1536x864".into());
    assert_eq!(image_request(&request).unwrap().1["size"], "1536x864");
}

#[test]
fn edit_body_uses_json_image_refs_and_mask() {
    let mut request = image("gpt-image-2");
    request.images = vec![
        GaiseMediaSource::bytes(vec![1, 2, 3], "image/png"),
        GaiseMediaSource::url("https://example.com/b.jpg"),
        GaiseMediaSource::url("file-abc123"),
    ];
    request.mask = Some(GaiseMediaSource::bytes(vec![4], "image/png"));
    let (endpoint, body) = image_request(&request).unwrap();
    assert_eq!(endpoint, OpenAIImageEndpoint::Edits);
    assert_eq!(endpoint.path(), "images/edits");
    assert_eq!(body["images"][0]["image_url"], "data:image/png;base64,AQID");
    assert_eq!(body["images"][1]["image_url"], "https://example.com/b.jpg");
    assert_eq!(body["images"][2]["file_id"], "file-abc123");
    assert_eq!(body["mask"]["image_url"], "data:image/png;base64,BA==");

    request.images = vec![GaiseMediaSource::url("https://x/y.png"); 17];
    assert!(image_request(&request).unwrap_err().contains("16"));
}

#[test]
fn image_requests_reject_unsupported_options() {
    let mut negative = image("gpt-image-2");
    negative.negative_prompt = Some("blur".into());
    assert!(
        image_request(&negative)
            .unwrap_err()
            .contains("negative_prompt")
    );
    let mut seed = image("gpt-image-2");
    seed.seed = Some(7);
    assert!(image_request(&seed).unwrap_err().contains("seed"));
    let mut many = image("gpt-image-2");
    many.n = Some(11);
    assert!(image_request(&many).is_err());
    let mut ratio = image("gpt-image-2");
    ratio.aspect_ratio = Some("16:9".into());
    assert!(image_request(&ratio).unwrap_err().contains("16:9"));
    let mut resolution = image("gpt-image-2");
    resolution.resolution = Some("4K".into());
    assert!(image_request(&resolution).unwrap_err().contains("4K"));
    let mut format = image("gpt-image-2");
    format.output_format = Some("image/gif".into());
    assert!(image_request(&format).unwrap_err().contains("image/gif"));
    let mut mask = image("gpt-image-2");
    mask.mask = Some(GaiseMediaSource::bytes(vec![1], "image/png"));
    assert!(image_request(&mask).is_err());
    assert!(
        image_request(&image("dall-e-3"))
            .unwrap_err()
            .contains("retired")
    );
}

#[test]
fn standard_sizes_follow_common_aspect_ratios() {
    assert_eq!(
        openai_image_size(Some("1:1"), Some("1k"))
            .unwrap()
            .as_deref(),
        Some("1024x1024")
    );
    assert_eq!(
        openai_image_size(Some("2:3"), None).unwrap().as_deref(),
        Some("1024x1536")
    );
    assert_eq!(openai_image_size(None, None).unwrap(), None);
}

#[test]
fn image_response_decodes_b64_and_maps_usage() {
    let value = json!({
        "created": 1, "output_format": "jpeg", "size": "1024x1024", "quality": "high",
        "data": [{"b64_json": "AQID"}, {"url": "https://cdn/x.png", "revised_prompt": "better"}],
        "usage": {"input_tokens": 50, "input_tokens_details": {"text_tokens": 40, "image_tokens": 10}, "output_tokens": 4160, "output_tokens_details": {"image_tokens": 4096, "text_tokens": 64}, "total_tokens": 4210}
    });
    let mapped = map_image_response(&value, Some("png")).unwrap();
    assert_eq!(mapped.images[0].data.as_deref(), Some(&[1u8, 2, 3][..]));
    assert_eq!(mapped.images[0].mime_type.as_deref(), Some("image/jpeg"));
    assert_eq!(mapped.images[1].url.as_deref(), Some("https://cdn/x.png"));
    assert_eq!(mapped.images[1].revised_prompt.as_deref(), Some("better"));
    let usage = mapped.usage.unwrap();
    let input = usage.input.unwrap();
    assert_eq!(input["input_tokens"], 50);
    assert_eq!(input["text_tokens"], 40);
    assert_eq!(input["image_tokens"], 10);
    let output = usage.output.unwrap();
    assert_eq!(output["output_tokens"], 4160);
    assert_eq!(output["image_tokens"], 4096);
    assert_eq!(output["text_tokens"], 64);
    assert_eq!(usage.total.unwrap()["total_tokens"], 4210);

    let fallback =
        map_image_response(&json!({"data": [{"b64_json": "AQID"}]}), Some("webp")).unwrap();
    assert_eq!(fallback.images[0].mime_type.as_deref(), Some("image/webp"));
    assert!(fallback.usage.is_none());
    assert!(map_image_response(&json!({"data": [{}]}), None).is_err());
    assert!(map_image_response(&json!({"data": [{"b64_json": "!!"}]}), None).is_err());
    assert!(map_image_response(&json!({"created": 1}), None).is_err());
}

// ── HTTP ──────────────────────────────────────────────────────────────

struct Part {
    name: String,
    filename: Option<String>,
    data: Vec<u8>,
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn parse_multipart(body: &[u8], boundary: &str) -> Vec<Part> {
    let delimiter = format!("--{boundary}");
    let mut parts = Vec::new();
    let mut pos = find(body, delimiter.as_bytes(), 0).expect("opening boundary") + delimiter.len();
    loop {
        if body[pos..].starts_with(b"--") {
            break;
        }
        pos += 2; // CRLF after the boundary
        let header_end = find(body, b"\r\n\r\n", pos).expect("part headers");
        let headers = String::from_utf8_lossy(&body[pos..header_end]).to_string();
        let data_start = header_end + 4;
        let next =
            find(body, format!("\r\n{delimiter}").as_bytes(), data_start).expect("next boundary");
        let attr = |key: &str| {
            headers
                .split(&format!("{key}=\""))
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .map(str::to_string)
        };
        parts.push(Part {
            name: attr("name").unwrap_or_default(),
            filename: attr("filename"),
            data: body[data_start..next].to_vec(),
        });
        pos = next + 2 + delimiter.len();
    }
    parts
}

fn field(parts: &[Part], name: &str) -> Option<String> {
    parts
        .iter()
        .find(|p| p.name == name)
        .map(|p| String::from_utf8_lossy(&p.data).to_string())
}

fn fields(parts: &[Part], name: &str) -> Vec<String> {
    parts
        .iter()
        .filter(|p| p.name == name)
        .map(|p| String::from_utf8_lossy(&p.data).to_string())
        .collect()
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
async fn transcribe_posts_multipart_and_maps_each_model_family() {
    let app = Router::new().route(
        "/v1/audio/transcriptions",
        post(|headers: HeaderMap, body: Bytes| async move {
            assert_eq!(headers["authorization"], "Bearer test-key");
            let content_type = headers["content-type"].to_str().unwrap().to_string();
            let boundary = content_type
                .strip_prefix("multipart/form-data; boundary=")
                .expect("multipart content type")
                .to_string();
            let parts = parse_multipart(&body, &boundary);
            let file = parts.iter().find(|p| p.name == "file").expect("file part");
            assert_eq!(file.filename.as_deref(), Some("audio.wav"));
            assert_eq!(file.data, wav());
            let model = field(&parts, "model").unwrap();
            let format = field(&parts, "response_format").unwrap();
            match model.as_str() {
                "gpt-transcribe" => {
                    assert_eq!(format, "json");
                    assert_eq!(fields(&parts, "languages[]"), vec!["en", "fr"]);
                    assert_eq!(fields(&parts, "keywords[]"), vec!["AC-42"]);
                    Json(json!({"text": "Bonjour hello", "languages": [{"code": "fr"}, {"code": "en"}], "usage": {"type": "duration", "seconds": 3.0}}))
                }
                "whisper-1" => {
                    assert_eq!(format, "verbose_json");
                    assert_eq!(fields(&parts, "timestamp_granularities[]"), vec!["word", "segment"]);
                    Json(json!({"language": "english", "duration": 0.5, "text": "Hi", "words": [{"word": "Hi", "start": 0.0, "end": 0.3}], "segments": [{"id": 0, "seek": 0, "start": 0.0, "end": 0.5, "text": " Hi"}]}))
                }
                "gpt-4o-transcribe-diarize" => {
                    assert_eq!(format, "diarized_json");
                    assert_eq!(field(&parts, "chunking_strategy").as_deref(), Some("auto"));
                    Json(json!({"task": "transcribe", "duration": 2.0, "text": "A. B.", "segments": [{"type": "transcript.text.segment", "id": "s0", "start": 0.0, "end": 1.0, "text": "A.", "speaker": "A"}, {"type": "transcript.text.segment", "id": "s1", "start": 1.0, "end": 2.0, "text": "B.", "speaker": "B"}]}))
                }
                other => panic!("unexpected model {other}"),
            }
        }),
    );
    let (client, task) = server(app).await;

    let mut request = transcription("gpt-transcribe");
    request.languages = vec!["en".into(), "fr".into()];
    request.keyterms = vec!["AC-42".into()];
    let response = client.transcribe(&request).await.unwrap();
    assert_eq!(response.text, "Bonjour hello");
    assert_eq!(response.language.as_deref(), Some("fr"));
    assert_eq!(
        response.usage.unwrap().input.unwrap()["audio_milliseconds"],
        3000
    );

    let mut request = transcription("whisper-1");
    request.timestamps = Some(GaiseTimestampGranularity::Word);
    let response = client.transcribe(&request).await.unwrap();
    assert_eq!(response.words[0].text, "Hi");
    assert_eq!(response.segments[0].text, "Hi");

    let mut request = transcription("gpt-4o-transcribe-diarize");
    request.diarize = true;
    let response = client.transcribe(&request).await.unwrap();
    assert_eq!(response.segments[1].speaker.as_deref(), Some("B"));
    task.abort();
}

#[tokio::test]
async fn transcribe_surfaces_api_errors_and_rejects_before_sending() {
    let app = Router::new().route(
        "/v1/audio/transcriptions",
        post(|| async {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {"message": "Invalid file format.", "type": "invalid_request_error"}})),
            )
        }),
    );
    let (client, task) = server(app).await;
    let err = client
        .transcribe(&transcription("gpt-transcribe"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("400"), "{err}");
    assert!(err.contains("Invalid file format."), "{err}");

    let mut url = transcription("gpt-transcribe");
    url.audio = Some(GaiseMediaSource::url("https://example.com/a.wav"));
    assert!(client.transcribe(&url).await.is_err());
    task.abort();
}

#[tokio::test]
async fn generate_images_hits_generations_or_json_edits() {
    let app = Router::new()
        .route(
            "/v1/images/generations",
            post(|Json(body): Json<Value>| async move {
                assert_eq!(body["model"], "gpt-image-2.5-flare");
                assert_eq!(body["size"], "1024x1024");
                assert_eq!(body["output_format"], "png");
                Json(json!({
                    "created": 1, "output_format": "png",
                    "data": [{"b64_json": "iVBORw=="}],
                    "usage": {"input_tokens": 9, "input_tokens_details": {"text_tokens": 9, "image_tokens": 0}, "output_tokens": 272, "output_tokens_details": {"image_tokens": 272, "text_tokens": 0}, "total_tokens": 281}
                }))
            }),
        )
        .route(
            "/v1/images/edits",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                assert!(
                    headers["content-type"]
                        .to_str()
                        .unwrap()
                        .starts_with("application/json")
                );
                assert_eq!(body["images"][0]["image_url"], "data:image/png;base64,AQID");
                assert_eq!(body["n"], 2);
                Json(json!({"created": 2, "data": [{"b64_json": "AQID"}, {"b64_json": "BAUG"}]}))
            }),
        );
    let (client, task) = server(app).await;

    let mut request = image("gpt-image-2.5-flare");
    request.aspect_ratio = Some("1:1".into());
    request.output_format = Some("image/png".into());
    let response = client.generate_images(&request).await.unwrap();
    assert_eq!(response.images.len(), 1);
    assert_eq!(
        response.images[0].data.as_deref(),
        Some(&[0x89u8, b'P', b'N', b'G'][..])
    );
    assert_eq!(response.usage.unwrap().output.unwrap()["image_tokens"], 272);

    let mut edit = image("gpt-image-2");
    edit.images = vec![GaiseMediaSource::bytes(vec![1, 2, 3], "image/png")];
    edit.n = Some(2);
    edit.output_format = Some("image/jpeg".into());
    let response = client.generate_images(&edit).await.unwrap();
    assert_eq!(response.images.len(), 2);
    assert_eq!(response.images[1].data.as_deref(), Some(&[4u8, 5, 6][..]));
    assert_eq!(response.images[1].mime_type.as_deref(), Some("image/jpeg"));
    task.abort();
}

#[tokio::test]
async fn generate_images_surfaces_api_errors() {
    let app = Router::new().route(
        "/v1/images/generations",
        post(|| async {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {"message": "Your request was rejected by the safety system.", "code": "moderation_blocked"}})),
            )
        }),
    );
    let (client, task) = server(app).await;
    let err = client
        .generate_images(&image("gpt-image-2"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("moderation_blocked"), "{err}");
    task.abort();
}
