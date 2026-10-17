//! Speech-to-text (Scribe) wire types and pure mapping helpers.
//!
//! Surfaces (API reference audited 2026-10-17):
//! - `POST /v1/speech-to-text` (multipart; `scribe_v2`, `scribe_v2_medical`;
//!   `scribe_v1` is deprecated)
//! - `wss://…/v1/speech-to-text/realtime` (`scribe_v2_realtime`)
//!
//! Webhook mode (`webhook=true`, HTTP 202) is never requested: GAISe
//! transcription is synchronous.

use std::collections::HashMap;

use base64::Engine;
use gaise_core::contracts::{
    GaiseAudioFormat, GaiseCommitStrategy, GaiseLiveTranscript, GaiseMediaSource,
    GaiseTimestampGranularity, GaiseTranscriptSegment, GaiseTranscriptTokenKind,
    GaiseTranscriptWord, GaiseTranscriptionChannel, GaiseTranscriptionEvent,
    GaiseTranscriptionMode, GaiseTranscriptionRequest, GaiseTranscriptionResponse,
    GaiseTranscriptionSessionConfig, GaiseUsage,
};
use serde::Deserialize;

/// Default realtime model when the session config names none.
pub const DEFAULT_REALTIME_TRANSCRIPTION_MODEL: &str = "scribe_v2_realtime";
/// `num_speakers` ceiling documented for `/v1/speech-to-text`.
pub const MAX_SPEAKERS: u32 = 32;

// ── Batch request (multipart) ─────────────────────────────────────────────

/// One multipart form value.
#[derive(Debug, Clone, PartialEq)]
pub enum ElevenLabsFormValue {
    Text(String),
    File {
        data: Vec<u8>,
        filename: String,
        mime_type: String,
    },
}

/// Ordered multipart form for `POST /v1/speech-to-text`. Array fields
/// (`keyterms`) repeat the field name once per item.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElevenLabsTranscriptionForm {
    pub fields: Vec<(String, ElevenLabsFormValue)>,
}

impl ElevenLabsTranscriptionForm {
    fn push_text(&mut self, name: &str, value: impl Into<String>) {
        self.fields
            .push((name.to_string(), ElevenLabsFormValue::Text(value.into())));
    }

    fn remove(&mut self, name: &str) {
        self.fields.retain(|(n, _)| n != name);
    }

    /// First text value of `name`.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.texts(name).into_iter().next()
    }

    /// Every text value of `name`, in order.
    pub fn texts(&self, name: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(n, _)| n == name)
            .filter_map(|(_, v)| match v {
                ElevenLabsFormValue::Text(t) => Some(t.as_str()),
                ElevenLabsFormValue::File { .. } => None,
            })
            .collect()
    }

    /// The `file` part, if any.
    pub fn file(&self) -> Option<(&[u8], &str, &str)> {
        self.fields.iter().find_map(|(_, v)| match v {
            ElevenLabsFormValue::File {
                data,
                filename,
                mime_type,
            } => Some((data.as_slice(), filename.as_str(), mime_type.as_str())),
            ElevenLabsFormValue::Text(_) => None,
        })
    }

    /// `multipart/form-data; boundary=…` for [`Self::encode`].
    pub fn content_type(boundary: &str) -> String {
        format!("multipart/form-data; boundary={boundary}")
    }

    /// Encode the form as a `multipart/form-data` body.
    pub fn encode(&self, boundary: &str) -> Vec<u8> {
        let mut body = Vec::new();
        for (name, value) in &self.fields {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            match value {
                ElevenLabsFormValue::Text(text) => {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                            escape_quoted(name)
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(text.as_bytes());
                }
                ElevenLabsFormValue::File {
                    data,
                    filename,
                    mime_type,
                } => {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                            escape_quoted(name),
                            escape_quoted(filename),
                            mime_type
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(data);
                }
            }
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        body
    }
}

fn escape_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ")
}

/// A boundary that does not occur in `data`.
pub fn multipart_boundary(data: &[u8]) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut boundary = format!("gaise-elevenlabs-{nanos:x}");
    while data
        .windows(boundary.len())
        .any(|w| w == boundary.as_bytes())
    {
        boundary.push('x');
    }
    boundary
}

/// MIME type and file extension for audio bytes: the declared type, else a
/// sniffed container, else `application/octet-stream`.
pub fn audio_mime_and_extension(data: &[u8], declared: Option<&str>) -> (String, &'static str) {
    let sniffed: Option<&'static str> = if data.starts_with(b"RIFF") {
        Some("audio/wav")
    } else if data.starts_with(b"ID3")
        || (data.len() > 1 && data[0] == 0xFF && data[1] & 0xE0 == 0xE0)
    {
        Some("audio/mpeg")
    } else if data.starts_with(b"OggS") {
        Some("audio/ogg")
    } else if data.starts_with(b"fLaC") {
        Some("audio/flac")
    } else if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some("audio/webm")
    } else if data.len() > 8 && &data[4..8] == b"ftyp" {
        Some("audio/mp4")
    } else {
        None
    };
    let mime = declared
        .filter(|m| !m.trim().is_empty())
        .map(str::to_string)
        .or_else(|| sniffed.map(str::to_string))
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let ext = match mime.split(';').next().unwrap_or("").trim() {
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/webm" | "video/webm" => "webm",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => "m4a",
        "video/mp4" => "mp4",
        "audio/pcm" | "audio/l16" => "pcm",
        _ => "bin",
    };
    (mime, ext)
}

fn decode_data_url(url: &str) -> Result<(Vec<u8>, Option<String>), String> {
    let rest = &url[5..];
    let (meta, payload) = rest
        .split_once(',')
        .ok_or("malformed data URL for transcription audio")?;
    if !meta.ends_with(";base64") {
        return Err("data URLs for transcription audio must be base64-encoded".into());
    }
    let mime = meta.trim_end_matches(";base64");
    let data = base64::prelude::BASE64_STANDARD
        .decode(payload.trim().as_bytes())
        .map_err(|e| format!("invalid base64 in transcription data URL: {e}"))?;
    Ok((data, (!mime.is_empty()).then(|| mime.to_string())))
}

/// Stringify a `provider_options` value for a form field or query param.
fn option_strings(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(items) => items.iter().flat_map(option_strings).collect(),
        other => vec![other.to_string()],
    }
}

fn reject_webhook(options: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let webhook = options.get("webhook").is_some_and(|v| {
        v.as_bool() == Some(true) || v.as_str().is_some_and(|s| s.eq_ignore_ascii_case("true"))
    });
    if webhook {
        return Err(
            "ElevenLabs webhook transcription (HTTP 202, result delivered later) is not supported by GAISe; remove `webhook` from provider_options".into(),
        );
    }
    Ok(())
}

fn provider_options_map(
    options: Option<&serde_json::Value>,
) -> Result<Option<&serde_json::Map<String, serde_json::Value>>, String> {
    match options {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Object(map)) => Ok(Some(map)),
        Some(_) => Err("provider_options must be a JSON object".into()),
    }
}

/// `file_format` for headerless audio: `pcm_s16le_16` for 16 kHz mono PCM16;
/// `None` for container formats (sniffed by the API). Other raw layouts are
/// rejected because the batch API only accepts that one.
pub fn transcription_file_format(
    format: &GaiseAudioFormat,
) -> Result<Option<&'static str>, String> {
    let codec = format.codec.trim().to_ascii_lowercase();
    match codec.as_str() {
        "pcm" | "pcm_s16le" | "s16le" | "linear16" | "audio/pcm" | "audio/l16" => {
            let rate = format.sample_rate.unwrap_or(16_000);
            let channels = format.channels.unwrap_or(1);
            if rate != 16_000 || channels != 1 {
                return Err(format!(
                    "ElevenLabs speech-to-text accepts raw PCM only as 16-bit 16 kHz mono (pcm_s16le_16); got {rate} Hz, {channels} channel(s). Send a container format (wav, mp3, …) instead"
                ));
            }
            Ok(Some("pcm_s16le_16"))
        }
        "mulaw" | "ulaw" | "alaw" | "pcm_mulaw" | "pcm_alaw" => Err(format!(
            "ElevenLabs speech-to-text does not accept headerless {codec} audio; wrap it in a WAV container"
        )),
        _ => Ok(None),
    }
}

fn model_supports_no_verbatim(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    model.starts_with("scribe_v2") && !model.contains("medical") && !model.contains("realtime")
}

/// Build the multipart form for `POST /v1/speech-to-text`.
pub fn build_transcription_form(
    request: &GaiseTranscriptionRequest,
) -> Result<ElevenLabsTranscriptionForm, String> {
    request.validate()?;
    let mut form = ElevenLabsTranscriptionForm::default();
    form.push_text("model_id", request.model.trim());

    match request
        .audio
        .as_ref()
        .ok_or("transcription request needs audio")?
    {
        GaiseMediaSource::Bytes { data, mime_type } => {
            push_file(&mut form, data.clone(), mime_type.as_deref());
        }
        GaiseMediaSource::Url { url, mime_type } => {
            let lower = url.trim().to_ascii_lowercase();
            if lower.starts_with("data:") {
                let (data, declared) = decode_data_url(url.trim())?;
                push_file(
                    &mut form,
                    data,
                    mime_type.as_deref().or(declared.as_deref()),
                );
            } else if lower.starts_with("https://") || lower.starts_with("http://") {
                form.push_text("source_url", url.trim());
            } else {
                return Err(format!(
                    "ElevenLabs speech-to-text fetches only http(s) URLs (source_url); pass bytes for {url}"
                ));
            }
        }
    }

    if let Some(language) = request
        .languages
        .iter()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
    {
        form.push_text("language_code", language);
    }
    if request
        .prompt
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty())
    {
        return Err(
            "ElevenLabs batch speech-to-text has no prompt / context field; use keyterms instead (previous_text is realtime-only)".into(),
        );
    }
    if request.diarize {
        form.push_text("diarize", "true");
    }
    if let Some(max) = request.max_speakers {
        if max == 0 || max > MAX_SPEAKERS {
            return Err(format!(
                "ElevenLabs num_speakers must be between 1 and {MAX_SPEAKERS}; got {max}"
            ));
        }
        form.push_text("num_speakers", max.to_string());
    }
    match request.timestamps {
        Some(GaiseTimestampGranularity::Word) | Some(GaiseTimestampGranularity::Segment) => {
            form.push_text("timestamps_granularity", "word")
        }
        Some(GaiseTimestampGranularity::Character) => {
            form.push_text("timestamps_granularity", "character")
        }
        Some(GaiseTimestampGranularity::None) | None => {}
    }
    if let Some(format) = &request.format
        && let Some(file_format) = transcription_file_format(format)?
    {
        if form.text("source_url").is_some() {
            return Err(
                "file_format pcm_s16le_16 applies to uploaded bytes, not source_url".into(),
            );
        }
        form.push_text("file_format", file_format);
    }
    if let Some(t) = request.temperature {
        if !(0.0..=2.0).contains(&t) {
            return Err(format!(
                "ElevenLabs speech-to-text temperature must be within 0.0–2.0; got {t}"
            ));
        }
        form.push_text("temperature", format_f32(t));
    }
    if request.multichannel {
        form.push_text("use_multi_channel", "true");
    }
    for term in request
        .keyterms
        .iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
    {
        form.push_text("keyterms", term);
    }
    if request.mode == Some(GaiseTranscriptionMode::Smart) {
        if !model_supports_no_verbatim(&request.model) {
            return Err(format!(
                "ElevenLabs no_verbatim (smart mode) is supported by scribe_v2 only, not {}",
                request.model
            ));
        }
        form.push_text("no_verbatim", "true");
    }

    if let Some(options) = provider_options_map(request.provider_options.as_ref())? {
        reject_webhook(options)?;
        for (key, value) in options {
            if key == "file" {
                return Err("provider_options cannot replace the audio `file` part".into());
            }
            form.remove(key);
            for v in option_strings(value) {
                form.push_text(key, v);
            }
        }
    }
    Ok(form)
}

fn push_file(form: &mut ElevenLabsTranscriptionForm, data: Vec<u8>, declared: Option<&str>) {
    let (mime_type, ext) = audio_mime_and_extension(&data, declared);
    form.fields.push((
        "file".to_string(),
        ElevenLabsFormValue::File {
            data,
            filename: format!("audio.{ext}"),
            mime_type,
        },
    ));
}

fn format_f32(value: f32) -> String {
    // Round-trip through f64 formatting without float noise (0.2 → "0.2").
    let s = format!("{value}");
    if s.contains('.') || s.contains('e') {
        s
    } else {
        format!("{s}.0")
    }
}

// ── Batch response ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ElevenLabsTranscriptCharacter {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ElevenLabsTranscriptWord {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    /// `word`, `spacing`, or `audio_event`.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub speaker_id: Option<String>,
    #[serde(default)]
    pub logprob: Option<f64>,
    #[serde(default)]
    pub channel_index: Option<u32>,
    #[serde(default)]
    pub characters: Option<Vec<ElevenLabsTranscriptCharacter>>,
}

/// `SpeechToTextChunkResponseModel`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ElevenLabsTranscriptChunk {
    #[serde(default)]
    pub language_code: Option<String>,
    #[serde(default)]
    pub language_probability: Option<f64>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub words: Vec<ElevenLabsTranscriptWord>,
    #[serde(default)]
    pub channel_index: Option<u32>,
    #[serde(default)]
    pub transcription_id: Option<String>,
    #[serde(default)]
    pub audio_duration_secs: Option<f64>,
}

/// `MultichannelSpeechToTextResponseModel`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ElevenLabsMultichannelTranscript {
    #[serde(default)]
    pub transcripts: Vec<ElevenLabsTranscriptChunk>,
    #[serde(default)]
    pub transcription_id: Option<String>,
    #[serde(default)]
    pub audio_duration_secs: Option<f64>,
}

/// `exp(logprob)` clamped to 0–1.
pub fn logprob_to_confidence(logprob: f64) -> f64 {
    if logprob.is_nan() {
        return 0.0;
    }
    logprob.exp().clamp(0.0, 1.0)
}

pub fn map_transcript_word(
    word: &ElevenLabsTranscriptWord,
    channel: Option<u32>,
) -> GaiseTranscriptWord {
    let kind = match word.kind.as_deref() {
        Some("spacing") => GaiseTranscriptTokenKind::Spacing,
        Some("audio_event") => GaiseTranscriptTokenKind::AudioEvent,
        _ => GaiseTranscriptTokenKind::Word,
    };
    GaiseTranscriptWord {
        text: word.text.clone(),
        start: word.start,
        end: word.end,
        confidence: word.logprob.map(logprob_to_confidence),
        speaker: word.speaker_id.clone(),
        kind,
        channel: word.channel_index.or(channel),
    }
}

/// Speaker turns: consecutive non-spacing tokens with the same speaker and
/// channel. Empty when no word carries a speaker.
pub fn speaker_segments(words: &[GaiseTranscriptWord]) -> Vec<GaiseTranscriptSegment> {
    if !words.iter().any(|w| w.speaker.is_some()) {
        return Vec::new();
    }
    let mut segments: Vec<GaiseTranscriptSegment> = Vec::new();
    for word in words {
        if word.kind == GaiseTranscriptTokenKind::Spacing {
            if let Some(last) = segments.last_mut() {
                last.text.push_str(&word.text);
            }
            continue;
        }
        match segments.last_mut() {
            Some(last) if last.speaker == word.speaker && last.channel == word.channel => {
                last.text.push_str(&word.text);
                if word.end.is_some() {
                    last.end = word.end;
                }
            }
            _ => segments.push(GaiseTranscriptSegment {
                text: word.text.clone(),
                start: word.start,
                end: word.end,
                speaker: word.speaker.clone(),
                channel: word.channel,
            }),
        }
    }
    for segment in &mut segments {
        segment.text = segment.text.trim().to_string();
    }
    segments
}

fn audio_usage(seconds: Option<f64>) -> Option<GaiseUsage> {
    let seconds = seconds.filter(|s| s.is_finite() && *s >= 0.0)?;
    Some(GaiseUsage {
        input: Some(HashMap::from([(
            "audio_milliseconds".to_string(),
            (seconds * 1000.0).round() as usize,
        )])),
        output: None,
        total: None,
    })
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty()).cloned()
}

/// Map a `/v1/speech-to-text` JSON body (single- or multichannel) onto
/// [`GaiseTranscriptionResponse`].
pub fn map_transcription_response(
    body: &serde_json::Value,
) -> Result<GaiseTranscriptionResponse, String> {
    if body.get("transcripts").is_some() {
        let multi: ElevenLabsMultichannelTranscript = serde_json::from_value(body.clone())
            .map_err(|e| format!("failed to parse ElevenLabs multichannel transcript: {e}"))?;
        return Ok(map_multichannel(&multi));
    }
    if body.get("text").is_none() && body.get("request_id").is_some() {
        return Err(
            "ElevenLabs accepted the transcription asynchronously (webhook mode); GAISe needs a synchronous result".into(),
        );
    }
    let chunk: ElevenLabsTranscriptChunk = serde_json::from_value(body.clone())
        .map_err(|e| format!("failed to parse ElevenLabs transcript: {e}"))?;
    let words: Vec<GaiseTranscriptWord> = chunk
        .words
        .iter()
        .map(|w| map_transcript_word(w, chunk.channel_index))
        .collect();
    Ok(GaiseTranscriptionResponse {
        text: chunk.text.clone(),
        language: non_empty(chunk.language_code.as_ref()),
        duration_seconds: chunk.audio_duration_secs,
        segments: speaker_segments(&words),
        words,
        channels: Vec::new(),
        usage: audio_usage(chunk.audio_duration_secs),
        external_id: chunk.transcription_id.clone(),
    })
}

fn map_multichannel(multi: &ElevenLabsMultichannelTranscript) -> GaiseTranscriptionResponse {
    let mut channels = Vec::new();
    let mut all_words = Vec::new();
    for (i, chunk) in multi.transcripts.iter().enumerate() {
        let index = chunk.channel_index.unwrap_or(i as u32);
        let words: Vec<GaiseTranscriptWord> = chunk
            .words
            .iter()
            .map(|w| map_transcript_word(w, Some(index)))
            .collect();
        all_words.extend(words.iter().cloned());
        channels.push(GaiseTranscriptionChannel {
            index,
            text: chunk.text.clone(),
            language: non_empty(chunk.language_code.as_ref()),
            words,
        });
    }
    // Interleave by start time (stable, so untimed tokens keep their order).
    all_words.sort_by(|a, b| {
        a.start
            .unwrap_or(f64::MAX)
            .partial_cmp(&b.start.unwrap_or(f64::MAX))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let text = channels
        .iter()
        .map(|c| c.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let language = channels.iter().find_map(|c| c.language.clone());
    let duration = multi.audio_duration_secs.or_else(|| {
        multi
            .transcripts
            .iter()
            .filter_map(|t| t.audio_duration_secs)
            .reduce(f64::max)
    });
    let external_id = multi.transcription_id.clone().or_else(|| {
        multi
            .transcripts
            .iter()
            .find_map(|t| t.transcription_id.clone())
    });
    GaiseTranscriptionResponse {
        text,
        language,
        duration_seconds: duration,
        segments: speaker_segments(&all_words),
        words: all_words,
        channels,
        usage: audio_usage(multi.audio_duration_secs),
        external_id,
    }
}

// ── Realtime (WebSocket) ──────────────────────────────────────────────────

/// Realtime `audio_format` and its sample rate. PCM16 at 8/16/22.05/24/44.1/48
/// kHz and μ-law at 8 kHz; everything else is rejected.
pub fn realtime_audio_format(format: &GaiseAudioFormat) -> Result<(String, u32), String> {
    let codec = format.codec.trim().to_ascii_lowercase();
    if let Some(channels) = format.channels
        && channels != 1
    {
        return Err(format!(
            "ElevenLabs realtime speech-to-text takes mono audio; got {channels} channels"
        ));
    }
    match codec.as_str() {
        "pcm" | "pcm_s16le" | "s16le" | "linear16" | "audio/pcm" | "audio/l16" => {
            let rate = format.sample_rate.unwrap_or(16_000);
            match rate {
                8_000 | 16_000 | 22_050 | 24_000 | 44_100 | 48_000 => {
                    Ok((format!("pcm_{rate}"), rate))
                }
                other => Err(format!(
                    "ElevenLabs realtime speech-to-text accepts PCM at 8000, 16000, 22050, 24000, 44100 or 48000 Hz; got {other}"
                )),
            }
        }
        "mulaw" | "ulaw" | "pcm_mulaw" | "audio/basic" => match format.sample_rate.unwrap_or(8_000)
        {
            8_000 => Ok(("ulaw_8000".to_string(), 8_000)),
            other => Err(format!(
                "ElevenLabs realtime speech-to-text accepts μ-law at 8000 Hz only; got {other}"
            )),
        },
        other => Err(format!(
            "ElevenLabs realtime speech-to-text accepts raw pcm or mulaw audio, not `{other}`"
        )),
    }
}

/// Resolved realtime session parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct ElevenLabsRealtimeSetup {
    pub url: String,
    pub model: String,
    pub audio_format: String,
    pub sample_rate: u32,
    pub include_timestamps: bool,
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `https://host` → `wss://host` (and `http` → `ws`).
pub fn websocket_base(api_url: &str) -> String {
    let api_url = api_url.trim_end_matches('/');
    if let Some(rest) = api_url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = api_url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        api_url.to_string()
    }
}

/// Build the `/v1/speech-to-text/realtime` URL and session parameters.
pub fn build_realtime_transcription_setup(
    api_url: &str,
    config: &GaiseTranscriptionSessionConfig,
) -> Result<ElevenLabsRealtimeSetup, String> {
    if config.diarize {
        return Err(
            "ElevenLabs realtime speech-to-text has no diarization; use batch transcribe with diarize".into(),
        );
    }
    let model = if config.model.trim().is_empty() {
        DEFAULT_REALTIME_TRANSCRIPTION_MODEL.to_string()
    } else {
        config.model.trim().to_string()
    };
    let (audio_format, sample_rate) = realtime_audio_format(&config.audio_format_or_default())?;
    let include_timestamps = matches!(
        config.timestamps,
        Some(
            GaiseTimestampGranularity::Word
                | GaiseTimestampGranularity::Character
                | GaiseTimestampGranularity::Segment
        )
    );

    let mut params: Vec<(String, String)> = vec![
        ("model_id".into(), model.clone()),
        ("audio_format".into(), audio_format.clone()),
    ];
    if let Some(language) = config
        .languages
        .iter()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
    {
        params.push(("language_code".into(), language.to_string()));
    }
    if let Some(strategy) = config.commit_strategy {
        let value = match strategy {
            GaiseCommitStrategy::Manual => "manual",
            GaiseCommitStrategy::TurnPrediction => "turn_prediction",
            _ => "vad",
        };
        params.push(("commit_strategy".into(), value.into()));
    }
    if let Some(ms) = config.silence_duration_ms {
        params.push((
            "vad_silence_threshold_secs".into(),
            format!("{}", f64::from(ms) / 1000.0),
        ));
    }
    if include_timestamps {
        params.push(("include_timestamps".into(), "true".into()));
    }
    for term in config
        .keyterms
        .iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
    {
        params.push(("keyterms".into(), term.to_string()));
    }
    if let Some(options) = provider_options_map(config.provider_options.as_ref())? {
        for (key, value) in options {
            if key == "token" {
                return Err(
                    "provider_options cannot set `token`; the client authenticates with xi-api-key"
                        .into(),
                );
            }
            params.retain(|(k, _)| k != key);
            for v in option_strings(value) {
                params.push((key.clone(), v));
            }
        }
    }
    let include_timestamps = params
        .iter()
        .any(|(k, v)| k == "include_timestamps" && v.eq_ignore_ascii_case("true"));
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    Ok(ElevenLabsRealtimeSetup {
        url: format!(
            "{}/v1/speech-to-text/realtime?{query}",
            websocket_base(api_url)
        ),
        model,
        audio_format,
        sample_rate,
        include_timestamps,
    })
}

/// One `input_audio_chunk` client frame. `previous_text` is only valid on
/// the first chunk of a session.
pub fn realtime_audio_chunk(
    audio: &[u8],
    sample_rate: u32,
    commit: bool,
    previous_text: Option<&str>,
) -> String {
    let mut frame = serde_json::json!({
        "message_type": "input_audio_chunk",
        "audio_base_64": base64::prelude::BASE64_STANDARD.encode(audio),
        "commit": commit,
        "sample_rate": sample_rate,
    });
    if let Some(text) = previous_text.filter(|t| !t.trim().is_empty()) {
        frame["previous_text"] = serde_json::Value::String(text.to_string());
    }
    frame.to_string()
}

/// Server error `message_type`s after which the session cannot continue.
pub fn realtime_error_is_fatal(message_type: &str) -> bool {
    matches!(
        message_type,
        "auth_error"
            | "quota_exceeded"
            | "unaccepted_terms"
            | "rate_limited"
            | "queue_overflow"
            | "resource_exhausted"
            | "session_time_limit_exceeded"
            | "invalid_request"
    )
}

const REALTIME_ERROR_TYPES: &[&str] = &[
    "error",
    "auth_error",
    "quota_exceeded",
    "commit_throttled",
    "unaccepted_terms",
    "rate_limited",
    "queue_overflow",
    "resource_exhausted",
    "session_time_limit_exceeded",
    "input_error",
    "invalid_request",
    "chunk_size_exceeded",
    "insufficient_audio_activity",
    "transcriber_error",
];

/// Stateful mapper from realtime server frames to transcription events.
///
/// With `include_timestamps`, the server sends both `committed_transcript`
/// and `committed_transcript_with_timestamps` for one segment; only the
/// timestamped one is emitted. A plain commit is held until the next frame
/// and released (without words) if no timestamped copy follows.
#[derive(Debug, Clone)]
pub struct ElevenLabsRealtimeMapper {
    model: String,
    include_timestamps: bool,
    interim_results: bool,
    pending_plain: Option<String>,
    last_timestamped: Option<String>,
    commits: u64,
    ended: bool,
}

impl ElevenLabsRealtimeMapper {
    pub fn new(model: impl Into<String>, include_timestamps: bool, interim_results: bool) -> Self {
        Self {
            model: model.into(),
            include_timestamps,
            interim_results,
            pending_plain: None,
            last_timestamped: None,
            commits: 0,
            ended: false,
        }
    }

    /// Committed segments seen so far (plain or timestamped).
    pub fn commits(&self) -> u64 {
        self.commits
    }

    /// Whether an `Ended` event has been produced.
    pub fn ended(&self) -> bool {
        self.ended
    }

    fn final_transcript(text: String) -> GaiseTranscriptionEvent {
        GaiseTranscriptionEvent::Transcript(GaiseLiveTranscript {
            text,
            is_final: true,
            utterance_end: true,
            ..Default::default()
        })
    }

    fn release_pending(&mut self, out: &mut Vec<GaiseTranscriptionEvent>) {
        if let Some(text) = self.pending_plain.take() {
            out.push(Self::final_transcript(text));
        }
    }

    /// Map one server text frame.
    pub fn map_frame(&mut self, frame: &str) -> Vec<GaiseTranscriptionEvent> {
        let value: serde_json::Value = match serde_json::from_str(frame) {
            Ok(v) => v,
            Err(e) => {
                return vec![GaiseTranscriptionEvent::Error {
                    message: format!("invalid ElevenLabs realtime frame: {e}"),
                }];
            }
        };
        let message_type = value
            .get("message_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let text = value
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut out = Vec::new();
        match message_type.as_str() {
            "session_started" => {
                let session_id = value
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let model = value
                    .pointer("/config/model_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| self.model.clone());
                out.push(GaiseTranscriptionEvent::Started { session_id, model });
            }
            "partial_transcript" => {
                self.release_pending(&mut out);
                if self.interim_results && !text.is_empty() {
                    out.push(GaiseTranscriptionEvent::Transcript(GaiseLiveTranscript {
                        text,
                        is_final: false,
                        ..Default::default()
                    }));
                }
            }
            "committed_transcript" => {
                self.commits += 1;
                self.release_pending(&mut out);
                if !self.include_timestamps {
                    out.push(Self::final_transcript(text));
                } else if self.last_timestamped.as_deref() == Some(text.as_str()) {
                    // Timestamped copy already emitted for this segment.
                    self.last_timestamped = None;
                } else {
                    self.pending_plain = Some(text);
                }
            }
            "committed_transcript_with_timestamps" => {
                self.commits += 1;
                let duplicate_of_pending = self.pending_plain.as_deref() == Some(text.as_str());
                if duplicate_of_pending {
                    self.pending_plain = None;
                    self.last_timestamped = None;
                } else {
                    self.release_pending(&mut out);
                    self.last_timestamped = Some(text.clone());
                }
                out.push(map_timestamped_commit(&value, text));
            }
            "warning" => {
                self.release_pending(&mut out);
                let warning = value
                    .get("warning")
                    .and_then(|v| v.as_str())
                    .unwrap_or("warning")
                    .to_string();
                out.push(GaiseTranscriptionEvent::Error {
                    message: format!("ElevenLabs warning: {warning}"),
                });
            }
            t if REALTIME_ERROR_TYPES.contains(&t) => {
                self.release_pending(&mut out);
                let detail = value
                    .get("error")
                    .or_else(|| value.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let message = if detail.is_empty() {
                    format!("ElevenLabs {t}")
                } else {
                    format!("ElevenLabs {t}: {detail}")
                };
                out.push(GaiseTranscriptionEvent::Error { message });
                if realtime_error_is_fatal(t) {
                    self.ended = true;
                    out.push(GaiseTranscriptionEvent::Ended {
                        reason: Some(t.to_string()),
                    });
                }
            }
            // committed_transcript_entities, edited_transcript, unknown: no
            // GAISe event.
            _ => {}
        }
        out
    }

    /// Release any held commit and end the session (socket closed).
    pub fn finish(&mut self, reason: Option<String>) -> Vec<GaiseTranscriptionEvent> {
        let mut out = Vec::new();
        self.release_pending(&mut out);
        if !self.ended {
            self.ended = true;
            out.push(GaiseTranscriptionEvent::Ended { reason });
        }
        out
    }
}

fn map_timestamped_commit(value: &serde_json::Value, text: String) -> GaiseTranscriptionEvent {
    let words: Vec<ElevenLabsTranscriptWord> = value
        .get("words")
        .cloned()
        .and_then(|w| serde_json::from_value(w).ok())
        .unwrap_or_default();
    let words: Vec<GaiseTranscriptWord> =
        words.iter().map(|w| map_transcript_word(w, None)).collect();
    let timed = || {
        words
            .iter()
            .filter(|w| w.kind != GaiseTranscriptTokenKind::Spacing)
    };
    let start = timed().find_map(|w| w.start);
    let end = timed().filter_map(|w| w.end).next_back();
    let speaker = timed().find_map(|w| w.speaker.clone());
    let language = value
        .get("language_code")
        .and_then(|v| v.as_str())
        .filter(|l| !l.is_empty())
        .map(str::to_string);
    GaiseTranscriptionEvent::Transcript(GaiseLiveTranscript {
        text,
        is_final: true,
        utterance_end: true,
        start,
        end,
        speaker,
        language,
        channel: None,
        words,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> GaiseTranscriptionRequest {
        GaiseTranscriptionRequest {
            model: "scribe_v2".into(),
            audio: Some(GaiseMediaSource::Bytes {
                data: b"RIFF....WAVEfmt ".to_vec(),
                mime_type: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn form_maps_every_field() {
        let req = GaiseTranscriptionRequest {
            languages: vec!["en".into(), "fr".into()],
            keyterms: vec!["GAISe".into(), " Scribe ".into(), "".into()],
            diarize: true,
            max_speakers: Some(3),
            timestamps: Some(GaiseTimestampGranularity::Character),
            mode: Some(GaiseTranscriptionMode::Smart),
            multichannel: true,
            temperature: Some(0.2),
            provider_options: Some(serde_json::json!({
                "tag_audio_events": false,
                "seed": 7,
                "entity_detection": ["pii", "phi"],
                "language_code": "de"
            })),
            ..request()
        };
        let form = build_transcription_form(&req).unwrap();
        assert_eq!(form.text("model_id"), Some("scribe_v2"));
        let (data, filename, mime) = form.file().unwrap();
        assert!(data.starts_with(b"RIFF"));
        assert_eq!(filename, "audio.wav");
        assert_eq!(mime, "audio/wav");
        assert_eq!(form.text("language_code"), Some("de"));
        assert_eq!(form.text("diarize"), Some("true"));
        assert_eq!(form.text("num_speakers"), Some("3"));
        assert_eq!(form.text("timestamps_granularity"), Some("character"));
        assert_eq!(form.text("no_verbatim"), Some("true"));
        assert_eq!(form.text("use_multi_channel"), Some("true"));
        assert_eq!(form.text("temperature"), Some("0.2"));
        assert_eq!(form.texts("keyterms"), vec!["GAISe", "Scribe"]);
        assert_eq!(form.text("tag_audio_events"), Some("false"));
        assert_eq!(form.text("seed"), Some("7"));
        assert_eq!(form.texts("entity_detection"), vec!["pii", "phi"]);
        assert!(form.text("webhook").is_none());
        assert!(form.text("file_format").is_none());
    }

    #[test]
    fn form_handles_urls_and_raw_pcm() {
        let req = GaiseTranscriptionRequest {
            audio: Some(GaiseMediaSource::url("https://cdn.example/a.mp3")),
            timestamps: Some(GaiseTimestampGranularity::None),
            ..request()
        };
        let form = build_transcription_form(&req).unwrap();
        assert_eq!(form.text("source_url"), Some("https://cdn.example/a.mp3"));
        assert!(form.file().is_none());
        assert!(form.text("timestamps_granularity").is_none());

        let req = GaiseTranscriptionRequest {
            audio: Some(GaiseMediaSource::url("data:audio/ogg;base64,AAEC")),
            ..request()
        };
        let form = build_transcription_form(&req).unwrap();
        let (data, filename, mime) = form.file().unwrap();
        assert_eq!(data, &[0, 1, 2]);
        assert_eq!((filename, mime), ("audio.ogg", "audio/ogg"));

        let pcm = GaiseTranscriptionRequest {
            audio: Some(GaiseMediaSource::Bytes {
                data: vec![0; 32],
                mime_type: None,
            }),
            format: Some(GaiseAudioFormat::pcm(16_000)),
            ..request()
        };
        let form = build_transcription_form(&pcm).unwrap();
        assert_eq!(form.text("file_format"), Some("pcm_s16le_16"));

        let bad = GaiseTranscriptionRequest {
            format: Some(GaiseAudioFormat::pcm(24_000)),
            ..pcm.clone()
        };
        assert!(
            build_transcription_form(&bad)
                .unwrap_err()
                .contains("16 kHz")
        );
        let gs = GaiseTranscriptionRequest {
            audio: Some(GaiseMediaSource::url("gs://bucket/a.wav")),
            ..request()
        };
        assert!(build_transcription_form(&gs).is_err());
    }

    #[test]
    fn form_rejects_what_the_api_cannot_honour() {
        let webhook = GaiseTranscriptionRequest {
            provider_options: Some(serde_json::json!({"webhook": true})),
            ..request()
        };
        assert!(
            build_transcription_form(&webhook)
                .unwrap_err()
                .contains("webhook")
        );
        let smart_medical = GaiseTranscriptionRequest {
            model: "scribe_v2_medical".into(),
            mode: Some(GaiseTranscriptionMode::Smart),
            ..request()
        };
        assert!(build_transcription_form(&smart_medical).is_err());
        let too_many = GaiseTranscriptionRequest {
            max_speakers: Some(33),
            ..request()
        };
        assert!(build_transcription_form(&too_many).is_err());
        let prompt = GaiseTranscriptionRequest {
            prompt: Some("context".into()),
            ..request()
        };
        assert!(build_transcription_form(&prompt).is_err());
        let verbatim = GaiseTranscriptionRequest {
            mode: Some(GaiseTranscriptionMode::Verbatim),
            ..request()
        };
        assert!(
            build_transcription_form(&verbatim)
                .unwrap()
                .text("no_verbatim")
                .is_none()
        );
    }

    #[test]
    fn multipart_encoding_is_well_formed() {
        let mut form = ElevenLabsTranscriptionForm::default();
        form.push_text("model_id", "scribe_v2");
        push_file(&mut form, vec![1, 2, 3], Some("audio/mpeg"));
        let body = form.encode("BOUND");
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with(
            "--BOUND\r\nContent-Disposition: form-data; name=\"model_id\"\r\n\r\nscribe_v2\r\n"
        ));
        assert!(text.contains(
            "name=\"file\"; filename=\"audio.mp3\"\r\nContent-Type: audio/mpeg\r\n\r\n\u{1}\u{2}\u{3}\r\n"
        ));
        assert!(text.ends_with("--BOUND--\r\n"));
        let boundary = multipart_boundary(b"abc");
        assert!(boundary.starts_with("gaise-elevenlabs-"));
    }

    #[test]
    fn single_channel_response_maps_words_events_and_usage() {
        let body = serde_json::json!({
            "language_code": "en",
            "language_probability": 0.98,
            "text": "Hello world (laughter)",
            "words": [
                {"text": "Hello", "start": 0.0, "end": 0.4, "type": "word", "speaker_id": "speaker_0", "logprob": -0.1},
                {"text": " ", "start": 0.4, "end": 0.5, "type": "spacing", "speaker_id": "speaker_0", "logprob": 0.0},
                {"text": "world", "start": 0.5, "end": 0.9, "type": "word", "speaker_id": "speaker_0", "logprob": 0.5,
                 "characters": [{"text": "w", "start": 0.5, "end": 0.6}]},
                {"text": " ", "start": 0.9, "end": 1.0, "type": "spacing", "speaker_id": "speaker_1", "logprob": 0.0},
                {"text": "(laughter)", "start": 1.0, "end": 1.5, "type": "audio_event", "speaker_id": "speaker_1", "logprob": -2.0}
            ],
            "transcription_id": "tr_1",
            "audio_duration_secs": 1.5
        });
        let response = map_transcription_response(&body).unwrap();
        assert_eq!(response.text, "Hello world (laughter)");
        assert_eq!(response.language.as_deref(), Some("en"));
        assert_eq!(response.external_id.as_deref(), Some("tr_1"));
        assert_eq!(response.duration_seconds, Some(1.5));
        assert_eq!(
            response.usage.unwrap().input.unwrap()["audio_milliseconds"],
            1500
        );
        assert_eq!(response.words.len(), 5);
        assert_eq!(response.words[1].kind, GaiseTranscriptTokenKind::Spacing);
        assert_eq!(response.words[4].kind, GaiseTranscriptTokenKind::AudioEvent);
        assert!((response.words[0].confidence.unwrap() - (-0.1f64).exp()).abs() < 1e-9);
        assert_eq!(response.words[2].confidence, Some(1.0));
        assert_eq!(response.words[0].speaker.as_deref(), Some("speaker_0"));
        assert_eq!(response.segments.len(), 2);
        assert_eq!(response.segments[0].text, "Hello world");
        assert_eq!(response.segments[0].end, Some(0.9));
        assert_eq!(response.segments[1].text, "(laughter)");
        assert!(response.channels.is_empty());
    }

    #[test]
    fn multichannel_response_maps_channels_and_merges() {
        let body = serde_json::json!({
            "transcripts": [
                {"language_code": "en", "text": "Hi there", "channel_index": 0, "words": [
                    {"text": "Hi", "start": 0.0, "end": 0.2, "type": "word", "logprob": 0.0, "channel_index": 0},
                    {"text": "there", "start": 1.0, "end": 1.2, "type": "word", "logprob": 0.0, "channel_index": 0}
                ]},
                {"language_code": "en", "text": "Hello", "channel_index": 1, "words": [
                    {"text": "Hello", "start": 0.5, "end": 0.8, "type": "word", "logprob": 0.0, "channel_index": 1}
                ]}
            ],
            "transcription_id": "tr_m",
            "audio_duration_secs": 2.0
        });
        let response = map_transcription_response(&body).unwrap();
        assert_eq!(response.text, "Hi there\nHello");
        assert_eq!(response.channels.len(), 2);
        assert_eq!(response.channels[1].index, 1);
        assert_eq!(response.channels[1].words[0].channel, Some(1));
        let order: Vec<_> = response.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(order, vec!["Hi", "Hello", "there"]);
        assert_eq!(response.external_id.as_deref(), Some("tr_m"));
        assert_eq!(
            response.usage.unwrap().input.unwrap()["audio_milliseconds"],
            2000
        );
        assert!(response.segments.is_empty());
    }

    #[test]
    fn webhook_acknowledgement_is_an_error() {
        let body =
            serde_json::json!({"message": "ok", "request_id": "r", "transcription_id": null});
        assert!(map_transcription_response(&body).is_err());
    }

    #[test]
    fn realtime_setup_builds_query() {
        let config = GaiseTranscriptionSessionConfig {
            model: "scribe_v2_realtime".into(),
            audio_format: Some(GaiseAudioFormat::pcm(24_000)),
            languages: vec!["en".into()],
            keyterms: vec!["GAISe".into(), "two words".into()],
            timestamps: Some(GaiseTimestampGranularity::Word),
            commit_strategy: Some(GaiseCommitStrategy::TurnPrediction),
            silence_duration_ms: Some(1500),
            provider_options: Some(serde_json::json!({"no_verbatim": true, "vad_threshold": 0.4})),
            ..Default::default()
        };
        let setup =
            build_realtime_transcription_setup("https://api.eu.residency.elevenlabs.io/", &config)
                .unwrap();
        assert_eq!(
            setup.url,
            "wss://api.eu.residency.elevenlabs.io/v1/speech-to-text/realtime?model_id=scribe_v2_realtime&audio_format=pcm_24000&language_code=en&commit_strategy=turn_prediction&vad_silence_threshold_secs=1.5&include_timestamps=true&keyterms=GAISe&keyterms=two%20words&no_verbatim=true&vad_threshold=0.4"
        );
        assert_eq!(setup.sample_rate, 24_000);
        assert!(setup.include_timestamps);

        let defaults = build_realtime_transcription_setup(
            "http://127.0.0.1:9",
            &GaiseTranscriptionSessionConfig::default(),
        )
        .unwrap();
        assert_eq!(
            defaults.url,
            "ws://127.0.0.1:9/v1/speech-to-text/realtime?model_id=scribe_v2_realtime&audio_format=pcm_16000"
        );
        assert!(!defaults.include_timestamps);
    }

    #[test]
    fn realtime_formats_are_validated() {
        let ulaw = GaiseAudioFormat {
            codec: "mulaw".into(),
            sample_rate: Some(8_000),
            channels: None,
        };
        assert_eq!(
            realtime_audio_format(&ulaw).unwrap(),
            ("ulaw_8000".to_string(), 8_000)
        );
        assert!(realtime_audio_format(&GaiseAudioFormat::pcm(11_025)).is_err());
        assert!(
            realtime_audio_format(&GaiseAudioFormat {
                codec: "opus".into(),
                ..Default::default()
            })
            .is_err()
        );
        let diarize = GaiseTranscriptionSessionConfig {
            diarize: true,
            ..Default::default()
        };
        assert!(build_realtime_transcription_setup("https://x", &diarize).is_err());
    }

    #[test]
    fn realtime_chunk_frames() {
        let first: serde_json::Value =
            serde_json::from_str(&realtime_audio_chunk(&[0, 1, 2], 16_000, false, Some("Hi")))
                .unwrap();
        assert_eq!(first["message_type"], "input_audio_chunk");
        assert_eq!(first["audio_base_64"], "AAEC");
        assert_eq!(first["commit"], false);
        assert_eq!(first["sample_rate"], 16_000);
        assert_eq!(first["previous_text"], "Hi");
        let commit: serde_json::Value =
            serde_json::from_str(&realtime_audio_chunk(&[], 8_000, true, None)).unwrap();
        assert_eq!(commit["audio_base_64"], "");
        assert_eq!(commit["commit"], true);
        assert!(commit.get("previous_text").is_none());
    }

    #[test]
    fn realtime_mapper_without_timestamps() {
        let mut mapper = ElevenLabsRealtimeMapper::new("scribe_v2_realtime", false, true);
        let started = mapper.map_frame(
            r#"{"message_type":"session_started","session_id":"s1","config":{"model_id":"scribe_v2_realtime"}}"#,
        );
        assert!(
            matches!(&started[0], GaiseTranscriptionEvent::Started { session_id, model }
            if session_id == "s1" && model == "scribe_v2_realtime")
        );
        let partial = mapper.map_frame(r#"{"message_type":"partial_transcript","text":"hel"}"#);
        assert!(
            matches!(&partial[0], GaiseTranscriptionEvent::Transcript(t) if !t.is_final && t.text == "hel")
        );
        let committed =
            mapper.map_frame(r#"{"message_type":"committed_transcript","text":"hello"}"#);
        assert!(
            matches!(&committed[0], GaiseTranscriptionEvent::Transcript(t)
            if t.is_final && t.utterance_end && t.text == "hello")
        );
        assert_eq!(mapper.commits(), 1);

        let mut quiet = ElevenLabsRealtimeMapper::new("m", false, false);
        assert!(
            quiet
                .map_frame(r#"{"message_type":"partial_transcript","text":"hel"}"#)
                .is_empty()
        );
    }

    #[test]
    fn realtime_mapper_prefers_timestamped_commits() {
        let mut mapper = ElevenLabsRealtimeMapper::new("m", true, false);
        assert!(
            mapper
                .map_frame(r#"{"message_type":"committed_transcript","text":"hi there"}"#)
                .is_empty()
        );
        let events = mapper.map_frame(
            r#"{"message_type":"committed_transcript_with_timestamps","text":"hi there","language_code":"en","words":[
                {"text":"hi","start":0.1,"end":0.3,"type":"word","speaker_id":"speaker_0","logprob":-0.5},
                {"text":" ","start":0.3,"end":0.35,"type":"spacing","logprob":0.0},
                {"text":"there","start":0.35,"end":0.7,"type":"word","speaker_id":"speaker_0","logprob":0.0}]}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            GaiseTranscriptionEvent::Transcript(t) => {
                assert!(t.is_final && t.utterance_end);
                assert_eq!(t.words.len(), 3);
                assert_eq!(t.start, Some(0.1));
                assert_eq!(t.end, Some(0.7));
                assert_eq!(t.language.as_deref(), Some("en"));
                assert_eq!(t.speaker.as_deref(), Some("speaker_0"));
            }
            other => panic!("unexpected {other:?}"),
        }
        // Reverse order: timestamped first, then the plain duplicate.
        assert_eq!(
            mapper
                .map_frame(r#"{"message_type":"committed_transcript_with_timestamps","text":"again","words":[]}"#)
                .len(),
            1
        );
        assert!(
            mapper
                .map_frame(r#"{"message_type":"committed_transcript","text":"again"}"#)
                .is_empty()
        );
        // A plain commit with no timestamped copy is released at the end.
        assert!(
            mapper
                .map_frame(r#"{"message_type":"committed_transcript","text":"tail"}"#)
                .is_empty()
        );
        let end = mapper.finish(None);
        assert!(matches!(&end[0], GaiseTranscriptionEvent::Transcript(t) if t.text == "tail"));
        assert!(matches!(&end[1], GaiseTranscriptionEvent::Ended { .. }));
        assert!(mapper.finish(None).is_empty());
    }

    #[test]
    fn realtime_mapper_errors() {
        let mut mapper = ElevenLabsRealtimeMapper::new("m", false, false);
        let soft = mapper
            .map_frame(r#"{"message_type":"insufficient_audio_activity","error":"no speech"}"#);
        assert_eq!(soft.len(), 1);
        assert!(
            matches!(&soft[0], GaiseTranscriptionEvent::Error { message }
            if message == "ElevenLabs insufficient_audio_activity: no speech")
        );
        let fatal = mapper.map_frame(r#"{"message_type":"auth_error","error":"bad key"}"#);
        assert_eq!(fatal.len(), 2);
        assert!(
            matches!(&fatal[1], GaiseTranscriptionEvent::Ended { reason: Some(r) } if r == "auth_error")
        );
        assert!(mapper.ended());
        assert!(mapper.finish(None).is_empty());
        let mut other = ElevenLabsRealtimeMapper::new("m", false, false);
        assert!(matches!(
            &other.map_frame("not json")[0],
            GaiseTranscriptionEvent::Error { .. }
        ));
        assert!(
            other
                .map_frame(
                    r#"{"message_type":"committed_transcript_entities","text":"x","entities":[]}"#
                )
                .is_empty()
        );
    }
}
