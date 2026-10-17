//! Media operations on the xAI REST API: speech-to-text (`POST /v1/stt`),
//! Grok Imagine images (`/v1/images/generations`, `/v1/images/edits`), and
//! Grok Imagine video (`/v1/videos/generations|edits|extensions`, polled
//! with `GET /v1/videos/{request_id}`).
//!
//! Everything here is pure (no I/O) so the wire shapes can be tested
//! without a network; [`crate::xai_client::GaiseClientXai`] does the HTTP.

use base64::Engine;
use gaise_core::contracts::{
    GaiseGeneratedMedia, GaiseImageRequest, GaiseImageResponse, GaiseJobStatus, GaiseMediaSource,
    GaiseTranscriptWord, GaiseTranscriptionChannel, GaiseTranscriptionMode,
    GaiseTranscriptionRequest, GaiseTranscriptionResponse, GaiseUsage, GaiseVideoJob,
    GaiseVideoRequest,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

// ── Shared ────────────────────────────────────────────────────────────

/// Merge the top-level keys of `provider_options` into a JSON body; the
/// options win over mapped fields.
pub fn merge_provider_options(body: &mut Value, options: Option<&Value>) {
    if let (Value::Object(target), Some(Value::Object(extra))) = (body, options) {
        for (key, value) in extra {
            target.insert(key.clone(), value.clone());
        }
    }
}

/// Form-field / query-string values for one `provider_options` entry:
/// strings as-is, arrays repeated per element, `null` skipped, anything
/// else as its JSON text.
pub fn option_values(value: &Value) -> Vec<String> {
    match value {
        Value::Null => Vec::new(),
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items.iter().flat_map(option_values).collect(),
        other => vec![other.to_string()],
    }
}

/// MIME type from the leading bytes of common image, audio, and video files.
pub fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    let starts = |prefix: &[u8]| data.starts_with(prefix);
    let at = |offset: usize, tag: &[u8]| {
        data.len() >= offset + tag.len() && &data[offset..offset + tag.len()] == tag
    };
    if starts(b"\x89PNG") {
        Some("image/png")
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if starts(b"GIF8") {
        Some("image/gif")
    } else if starts(b"RIFF") && at(8, b"WEBP") {
        Some("image/webp")
    } else if starts(b"RIFF") && at(8, b"WAVE") {
        Some("audio/wav")
    } else if starts(b"ID3") || (data.len() > 1 && data[0] == 0xFF && data[1] & 0xE0 == 0xE0) {
        Some("audio/mpeg")
    } else if starts(b"OggS") {
        Some("audio/ogg")
    } else if starts(b"fLaC") {
        Some("audio/flac")
    } else if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some("video/webm")
    } else if at(4, b"ftyp") {
        Some("video/mp4")
    } else {
        None
    }
}

fn source_url(source: &GaiseMediaSource, fallback_mime: &str) -> String {
    match source {
        GaiseMediaSource::Bytes {
            data,
            mime_type: None,
        } => source.to_url(sniff_mime(data).unwrap_or(fallback_mime)),
        _ => source.to_url(fallback_mime),
    }
}

/// Usage block shared by the Images and Videos APIs (`input_tokens`,
/// `output_tokens`, their details, `total_tokens`, `cost_in_usd_ticks`).
pub fn map_xai_media_usage(usage: &Value) -> Option<GaiseUsage> {
    if !usage.is_object() {
        return None;
    }
    let count = |v: &Value| v.as_u64().map(|n| n as usize);
    let side = |total_key: &str, details_key: &str| {
        let mut map = HashMap::new();
        if let Some(n) = count(&usage[total_key]) {
            map.insert(total_key.to_string(), n);
        }
        if let Some(details) = usage[details_key].as_object() {
            for (key, value) in details {
                if let Some(n) = count(value) {
                    map.insert(key.clone(), n);
                }
            }
        }
        (!map.is_empty()).then_some(map)
    };
    let mut total = HashMap::new();
    for key in ["total_tokens", "cost_in_usd_ticks"] {
        if let Some(n) = count(&usage[key]) {
            total.insert(key.to_string(), n);
        }
    }
    let result = GaiseUsage {
        input: side("input_tokens", "input_tokens_details"),
        output: side("output_tokens", "output_tokens_details"),
        total: (!total.is_empty()).then_some(total),
    };
    (result.input.is_some() || result.output.is_some() || result.total.is_some()).then_some(result)
}

// ── Speech to text ────────────────────────────────────────────────────

/// One `multipart/form-data` field of a `POST /v1/stt` request.
#[derive(Debug, Clone, PartialEq)]
pub enum XaiSttPart {
    Text {
        name: String,
        value: String,
    },
    /// The audio upload; always the last part (xAI ignores fields after it).
    File {
        data: Vec<u8>,
        file_name: String,
        mime_type: String,
    },
}

/// `pcm`, `mulaw`, or `alaw` for a headerless codec name, else `None`.
pub fn xai_raw_codec(codec: &str) -> Option<&'static str> {
    match codec.trim().to_ascii_lowercase().as_str() {
        "pcm" | "pcm16" | "pcm_s16le" | "s16le" | "linear16" | "audio/pcm" | "audio/l16" => {
            Some("pcm")
        }
        "mulaw" | "ulaw" | "mu-law" | "pcmu" | "audio/basic" => Some("mulaw"),
        "alaw" | "a-law" | "pcma" => Some("alaw"),
        _ => None,
    }
}

fn audio_extension(mime: &str) -> &'static str {
    match mime {
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/aac" => "aac",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => "m4a",
        "video/mp4" => "mp4",
        "video/webm" | "audio/webm" => "webm",
        "video/x-matroska" => "mkv",
        _ => "bin",
    }
}

/// Build the ordered multipart fields for `POST /v1/stt`. The audio file,
/// when inline, is always last.
pub fn build_xai_stt_parts(request: &GaiseTranscriptionRequest) -> Result<Vec<XaiSttPart>, String> {
    request.validate()?;
    if request.prompt.is_some() {
        return Err("xAI speech-to-text does not accept a prompt; use keyterms".into());
    }
    if request.temperature.is_some() {
        return Err("xAI speech-to-text does not accept temperature".into());
    }
    if request.max_speakers.is_some() {
        return Err(
            "xAI speech-to-text does not accept max_speakers (diarize detects speakers)".into(),
        );
    }
    let mut parts = Vec::new();
    let mut text = |name: &str, value: String| {
        parts.push(XaiSttPart::Text {
            name: name.to_string(),
            value,
        })
    };
    if !request.model.trim().is_empty() {
        text("model", request.model.clone());
    }
    let language = request.languages.iter().find(|l| !l.trim().is_empty());
    if let Some(language) = language {
        text("language", language.clone());
    }
    match request.mode {
        // Inverse text normalization needs a language; without one it is a 400.
        Some(GaiseTranscriptionMode::Smart) if language.is_some() => text("format", "true".into()),
        Some(GaiseTranscriptionMode::Verbatim) => text("filler_words", "true".into()),
        _ => {}
    }
    if request.diarize {
        text("diarize", "true".into());
    }
    for term in &request.keyterms {
        text("keyterm", term.clone());
    }
    if request.multichannel {
        text("multichannel", "true".into());
    }
    // Container formats are auto-detected from the file header; xAI asks
    // callers not to send audio_format for them.
    let raw_codec = request
        .format
        .as_ref()
        .and_then(|f| xai_raw_codec(&f.codec));
    if let (Some(format), Some(codec)) = (&request.format, raw_codec) {
        let rate = format.sample_rate.ok_or_else(|| {
            format!("xAI speech-to-text needs format.sample_rate for raw {codec} audio")
        })?;
        text("audio_format", codec.to_string());
        text("sample_rate", rate.to_string());
        if let Some(channels) = format.channels.filter(|c| *c > 1) {
            text("channels", channels.to_string());
        }
    }
    if let Some(Value::Object(options)) = &request.provider_options {
        for (name, value) in options {
            parts.retain(|p| !matches!(p, XaiSttPart::Text { name: n, .. } if n == name));
            for v in option_values(value) {
                parts.push(XaiSttPart::Text {
                    name: name.clone(),
                    value: v,
                });
            }
        }
    }
    match request.audio.as_ref() {
        Some(GaiseMediaSource::Url { url, .. }) => parts.push(XaiSttPart::Text {
            name: "url".into(),
            value: url.clone(),
        }),
        Some(GaiseMediaSource::Bytes { data, mime_type }) => {
            let mime = mime_type
                .clone()
                .or_else(|| sniff_mime(data).map(str::to_string))
                .unwrap_or_else(|| "application/octet-stream".into());
            let ext = raw_codec.unwrap_or_else(|| audio_extension(&mime));
            parts.push(XaiSttPart::File {
                data: data.clone(),
                file_name: format!("audio.{ext}"),
                mime_type: mime,
            });
        }
        None => unreachable!("validated above"),
    }
    Ok(parts)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct XaiSttWord {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub speaker: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct XaiSttChannel {
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub words: Vec<XaiSttWord>,
}

/// `POST /v1/stt` response body.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct XaiSttResponse {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub words: Vec<XaiSttWord>,
    #[serde(default)]
    pub channels: Vec<XaiSttChannel>,
}

fn speaker_label(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Map an xAI word (`speaker` integer → string label).
pub fn map_xai_stt_word(word: &XaiSttWord, channel: Option<u32>) -> GaiseTranscriptWord {
    GaiseTranscriptWord {
        text: word.text.clone(),
        start: word.start,
        end: word.end,
        confidence: word.confidence,
        speaker: word.speaker.as_ref().and_then(speaker_label),
        channel,
        ..Default::default()
    }
}

/// Usage for duration-billed transcription: `input.audio_milliseconds`.
pub fn audio_duration_usage(seconds: f64) -> GaiseUsage {
    GaiseUsage {
        input: Some(HashMap::from([(
            "audio_milliseconds".to_string(),
            (seconds.max(0.0) * 1000.0).round() as usize,
        )])),
        output: None,
        total: None,
    }
}

pub fn map_xai_stt_response(response: XaiSttResponse) -> GaiseTranscriptionResponse {
    GaiseTranscriptionResponse {
        text: response.text,
        language: response.language.filter(|l| !l.is_empty()),
        duration_seconds: response.duration,
        words: response
            .words
            .iter()
            .map(|w| map_xai_stt_word(w, None))
            .collect(),
        channels: response
            .channels
            .iter()
            .map(|c| GaiseTranscriptionChannel {
                index: c.index,
                text: c.text.clone(),
                language: c.language.clone().filter(|l| !l.is_empty()),
                words: c
                    .words
                    .iter()
                    .map(|w| map_xai_stt_word(w, Some(c.index)))
                    .collect(),
            })
            .collect(),
        usage: response.duration.map(audio_duration_usage),
        ..Default::default()
    }
}

// ── Images ────────────────────────────────────────────────────────────

/// `1K`/`1.5K`/`2K` (any case, `1024`/`2048` too) → xAI `1k`/`1.5k`/`2k`.
pub fn xai_image_resolution(resolution: &str) -> Result<&'static str, String> {
    match resolution.trim().to_ascii_lowercase().as_str() {
        "1k" | "1024" => Ok("1k"),
        "1.5k" | "1536" => Ok("1.5k"),
        "2k" | "2048" => Ok("2k"),
        other => Err(format!(
            "xAI image resolution must be 1K, 1.5K, or 2K; '{other}' is not supported"
        )),
    }
}

/// `low`, `medium`, `auto` pass; anything else is rejected.
pub fn xai_image_quality(quality: &str) -> Result<&'static str, String> {
    match quality.trim().to_ascii_lowercase().as_str() {
        "low" => Ok("low"),
        "medium" => Ok("medium"),
        "auto" => Ok("auto"),
        other => Err(format!(
            "xAI image quality must be low, medium, or auto; '{other}' is not supported"
        )),
    }
}

/// Build the Images API call: `(path, body)` with path `images/generations`
/// (no input images) or `images/edits` (`image` for one input, `images` for
/// several). Base64 output is always requested.
pub fn build_xai_image_request(
    request: &GaiseImageRequest,
) -> Result<(&'static str, Value), String> {
    request.validate()?;
    for (set, field) in [
        (request.mask.is_some(), "mask"),
        (request.negative_prompt.is_some(), "negative_prompt"),
        (request.seed.is_some(), "seed"),
        (
            request.size.is_some(),
            "size (use aspect_ratio and resolution)",
        ),
        (request.output_format.is_some(), "output_format"),
    ] {
        if set {
            return Err(format!("xAI image generation does not support {field}"));
        }
    }
    let mut body = json!({
        "model": request.model,
        "prompt": request.prompt,
        "response_format": "b64_json",
    });
    if let Some(n) = request.n {
        body["n"] = json!(n);
    }
    if let Some(ratio) = request
        .aspect_ratio
        .as_deref()
        .filter(|r| !r.trim().is_empty())
    {
        body["aspect_ratio"] = json!(ratio);
    }
    if let Some(resolution) = request.resolution.as_deref() {
        body["resolution"] = json!(xai_image_resolution(resolution)?);
    }
    if let Some(quality) = request.quality.as_deref() {
        body["quality"] = json!(xai_image_quality(quality)?);
    }
    let path = match request.images.as_slice() {
        [] => "images/generations",
        [one] => {
            body["image"] = json!({"url": source_url(one, "image/png")});
            "images/edits"
        }
        many => {
            body["images"] = Value::Array(
                many.iter()
                    .map(|s| json!({"url": source_url(s, "image/png")}))
                    .collect(),
            );
            "images/edits"
        }
    };
    merge_provider_options(&mut body, request.provider_options.as_ref());
    Ok((path, body))
}

/// Map an Images API response (`data[]` with `b64_json` or `url`).
pub fn map_xai_image_response(body: &Value) -> Result<GaiseImageResponse, String> {
    let mut images = Vec::new();
    let mut filtered = 0u32;
    for item in body["data"].as_array().into_iter().flatten() {
        if item["respect_moderation"] == Value::Bool(false) {
            filtered += 1;
            continue;
        }
        let data = match item["b64_json"].as_str().filter(|s| !s.is_empty()) {
            Some(b64) => Some(
                base64::prelude::BASE64_STANDARD
                    .decode(b64.as_bytes())
                    .map_err(|e| format!("xAI image is not valid base64: {e}"))?,
            ),
            None => None,
        };
        let url = item["url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if data.is_none() && url.is_none() {
            continue;
        }
        let mime_type = item["mime_type"]
            .as_str()
            .map(str::to_string)
            .or_else(|| data.as_deref().and_then(sniff_mime).map(str::to_string));
        images.push(GaiseGeneratedMedia {
            data,
            url,
            mime_type,
            revised_prompt: item["revised_prompt"].as_str().map(str::to_string),
            ..Default::default()
        });
    }
    Ok(GaiseImageResponse {
        images,
        filtered: (filtered > 0).then_some(filtered),
        usage: map_xai_media_usage(&body["usage"]),
        external_id: body["request_id"].as_str().map(str::to_string),
        ..Default::default()
    })
}

// ── Video ─────────────────────────────────────────────────────────────

/// Build the Videos API call: `(path, body)`.
///
/// - No `video`: `videos/generations` (text-, image-, and reference-to-video;
///   `image` is the first frame, `last_frame` the last).
/// - `video` without `duration_seconds`: `videos/edits` (prompt + video).
/// - `video` with `duration_seconds`: `videos/extensions` (continue the clip
///   by 2–10 s).
pub fn build_xai_video_request(
    request: &GaiseVideoRequest,
) -> Result<(&'static str, Value), String> {
    request.validate()?;
    for (set, field) in [
        (
            request.n.is_some_and(|n| n != 1),
            "n (one video per request)",
        ),
        (request.negative_prompt.is_some(), "negative_prompt"),
        (request.seed.is_some(), "seed"),
    ] {
        if set {
            return Err(format!("xAI video generation does not support {field}"));
        }
    }
    let mut body = json!({"model": request.model});
    if let Some(prompt) = request.prompt.as_deref().filter(|p| !p.trim().is_empty()) {
        body["prompt"] = json!(prompt);
    }
    if let Some(uri) = request.output_uri.as_deref() {
        if !(uri.starts_with("https://") || uri.starts_with("http://")) {
            return Err(
                "xAI video output_uri must be a signed http(s) URL (it is uploaded with HTTP PUT)"
                    .into(),
            );
        }
        body["output"] = json!({"upload_url": uri});
    }
    let path = if let Some(video) = &request.video {
        for (set, field) in [
            (request.image.is_some(), "image"),
            (request.last_frame.is_some(), "last_frame"),
            (!request.reference_images.is_empty(), "reference_images"),
            (request.aspect_ratio.is_some(), "aspect_ratio"),
            (request.resolution.is_some(), "resolution"),
            (request.generate_audio.is_some(), "generate_audio"),
        ] {
            if set {
                return Err(format!(
                    "xAI video edits and extensions do not support {field} (the output follows the input video)"
                ));
            }
        }
        if body.get("prompt").is_none() {
            return Err("xAI video edits and extensions need a prompt".into());
        }
        body["video"] = json!({"url": source_url(video, "video/mp4")});
        match request.duration_seconds {
            Some(seconds) => {
                body["duration"] = json!(seconds);
                "videos/extensions"
            }
            None => "videos/edits",
        }
    } else {
        if let Some(image) = &request.image {
            body["image"] = json!({"url": source_url(image, "image/png")});
        }
        if let Some(last) = &request.last_frame {
            body["last_frame"] = json!({"url": source_url(last, "image/png")});
        }
        if !request.reference_images.is_empty() {
            body["reference_images"] = Value::Array(
                request
                    .reference_images
                    .iter()
                    .map(|s| json!({"url": source_url(s, "image/png")}))
                    .collect(),
            );
        }
        if let Some(seconds) = request.duration_seconds {
            body["duration"] = json!(seconds);
        }
        if let Some(ratio) = request
            .aspect_ratio
            .as_deref()
            .filter(|r| !r.trim().is_empty())
        {
            body["aspect_ratio"] = json!(ratio);
        }
        if let Some(resolution) = request.resolution.as_deref() {
            let res = resolution.trim().to_ascii_lowercase();
            if !matches!(res.as_str(), "480p" | "720p" | "1080p") {
                return Err(format!(
                    "xAI video resolution must be 480p, 720p, or 1080p; '{resolution}' is not supported"
                ));
            }
            body["resolution"] = json!(res);
        }
        if let Some(audio) = request.generate_audio {
            body["generate_audio"] = json!(audio);
        }
        "videos/generations"
    };
    merge_provider_options(&mut body, request.provider_options.as_ref());
    Ok((path, body))
}

/// Map a `GET /v1/videos/{request_id}` body to a job snapshot.
pub fn map_xai_video_job(id: &str, model: &str, body: &Value) -> GaiseVideoJob {
    let status_text = body["status"]
        .as_str()
        .unwrap_or("pending")
        .to_ascii_lowercase();
    let status = match status_text.as_str() {
        "done" | "completed" | "succeeded" => GaiseJobStatus::Completed,
        "failed" | "expired" | "error" | "cancelled" => GaiseJobStatus::Failed,
        "queued" => GaiseJobStatus::Queued,
        _ => GaiseJobStatus::Running,
    };
    let mut job = GaiseVideoJob {
        id: id.to_string(),
        model: model.to_string(),
        status,
        progress: body["progress"].as_u64().map(|p| p.min(100) as u8),
        usage: map_xai_media_usage(&body["usage"]),
        ..Default::default()
    };
    match status {
        GaiseJobStatus::Failed => {
            job.error = Some(
                body["error"]["message"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("xAI video generation {status_text}")),
            );
        }
        GaiseJobStatus::Completed => {
            let video = &body["video"];
            if video["respect_moderation"] == Value::Bool(false) {
                job.filtered = Some(1);
            } else if let Some(url) = video["url"].as_str().filter(|u| !u.is_empty()) {
                job.videos.push(GaiseGeneratedMedia {
                    url: Some(url.to_string()),
                    mime_type: Some("video/mp4".into()),
                    duration_seconds: video["duration"].as_f64(),
                    ..Default::default()
                });
            }
            job.progress = Some(100);
        }
        _ => {}
    }
    job
}
