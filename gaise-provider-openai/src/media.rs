//! Media endpoints: speech-to-text (`POST /v1/audio/transcriptions`) and
//! image generation and editing (`POST /v1/images/generations`,
//! `POST /v1/images/edits` with the JSON body).
//!
//! Sources (audited 2026-10-17): the OpenAI OpenAPI spec
//! (`CreateTranscriptionRequest`, `CreateTranscriptionResponseJson` /
//! `VerboseJson` / `DiarizedJson`, `CreateImageRequest`,
//! `EditImageBodyJsonParam`, `ImagesResponse`, `ImageGenUsage`), the
//! speech-to-text and image-generation guides, and the deprecations page.
//! OpenAI has no video API since the Videos API (Sora) shut down on
//! 2026-09-24, so video generation stays unimplemented.
//!
//! Everything here is pure; the client in [`crate::openai_client`] only
//! sends what these builders produce.

use base64::Engine;
use gaise_core::contracts::{
    GaiseGeneratedMedia, GaiseImageRequest, GaiseImageResponse, GaiseMediaSource,
    GaiseTimestampGranularity, GaiseTranscriptSegment, GaiseTranscriptWord,
    GaiseTranscriptionRequest, GaiseTranscriptionResponse, GaiseUsage,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Largest audio file `/v1/audio/transcriptions` accepts.
pub const MAX_TRANSCRIPTION_BYTES: usize = 25 * 1024 * 1024;
/// Most input images an edit accepts.
pub const MAX_EDIT_IMAGES: usize = 16;

// ── Transcription ─────────────────────────────────────────────────────

/// Transcription model families with different parameter rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAITranscriptionFamily {
    /// `gpt-transcribe`: `languages[]`, `keywords[]`, `prompt`; JSON only.
    GptTranscribe,
    /// `gpt-4o-transcribe-diarize`: `diarized_json` speaker segments, no prompt.
    Diarize,
    /// `whisper-1`: `verbose_json` with word / segment timestamps.
    Whisper,
    /// `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`, and unknown models:
    /// one `language`, `prompt`, JSON only.
    Standard,
}

pub fn openai_transcription_family(model: &str) -> OpenAITranscriptionFamily {
    let m = model.trim().to_ascii_lowercase();
    if m.starts_with("gpt-transcribe") {
        OpenAITranscriptionFamily::GptTranscribe
    } else if m.contains("transcribe-diarize") {
        OpenAITranscriptionFamily::Diarize
    } else if m.starts_with("whisper") {
        OpenAITranscriptionFamily::Whisper
    } else {
        OpenAITranscriptionFamily::Standard
    }
}

/// A `multipart/form-data` transcription request: text fields in order plus
/// the audio file.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAITranscriptionForm {
    pub fields: Vec<(String, String)>,
    pub file_name: String,
    pub file_mime: String,
    pub file: Vec<u8>,
}

impl OpenAITranscriptionForm {
    /// Every value sent under `name`.
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// The single value sent under `name`, if any.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values(name).into_iter().next()
    }

    /// The `response_format` the request asks for (`json` when unset).
    pub fn response_format(&self) -> &str {
        self.value("response_format").unwrap_or("json")
    }

    /// Encode as a `multipart/form-data` body with `boundary`.
    pub fn encode(&self, boundary: &str) -> Vec<u8> {
        let mut body = Vec::with_capacity(self.file.len() + 512);
        for (name, value) in &self.fields {
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{value}\r\n",
                    escape_quoted(name)
                )
                .as_bytes(),
            );
        }
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                escape_quoted(&self.file_name),
                self.file_mime
            )
            .as_bytes(),
        );
        body.extend_from_slice(&self.file);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }
}

fn escape_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ")
}

/// File extension and MIME type OpenAI accepts for this audio, from the
/// declared MIME type, the audio format codec, or the leading bytes.
pub fn openai_audio_file_type(
    data: &[u8],
    mime_type: Option<&str>,
    codec: Option<&str>,
) -> Option<(&'static str, &'static str)> {
    let by_name = |name: &str| -> Option<(&'static str, &'static str)> {
        let name = name.trim().to_ascii_lowercase();
        let name = name
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        Some(match name.as_str() {
            "audio/wav" | "audio/wave" | "audio/x-wav" | "audio/vnd.wave" | "wav" | "wave" => {
                ("wav", "audio/wav")
            }
            "audio/mpeg" | "audio/mp3" | "audio/mpga" | "mp3" | "mpga" | "mpeg" => {
                ("mp3", "audio/mpeg")
            }
            "audio/flac" | "audio/x-flac" | "flac" => ("flac", "audio/flac"),
            "audio/ogg" | "audio/opus" | "application/ogg" | "ogg" | "oga" => ("ogg", "audio/ogg"),
            "audio/webm" | "video/webm" | "webm" => ("webm", "audio/webm"),
            "audio/mp4" | "audio/m4a" | "audio/x-m4a" | "audio/aac" | "m4a" => ("m4a", "audio/mp4"),
            "video/mp4" | "mp4" => ("mp4", "video/mp4"),
            "video/mpeg" => ("mpeg", "video/mpeg"),
            _ => return None,
        })
    };
    if let Some(found) = mime_type.and_then(by_name) {
        return Some(found);
    }
    if let Some(found) = codec.and_then(by_name) {
        return Some(found);
    }
    match data {
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'A',
            b'V',
            b'E',
            ..,
        ] => Some(("wav", "audio/wav")),
        [b'f', b'L', b'a', b'C', ..] => Some(("flac", "audio/flac")),
        [b'O', b'g', b'g', b'S', ..] => Some(("ogg", "audio/ogg")),
        [0x1A, 0x45, 0xDF, 0xA3, ..] => Some(("webm", "audio/webm")),
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => Some(("m4a", "audio/mp4")),
        [b'I', b'D', b'3', ..] => Some(("mp3", "audio/mpeg")),
        [0xFF, second, ..] if second & 0xE0 == 0xE0 => Some(("mp3", "audio/mpeg")),
        _ => None,
    }
}

/// Decode a `data:<mime>;base64,<payload>` URL.
pub fn decode_data_url(url: &str) -> Option<(Vec<u8>, Option<String>)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    let data = base64::prelude::BASE64_STANDARD.decode(payload).ok()?;
    let mime = (!mime.is_empty()).then(|| mime.to_string());
    Some((data, mime))
}

/// Language code for the single-`language` models: the ISO-639-1 primary
/// subtag (`en-US` → `en`).
fn primary_language(code: &str) -> String {
    code.trim()
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Language code for `gpt-transcribe` `languages[]`: ISO-639-1, selected
/// ISO-639-3, and the regional `zh` locales (`zh-cn`, `zh-tw`, `zh-hk`).
fn gpt_transcribe_language(code: &str) -> String {
    let lower = code.trim().to_ascii_lowercase().replace('_', "-");
    if lower.starts_with("zh-") {
        lower
    } else {
        primary_language(&lower)
    }
}

/// Build the multipart transcription request, honouring each model
/// family's rules and rejecting what the model cannot do.
pub fn transcription_form(
    request: &GaiseTranscriptionRequest,
) -> Result<OpenAITranscriptionForm, String> {
    request.validate()?;
    let model = request.model.trim();
    let family = openai_transcription_family(model);

    let (data, mime) = match request.audio.as_ref() {
        Some(GaiseMediaSource::Bytes { data, mime_type }) => (data.clone(), mime_type.clone()),
        Some(GaiseMediaSource::Url { url, mime_type }) => match decode_data_url(url) {
            Some((data, data_mime)) => (data, mime_type.clone().or(data_mime)),
            None => {
                return Err(format!(
                    "OpenAI transcription needs inline audio bytes (or a data: URL); it cannot fetch '{url}'. Download the file and pass GaiseMediaSource::Bytes"
                ));
            }
        },
        None => return Err("transcription request needs audio".into()),
    };
    if data.is_empty() {
        return Err("transcription audio is empty".into());
    }
    if data.len() > MAX_TRANSCRIPTION_BYTES {
        return Err(format!(
            "OpenAI transcription accepts files up to 25 MB; this audio is {} bytes. Compress it or split it into chunks",
            data.len()
        ));
    }
    if let Some(format) = &request.format {
        let codec = format.codec.trim().to_ascii_lowercase();
        if matches!(codec.as_str(), "pcm" | "mulaw" | "alaw" | "opus") {
            return Err(format!(
                "OpenAI transcription needs a container file (flac, mp3, mp4, mpeg, mpga, m4a, ogg, wav, webm); headerless '{codec}' audio is not accepted. Wrap it in WAV first"
            ));
        }
    }
    let codec = request.format.as_ref().map(|f| f.codec.as_str());
    let (ext, file_mime) = openai_audio_file_type(&data, mime.as_deref(), codec).ok_or_else(|| {
        format!(
            "could not determine the audio format{}; OpenAI transcription accepts flac, mp3, mp4, mpeg, mpga, m4a, ogg, wav, and webm. Set the MIME type on the audio source",
            mime.as_deref()
                .map(|m| format!(" ('{m}')"))
                .unwrap_or_default()
        )
    })?;

    if request.multichannel {
        return Err(format!(
            "OpenAI transcription has no per-channel mode; model '{model}' cannot honour multichannel"
        ));
    }
    if request.mode.is_some() {
        return Err(format!(
            "OpenAI transcription has no verbatim/smart mode control; model '{model}' cannot honour mode"
        ));
    }
    if request.max_speakers.is_some() {
        return Err(format!(
            "OpenAI transcription has no speaker-count hint; model '{model}' cannot honour max_speakers"
        ));
    }

    let mut fields: Vec<(String, String)> = vec![("model".into(), model.to_string())];

    // Languages.
    let languages: Vec<&String> = request
        .languages
        .iter()
        .filter(|l| !l.trim().is_empty())
        .collect();
    if family == OpenAITranscriptionFamily::GptTranscribe {
        for language in &languages {
            fields.push(("languages[]".into(), gpt_transcribe_language(language)));
        }
    } else {
        match languages.as_slice() {
            [] => {}
            [one] => fields.push(("language".into(), primary_language(one))),
            _ => {
                return Err(format!(
                    "OpenAI model '{model}' accepts one language hint; only gpt-transcribe accepts several"
                ));
            }
        }
    }

    // Prompt.
    if let Some(prompt) = request.prompt.as_deref().filter(|p| !p.trim().is_empty()) {
        if family == OpenAITranscriptionFamily::Diarize {
            return Err(format!(
                "OpenAI model '{model}' does not accept a prompt; use gpt-transcribe for prompted transcription"
            ));
        }
        fields.push(("prompt".into(), prompt.to_string()));
    }

    // Keywords.
    if !request.keyterms.is_empty() {
        if family != OpenAITranscriptionFamily::GptTranscribe {
            return Err(format!(
                "OpenAI model '{model}' does not accept keywords; only gpt-transcribe does"
            ));
        }
        for keyword in &request.keyterms {
            if keyword.contains(['<', '>', '\r', '\n']) {
                return Err(format!(
                    "OpenAI rejects keywords containing '<', '>', a carriage return, or a line feed: {keyword:?}"
                ));
            }
            fields.push(("keywords[]".into(), keyword.clone()));
        }
    }

    // Diarization and timestamps decide the response format.
    let timestamps = request
        .timestamps
        .filter(|t| *t != GaiseTimestampGranularity::None);
    let response_format = match family {
        OpenAITranscriptionFamily::Diarize => {
            match timestamps {
                None | Some(GaiseTimestampGranularity::Segment) => {}
                Some(other) => {
                    return Err(format!(
                        "OpenAI model '{model}' returns speaker segments only; {other:?} timestamps need whisper-1"
                    ));
                }
            }
            if request.diarize || timestamps.is_some() {
                fields.push(("chunking_strategy".into(), "auto".into()));
                "diarized_json"
            } else {
                "json"
            }
        }
        OpenAITranscriptionFamily::Whisper => {
            if request.diarize {
                return Err(format!(
                    "OpenAI model '{model}' cannot label speakers; use gpt-4o-transcribe-diarize"
                ));
            }
            match timestamps {
                None => {}
                Some(GaiseTimestampGranularity::Word) => {
                    fields.push(("timestamp_granularities[]".into(), "word".into()));
                    fields.push(("timestamp_granularities[]".into(), "segment".into()));
                }
                Some(GaiseTimestampGranularity::Segment) => {
                    fields.push(("timestamp_granularities[]".into(), "segment".into()));
                }
                Some(other) => {
                    return Err(format!(
                        "OpenAI model '{model}' supports word and segment timestamps, not {other:?}"
                    ));
                }
            }
            "verbose_json"
        }
        OpenAITranscriptionFamily::GptTranscribe | OpenAITranscriptionFamily::Standard => {
            if request.diarize {
                return Err(format!(
                    "OpenAI model '{model}' cannot label speakers; use gpt-4o-transcribe-diarize"
                ));
            }
            if let Some(other) = timestamps {
                return Err(format!(
                    "OpenAI model '{model}' returns no timestamps; {other:?} timestamps need whisper-1"
                ));
            }
            "json"
        }
    };
    fields.push(("response_format".into(), response_format.into()));

    if let Some(temperature) = request.temperature {
        fields.push(("temperature".into(), temperature.to_string()));
    }

    if let Some(options) = &request.provider_options {
        merge_form_options(&mut fields, options)?;
    }

    Ok(OpenAITranscriptionForm {
        fields,
        file_name: format!("audio.{ext}"),
        file_mime: file_mime.to_string(),
        file: data,
    })
}

/// Merge `provider_options` into multipart fields: each top-level key
/// replaces the fields of that name; arrays become repeated `key[]` fields,
/// strings are sent as-is, other values as JSON, and `null` removes the key.
pub fn merge_form_options(
    fields: &mut Vec<(String, String)>,
    options: &Value,
) -> Result<(), String> {
    let Some(options) = options.as_object() else {
        return Err("OpenAI provider_options must be a JSON object".into());
    };
    for (key, value) in options {
        let base = key.strip_suffix("[]").unwrap_or(key);
        let array_key = format!("{base}[]");
        fields.retain(|(name, _)| name != base && name != &array_key);
        let as_text = |v: &Value| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        match value {
            Value::Null => {}
            Value::Array(items) => {
                for item in items {
                    fields.push((array_key.clone(), as_text(item)));
                }
            }
            other => fields.push((base.to_string(), as_text(other))),
        }
    }
    Ok(())
}

/// Map a transcription token or duration usage object.
pub fn map_transcription_usage(usage: &Value) -> Option<GaiseUsage> {
    let as_usize = |v: &Value| v.as_u64().map(|n| n as usize);
    match usage["type"].as_str() {
        Some("duration") => {
            let seconds = usage["seconds"].as_f64()?;
            Some(GaiseUsage {
                input: Some(HashMap::from([(
                    "audio_milliseconds".to_string(),
                    (seconds * 1000.0).round().max(0.0) as usize,
                )])),
                output: None,
                total: None,
            })
        }
        Some("tokens") | None if usage.get("input_tokens").is_some() => {
            let mut input = HashMap::new();
            if let Some(n) = as_usize(&usage["input_tokens"]) {
                input.insert("input_tokens".to_string(), n);
            }
            for key in ["text_tokens", "audio_tokens"] {
                if let Some(n) = as_usize(&usage["input_token_details"][key]) {
                    input.insert(key.to_string(), n);
                }
            }
            let output = as_usize(&usage["output_tokens"])
                .map(|n| HashMap::from([("output_tokens".to_string(), n)]));
            let total = as_usize(&usage["total_tokens"])
                .map(|n| HashMap::from([("total_tokens".to_string(), n)]));
            Some(GaiseUsage {
                input: Some(input),
                output,
                total,
            })
        }
        _ => None,
    }
}

/// Map a transcription response body. `json`, `verbose_json`, and
/// `diarized_json` are read by shape; `text`, `srt`, and `vtt` bodies are
/// returned verbatim as the text.
pub fn map_transcription_response(
    body: &str,
    response_format: &str,
) -> Result<GaiseTranscriptionResponse, String> {
    if matches!(response_format, "text" | "srt" | "vtt") {
        return Ok(GaiseTranscriptionResponse {
            text: body.trim_end_matches(['\r', '\n']).to_string(),
            ..Default::default()
        });
    }
    let value: Value = serde_json::from_str(body).map_err(|e| {
        let snippet: String = body.chars().take(400).collect();
        format!("failed to parse OpenAI transcription response: {e}; body starts: {snippet}")
    })?;
    let Some(text) = value["text"].as_str() else {
        let snippet: String = body.chars().take(400).collect();
        return Err(format!(
            "OpenAI transcription response has no text; body starts: {snippet}"
        ));
    };
    let language = value["language"]
        .as_str()
        .map(str::to_string)
        .or_else(|| value["languages"][0]["code"].as_str().map(str::to_string));
    let usage = map_transcription_usage(&value["usage"]);
    let duration_seconds = value["duration"].as_f64().or_else(|| {
        (value["usage"]["type"] == "duration")
            .then(|| value["usage"]["seconds"].as_f64())
            .flatten()
    });
    let words = value["words"]
        .as_array()
        .map(|words| {
            words
                .iter()
                .map(|w| GaiseTranscriptWord {
                    text: w["word"].as_str().unwrap_or_default().to_string(),
                    start: w["start"].as_f64(),
                    end: w["end"].as_f64(),
                    ..Default::default()
                })
                .collect()
        })
        .unwrap_or_default();
    let segments = value["segments"]
        .as_array()
        .map(|segments| {
            segments
                .iter()
                .map(|s| GaiseTranscriptSegment {
                    text: s["text"].as_str().unwrap_or_default().trim().to_string(),
                    start: s["start"].as_f64(),
                    end: s["end"].as_f64(),
                    speaker: s["speaker"].as_str().map(str::to_string),
                    channel: None,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(GaiseTranscriptionResponse {
        text: text.to_string(),
        language,
        duration_seconds,
        segments,
        words,
        channels: Vec::new(),
        usage,
        external_id: None,
    })
}

// ── Images ────────────────────────────────────────────────────────────

/// Which images endpoint a request goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAIImageEndpoint {
    /// `POST /v1/images/generations`.
    Generations,
    /// `POST /v1/images/edits` (JSON body with `images[]`).
    Edits,
}

impl OpenAIImageEndpoint {
    pub fn path(&self) -> &'static str {
        match self {
            Self::Generations => "images/generations",
            Self::Edits => "images/edits",
        }
    }
}

/// `png` / `jpeg` / `webp` for an output MIME type (bare names accepted).
pub fn openai_image_output_format(mime: &str) -> Option<&'static str> {
    match mime.trim().to_ascii_lowercase().as_str() {
        "image/png" | "png" => Some("png"),
        "image/jpeg" | "image/jpg" | "jpeg" | "jpg" => Some("jpeg"),
        "image/webp" | "webp" => Some("webp"),
        _ => None,
    }
}

fn image_mime(format: &str) -> &'static str {
    match format {
        "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

/// Standard size for a common aspect ratio and resolution tier.
pub fn openai_image_size(
    aspect_ratio: Option<&str>,
    resolution: Option<&str>,
) -> Result<Option<String>, String> {
    if let Some(resolution) = resolution {
        let tier = resolution.trim().to_ascii_uppercase();
        if tier != "1K" && tier != "AUTO" {
            return Err(format!(
                "OpenAI image models take an exact size (WIDTHxHEIGHT), not resolution '{resolution}'; set size instead"
            ));
        }
    }
    let Some(ratio) = aspect_ratio else {
        return Ok(None);
    };
    let size = match ratio.trim().replace(' ', "").as_str() {
        "1:1" => "1024x1024",
        "3:2" => "1536x1024",
        "2:3" => "1024x1536",
        "auto" => "auto",
        _ => {
            return Err(format!(
                "OpenAI image models map only the aspect ratios 1:1, 3:2, and 2:3; set an exact size (WIDTHxHEIGHT) for aspect ratio '{ratio}'"
            ));
        }
    };
    Ok(Some(size.to_string()))
}

/// Image reference for the JSON edit body: an uploaded file id
/// (`file-…`), or a URL / data URL.
fn image_ref(source: &GaiseMediaSource) -> Value {
    if let GaiseMediaSource::Url { url, .. } = source
        && url.starts_with("file-")
    {
        return json!({"file_id": url});
    }
    json!({"image_url": source.to_url("image/png")})
}

/// Build the images request body and pick the endpoint. Input images turn
/// the request into a JSON edit (up to 16 images, optional mask).
pub fn image_request(request: &GaiseImageRequest) -> Result<(OpenAIImageEndpoint, Value), String> {
    request.validate()?;
    let model = request.model.trim();
    if model.to_ascii_lowercase().starts_with("dall-e") {
        return Err(format!(
            "OpenAI retired '{model}' on 2026-05-12; use gpt-image-2 or gpt-image-2.5-sunburst"
        ));
    }
    if request.negative_prompt.is_some() {
        return Err("OpenAI image models do not support negative_prompt".into());
    }
    if request.seed.is_some() {
        return Err("OpenAI image models do not support seed".into());
    }
    if let Some(n) = request.n
        && !(1..=10).contains(&n)
    {
        return Err(format!("OpenAI image n must be between 1 and 10, got {n}"));
    }

    let mut body = Map::new();
    body.insert("model".into(), json!(model));
    body.insert("prompt".into(), json!(request.prompt));
    if let Some(n) = request.n {
        body.insert("n".into(), json!(n));
    }
    let size = match request.size.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(size) => Some(size.trim().to_string()),
        None => openai_image_size(
            request.aspect_ratio.as_deref(),
            request.resolution.as_deref(),
        )?,
    };
    if let Some(size) = size {
        body.insert("size".into(), json!(size));
    }
    if let Some(quality) = request.quality.as_deref().filter(|q| !q.trim().is_empty()) {
        body.insert("quality".into(), json!(quality.trim().to_ascii_lowercase()));
    }
    if let Some(mime) = request.output_format.as_deref() {
        let format = openai_image_output_format(mime).ok_or_else(|| {
            format!("OpenAI image models output png, jpeg, or webp, not '{mime}'")
        })?;
        body.insert("output_format".into(), json!(format));
    }

    let endpoint = if request.images.is_empty() {
        if request.mask.is_some() {
            return Err("an image mask needs at least one input image to edit".into());
        }
        OpenAIImageEndpoint::Generations
    } else {
        if request.images.len() > MAX_EDIT_IMAGES {
            return Err(format!(
                "OpenAI image edits accept up to {MAX_EDIT_IMAGES} input images, got {}",
                request.images.len()
            ));
        }
        body.insert(
            "images".into(),
            Value::Array(request.images.iter().map(image_ref).collect()),
        );
        if let Some(mask) = &request.mask {
            body.insert("mask".into(), image_ref(mask));
        }
        OpenAIImageEndpoint::Edits
    };

    if let Some(options) = &request.provider_options {
        let Some(options) = options.as_object() else {
            return Err("OpenAI provider_options must be a JSON object".into());
        };
        for (key, value) in options {
            if value.is_null() {
                body.remove(key);
            } else {
                body.insert(key.clone(), value.clone());
            }
        }
    }
    Ok((endpoint, Value::Object(body)))
}

/// Map `ImageGenUsage` to GAISe counters.
pub fn map_image_usage(usage: &Value) -> Option<GaiseUsage> {
    if !usage.is_object() {
        return None;
    }
    let as_usize = |v: &Value| v.as_u64().map(|n| n as usize);
    let mut input = HashMap::new();
    if let Some(n) = as_usize(&usage["input_tokens"]) {
        input.insert("input_tokens".to_string(), n);
    }
    for key in ["text_tokens", "image_tokens"] {
        if let Some(n) = as_usize(&usage["input_tokens_details"][key]) {
            input.insert(key.to_string(), n);
        }
    }
    let mut output = HashMap::new();
    if let Some(n) = as_usize(&usage["output_tokens"]) {
        output.insert("output_tokens".to_string(), n);
    }
    for key in ["text_tokens", "image_tokens"] {
        if let Some(n) = as_usize(&usage["output_tokens_details"][key]) {
            output.insert(key.to_string(), n);
        }
    }
    let total =
        as_usize(&usage["total_tokens"]).map(|n| HashMap::from([("total_tokens".to_string(), n)]));
    Some(GaiseUsage {
        input: (!input.is_empty()).then_some(input),
        output: (!output.is_empty()).then_some(output),
        total,
    })
}

/// Map an `ImagesResponse`, decoding `b64_json` images. `requested_format`
/// (`png` / `jpeg` / `webp`) sets the MIME type when the response omits
/// `output_format`.
pub fn map_image_response(
    value: &Value,
    requested_format: Option<&str>,
) -> Result<GaiseImageResponse, String> {
    let format = value["output_format"]
        .as_str()
        .or(requested_format)
        .unwrap_or("png");
    let mime = image_mime(format);
    let Some(data) = value["data"].as_array() else {
        return Err(format!(
            "OpenAI image response has no data array: {}",
            value.to_string().chars().take(400).collect::<String>()
        ));
    };
    let mut images = Vec::with_capacity(data.len());
    for (index, item) in data.iter().enumerate() {
        let bytes = match item["b64_json"].as_str() {
            Some(b64) => Some(
                base64::prelude::BASE64_STANDARD
                    .decode(b64)
                    .map_err(|e| format!("OpenAI image {index} has invalid base64: {e}"))?,
            ),
            None => None,
        };
        let url = item["url"].as_str().map(str::to_string);
        if bytes.is_none() && url.is_none() {
            return Err(format!("OpenAI image {index} has neither b64_json nor url"));
        }
        images.push(GaiseGeneratedMedia {
            data: bytes,
            url,
            mime_type: Some(mime.to_string()),
            revised_prompt: item["revised_prompt"].as_str().map(str::to_string),
            ..Default::default()
        });
    }
    Ok(GaiseImageResponse {
        images,
        text: None,
        filtered: None,
        usage: map_image_usage(&value["usage"]),
        external_id: None,
    })
}
