//! Media operations on Vertex AI / Gemini Enterprise Agent Platform
//! (`aiplatform.googleapis.com`, the client's `{{MODEL}}` URL template):
//!
//! - image generation and editing with the Nano Banana models and file
//!   transcription with `gemini-3.5-transcribe`, both through
//!   `{{MODEL}}:generateContent` (Imagen is discontinued on Vertex);
//! - Veo 3.1 video jobs through `{{MODEL}}:predictLongRunning`, polled with
//!   `{{MODEL}}:fetchPredictOperation`.
//!
//! The builders and mappers here are pure; [`crate::vertexai_client`] only
//! adds auth and the HTTP calls.
//!
//! `provider_options`: for `generateContent` each top-level key is merged into
//! `generationConfig` (replacing a key the adapter set), except the
//! request-level keys in [`REQUEST_LEVEL_KEYS`] (`safetySettings`,
//! `systemInstruction`, `tools`, `toolConfig`, `cachedContent`, `labels`),
//! which go to the body root. For Veo each top-level key is merged into
//! `parameters` (`personGeneration`, `enhancePrompt`, `compressionQuality`,
//! `resizeMode`, `fps`, …), except `instance`, whose keys are merged into
//! `instances[0]` (`cameraControl`, `mask`, …).

use base64::Engine;
use gaise_core::contracts::{
    GaiseGeneratedMedia, GaiseImageRequest, GaiseImageResponse, GaiseJobStatus, GaiseMediaSource,
    GaiseTimestampGranularity, GaiseTranscriptSegment, GaiseTranscriptWord, GaiseTranscriptionMode,
    GaiseTranscriptionRequest, GaiseTranscriptionResponse, GaiseVideoJob, GaiseVideoRequest,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::contracts::models::GoogleUsageMetadata;

/// `provider_options` keys that belong at the root of a `generateContent`
/// body rather than inside `generationConfig`.
pub const REQUEST_LEVEL_KEYS: &[&str] = &[
    "safetySettings",
    "systemInstruction",
    "tools",
    "toolConfig",
    "cachedContent",
    "labels",
];

/// Aspect ratios `imageConfig.aspectRatio` accepts: the Gemini API list plus
/// `9:21` from the Vertex Nano Banana 2.1 model card (2026-10-17).
pub const IMAGE_ASPECT_RATIOS: &[&str] = &[
    "1:1", "1:4", "4:1", "1:8", "8:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9",
    "21:9", "9:21",
];

/// Maximum input images per Nano Banana request.
pub const MAX_INPUT_IMAGES: usize = 14;

// ── Shared helpers ────────────────────────────────────────────────────

/// Merge `provider_options` into a `generateContent` body: request-level
/// keys to the root, everything else into `generationConfig`.
pub fn merge_generate_content_options(
    body: &mut Value,
    options: Option<&Value>,
) -> Result<(), String> {
    let Some(options) = options else {
        return Ok(());
    };
    let Value::Object(options) = options else {
        return Err("provider_options must be a JSON object".into());
    };
    let root = body
        .as_object_mut()
        .ok_or("generateContent body must be an object")?;
    for (key, value) in options {
        if REQUEST_LEVEL_KEYS.contains(&key.as_str()) {
            root.insert(key.clone(), value.clone());
        } else {
            root.entry("generationConfig")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or("generationConfig must be an object")?
                .insert(key.clone(), value.clone());
        }
    }
    Ok(())
}

/// MIME type from magic bytes for the image and audio formats Gemini models take.
pub fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    let starts = |prefix: &[u8]| data.starts_with(prefix);
    if starts(b"\x89PNG") {
        Some("image/png")
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        Some("audio/wav")
    } else if starts(b"GIF8") {
        Some("image/gif")
    } else if starts(b"ID3") || (data.len() >= 2 && data[0] == 0xFF && data[1] & 0xE0 == 0xE0) {
        Some("audio/mp3")
    } else if starts(b"fLaC") {
        Some("audio/flac")
    } else if starts(b"OggS") {
        Some("audio/ogg")
    } else if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some("audio/webm")
    } else if data.len() >= 8 && &data[4..8] == b"ftyp" {
        Some("audio/mp4")
    } else {
        None
    }
}

/// MIME type from a URL's file extension.
pub fn mime_from_url(url: &str) -> Option<&'static str> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "wav" => "audio/wav",
        "mp3" => "audio/mp3",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "aac" => "audio/aac",
        "aiff" | "aif" => "audio/aiff",
        "m4a" => "audio/mp4",
        "webm" => "audio/webm",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        _ => return None,
    })
}

fn decode_data_url(url: &str) -> Option<(String, Vec<u8>)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .ok()?;
    Some((mime.to_string(), bytes))
}

/// One request part for a media source: `inlineData` for bytes and `data:`
/// URLs, `fileData{fileUri}` for anything else (Files API URIs, `gs://`,
/// `https://`). `default_mime` is used when neither the source, the bytes,
/// nor the URL extension give a type.
pub fn media_part(source: &GaiseMediaSource, default_mime: &str) -> Result<Value, String> {
    match source {
        GaiseMediaSource::Bytes { data, mime_type } => {
            if data.is_empty() {
                return Err("media source bytes are empty".into());
            }
            let mime = mime_type
                .clone()
                .or_else(|| sniff_mime(data).map(str::to_string))
                .unwrap_or_else(|| default_mime.to_string());
            Ok(json!({"inlineData": {
                "mimeType": mime,
                "data": base64::engine::general_purpose::STANDARD.encode(data),
            }}))
        }
        GaiseMediaSource::Url { url, mime_type } => {
            if let Some((mime, data)) = decode_data_url(url) {
                return media_part(
                    &GaiseMediaSource::Bytes {
                        data,
                        mime_type: mime_type.clone().or(Some(mime).filter(|m| !m.is_empty())),
                    },
                    default_mime,
                );
            }
            if url.starts_with("data:") {
                return Err("data URLs must be base64-encoded".into());
            }
            let mime = mime_type
                .clone()
                .or_else(|| mime_from_url(url).map(str::to_string))
                .unwrap_or_else(|| default_mime.to_string());
            Ok(json!({"fileData": {"mimeType": mime, "fileUri": url}}))
        }
    }
}

/// Seconds from a protobuf Duration string (`"3.5s"`).
pub fn parse_duration_seconds(value: &str) -> Option<f64> {
    value.trim().trim_end_matches('s').parse().ok()
}

// ── Response (shared by images and transcription) ─────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexMediaResponse {
    #[serde(default)]
    pub candidates: Vec<VertexMediaCandidate>,
    #[serde(default)]
    pub prompt_feedback: Option<VertexPromptFeedback>,
    #[serde(default)]
    pub usage_metadata: Option<GoogleUsageMetadata>,
    #[serde(default)]
    pub response_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexPromptFeedback {
    #[serde(default)]
    pub block_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexMediaCandidate {
    #[serde(default)]
    pub content: Option<VertexMediaContent>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct VertexMediaContent {
    #[serde(default)]
    pub parts: Vec<VertexMediaPart>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexMediaPart {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub thought: Option<bool>,
    #[serde(default)]
    pub inline_data: Option<VertexMediaBlob>,
    #[serde(default)]
    pub audio_transcription: Option<VertexAudioTranscription>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexMediaBlob {
    #[serde(default)]
    pub mime_type: Option<String>,
    pub data: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexAudioTranscription {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub speaker_label: Option<String>,
    #[serde(default)]
    pub words: Vec<VertexWordInfo>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexWordInfo {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub start_offset: Option<String>,
    #[serde(default)]
    pub end_offset: Option<String>,
}

const SAFETY_FINISH_REASONS: &[&str] = &[
    "SAFETY",
    "IMAGE_SAFETY",
    "PROHIBITED_CONTENT",
    "IMAGE_PROHIBITED_CONTENT",
    "BLOCKLIST",
    "SPII",
    "RECITATION",
    "IMAGE_RECITATION",
];

impl VertexMediaResponse {
    /// Non-thought parts of every candidate.
    fn answer_parts(&self) -> impl Iterator<Item = &VertexMediaPart> {
        self.candidates
            .iter()
            .filter_map(|c| c.content.as_ref())
            .flat_map(|c| c.parts.iter())
            .filter(|p| p.thought != Some(true))
    }

    fn safety_stopped(&self) -> bool {
        self.candidates.iter().any(|c| {
            c.finish_reason
                .as_deref()
                .is_some_and(|r| SAFETY_FINISH_REASONS.contains(&r))
        })
    }
}

// ── Images ────────────────────────────────────────────────────────────

/// Normalise a resolution tier to the `imageConfig.imageSize` spelling
/// (`512`, `1K`, `2K`, `4K`; uppercase `K` is required).
pub fn normalize_image_size(value: &str) -> Result<String, String> {
    let upper = value.trim().to_ascii_uppercase();
    match upper.as_str() {
        "512" | "0.5K" => Ok("512".into()),
        "1K" | "2K" | "4K" => Ok(upper),
        _ => Err(format!(
            "Gemini image resolution must be 512, 1K, 2K, or 4K (got `{value}`)"
        )),
    }
}

/// Build the `generateContent` body for an image request: the prompt, then
/// up to 14 input images, with `responseModalities: [TEXT, IMAGE]` and
/// `imageConfig{aspectRatio, imageSize}`. Options the Nano Banana models
/// cannot honour (`n > 1`, `size`, `mask`, `negative_prompt`, `quality`,
/// `output_format`, a seed on Gemini 3-generation models) are errors.
pub fn vertex_image_request(request: &GaiseImageRequest) -> Result<Value, String> {
    request.validate()?;
    if request.n.is_some_and(|n| n > 1) {
        return Err("Gemini image models return one image set per request (no candidate count); send n separate requests".into());
    }
    if request.size.is_some() {
        return Err(
            "Gemini image models take aspect_ratio and resolution, not an exact size".into(),
        );
    }
    if request.mask.is_some() {
        return Err(
            "Gemini image models do not take a mask; describe the edit in the prompt".into(),
        );
    }
    if request.negative_prompt.is_some() {
        return Err(
            "Gemini image models do not take a negative prompt; state exclusions in the prompt"
                .into(),
        );
    }
    if request
        .quality
        .as_deref()
        .is_some_and(|q| !q.eq_ignore_ascii_case("auto"))
    {
        return Err("Gemini image models have no quality setting; use resolution".into());
    }
    if request.output_format.is_some() {
        return Err("Gemini image models choose the output format; output_format is not supported (set it through provider_options where a model documents one)".into());
    }
    if request.images.len() > MAX_INPUT_IMAGES {
        return Err(format!(
            "Gemini image models take at most {MAX_INPUT_IMAGES} input images"
        ));
    }

    let mut parts = vec![json!({"text": request.prompt})];
    for image in &request.images {
        parts.push(media_part(image, "image/png")?);
    }

    let mut image_config = Map::new();
    if let Some(ratio) = request.aspect_ratio.as_deref() {
        let ratio = ratio.trim();
        if !IMAGE_ASPECT_RATIOS.contains(&ratio) {
            return Err(format!("unsupported Gemini image aspect ratio `{ratio}`"));
        }
        image_config.insert("aspectRatio".into(), json!(ratio));
    }
    if let Some(size) = request.resolution.as_deref() {
        image_config.insert("imageSize".into(), json!(normalize_image_size(size)?));
    }

    let mut generation_config = Map::new();
    generation_config.insert("responseModalities".into(), json!(["TEXT", "IMAGE"]));
    if !image_config.is_empty() {
        generation_config.insert("imageConfig".into(), Value::Object(image_config));
    }
    if let Some(seed) = request.seed {
        if crate::contracts::models::model_uses_fixed_sampling(&request.model) {
            return Err(format!("{} does not accept a seed", request.model));
        }
        generation_config.insert("seed".into(), json!(seed));
    }

    let mut body = json!({
        "contents": [{"role": "user", "parts": parts}],
        "generationConfig": generation_config,
    });
    merge_generate_content_options(&mut body, request.provider_options.as_ref())?;
    Ok(body)
}

/// Map a `generateContent` response to generated images. Thought images
/// (interim drafts some models return with `thought: true`) are skipped.
/// A prompt block is an error; a safety stop with no images is reported as
/// `filtered: 1`.
pub fn map_vertex_image_response(
    response: VertexMediaResponse,
) -> Result<GaiseImageResponse, String> {
    if let Some(reason) = response
        .prompt_feedback
        .as_ref()
        .and_then(|f| f.block_reason.as_deref())
    {
        return Err(format!("Gemini blocked the image prompt: {reason}"));
    }
    let mut images = Vec::new();
    let mut text = String::new();
    for part in response.answer_parts() {
        if let Some(blob) = &part.inline_data {
            let data = base64::engine::general_purpose::STANDARD
                .decode(&blob.data)
                .map_err(|e| format!("Gemini image data is not valid base64: {e}"))?;
            images.push(GaiseGeneratedMedia {
                data: Some(data),
                mime_type: blob.mime_type.clone(),
                ..Default::default()
            });
        } else if let Some(t) = &part.text {
            text.push_str(t);
        }
    }
    let filtered = (images.is_empty() && response.safety_stopped()).then_some(1);
    Ok(GaiseImageResponse {
        images,
        text: (!text.is_empty()).then_some(text),
        filtered,
        usage: response
            .usage_metadata
            .as_ref()
            .map(crate::contracts::models::map_google_usage),
        external_id: response.response_id,
    })
}

// ── Transcription ─────────────────────────────────────────────────────

/// MIME type for a declared [`gaise_core::contracts::GaiseAudioFormat`].
pub fn audio_format_mime(
    format: &gaise_core::contracts::GaiseAudioFormat,
) -> Result<String, String> {
    let codec = format.codec.trim().to_ascii_lowercase();
    Ok(match codec.as_str() {
        "pcm" | "pcm16" | "l16" | "linear16" => {
            let rate = format
                .sample_rate
                .ok_or("raw PCM audio needs a sample_rate")?;
            format!("audio/pcm;rate={rate}")
        }
        "wav" => "audio/wav".into(),
        "mp3" => "audio/mp3".into(),
        "ogg" | "opus" => "audio/ogg".into(),
        "flac" => "audio/flac".into(),
        "aac" => "audio/aac".into(),
        "aiff" => "audio/aiff".into(),
        "webm" => "audio/webm".into(),
        "m4a" | "mp4" => "audio/mp4".into(),
        other => {
            return Err(format!(
                "Gemini transcription does not take `{other}` audio"
            ));
        }
    })
}

/// Build the `generateContent` body for `gemini-3.5-transcribe`: one audio
/// part and `generationConfig.audioTranscriptionConfig`. The combinations the
/// API rejects (SMART with diarization or word timestamps; custom vocabulary
/// with word timestamps) and options it has no field for are errors. Unlike
/// the Gemini API, custom vocabulary with diarization is allowed: the Vertex
/// model card's own example combines them.
pub fn vertex_transcription_request(request: &GaiseTranscriptionRequest) -> Result<Value, String> {
    request.validate()?;
    if request.prompt.is_some() {
        return Err(
            "Gemini transcription takes no free-text prompt; use keyterms (customVocabulary)"
                .into(),
        );
    }
    if request.max_speakers.is_some() {
        return Err("Gemini transcription does not take max_speakers".into());
    }
    if request.multichannel {
        return Err("Gemini transcription does not transcribe channels separately".into());
    }
    if request.temperature.is_some() {
        return Err("Gemini transcription does not take a temperature".into());
    }
    let word_timestamps = match request.timestamps {
        None | Some(GaiseTimestampGranularity::None) | Some(GaiseTimestampGranularity::Segment) => {
            false
        }
        Some(GaiseTimestampGranularity::Word) => true,
        Some(GaiseTimestampGranularity::Character) => {
            return Err("Gemini transcription reports word timestamps, not character ones".into());
        }
    };
    let smart = request.mode == Some(GaiseTranscriptionMode::Smart);
    let has_vocabulary = !request.keyterms.is_empty();
    if smart && (request.diarize || word_timestamps) {
        return Err(
            "Gemini SMART transcription cannot be combined with diarization or word timestamps"
                .into(),
        );
    }
    if has_vocabulary && word_timestamps {
        return Err(
            "Gemini custom vocabulary (keyterms) cannot be combined with word timestamps".into(),
        );
    }

    let audio = request
        .audio
        .as_ref()
        .ok_or("transcription request needs audio")?;
    let audio = match (&request.format, audio) {
        (
            Some(format),
            GaiseMediaSource::Bytes {
                data,
                mime_type: None,
            },
        ) => GaiseMediaSource::Bytes {
            data: data.clone(),
            mime_type: Some(audio_format_mime(format)?),
        },
        (
            Some(format),
            GaiseMediaSource::Url {
                url,
                mime_type: None,
            },
        ) => GaiseMediaSource::Url {
            url: url.clone(),
            mime_type: Some(audio_format_mime(format)?),
        },
        _ => audio.clone(),
    };

    let mut config = Map::new();
    if !request.languages.is_empty() {
        config.insert("languageCodes".into(), json!(request.languages));
    }
    if has_vocabulary {
        config.insert("customVocabulary".into(), json!(request.keyterms));
    }
    if word_timestamps {
        config.insert("wordTimestamp".into(), json!(true));
    }
    if request.diarize {
        config.insert("diarization".into(), json!(true));
    }
    if let Some(mode) = request.mode {
        let mode = match mode {
            GaiseTranscriptionMode::Verbatim => "VERBATIM",
            GaiseTranscriptionMode::Smart => "SMART",
        };
        config.insert("mode".into(), json!(mode));
    }

    let mut body = json!({
        "contents": [{"role": "user", "parts": [media_part(&audio, "audio/wav")?]}],
        "generationConfig": {"audioTranscriptionConfig": config},
    });
    merge_generate_content_options(&mut body, request.provider_options.as_ref())?;
    Ok(body)
}

/// Map a transcription response. Each `audioTranscription` part is one
/// segment (one per speaker turn when diarizing); its words carry the
/// segment's speaker. Segments are only returned when they carry a speaker
/// or timing. Without `audioTranscription` parts the text parts are used.
pub fn map_vertex_transcription_response(
    response: VertexMediaResponse,
) -> Result<GaiseTranscriptionResponse, String> {
    if let Some(reason) = response
        .prompt_feedback
        .as_ref()
        .and_then(|f| f.block_reason.as_deref())
    {
        return Err(format!(
            "Gemini blocked the transcription request: {reason}"
        ));
    }
    let mut segments = Vec::new();
    let mut words = Vec::new();
    let mut plain_text = String::new();
    let mut detailed = false;
    for part in response.answer_parts() {
        match &part.audio_transcription {
            Some(tx) => {
                let text = if tx.text.is_empty() {
                    part.text.clone().unwrap_or_default()
                } else {
                    tx.text.clone()
                };
                let speaker = tx.speaker_label.clone().filter(|s| !s.is_empty());
                let segment_words: Vec<GaiseTranscriptWord> = tx
                    .words
                    .iter()
                    .map(|w| GaiseTranscriptWord {
                        text: w.word.clone(),
                        start: w.start_offset.as_deref().and_then(parse_duration_seconds),
                        end: w.end_offset.as_deref().and_then(parse_duration_seconds),
                        speaker: speaker.clone(),
                        ..Default::default()
                    })
                    .collect();
                detailed |= speaker.is_some() || !segment_words.is_empty();
                segments.push(GaiseTranscriptSegment {
                    text,
                    start: segment_words.first().and_then(|w| w.start),
                    end: segment_words.last().and_then(|w| w.end),
                    speaker,
                    channel: None,
                });
                words.extend(segment_words);
            }
            None => {
                if let Some(t) = &part.text {
                    plain_text.push_str(t);
                }
            }
        }
    }
    let text = if segments.is_empty() {
        plain_text
    } else {
        let separator = if segments.iter().any(|s| s.speaker.is_some()) {
            "\n"
        } else {
            " "
        };
        segments
            .iter()
            .map(|s| s.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(separator)
    };
    if text.is_empty() && response.safety_stopped() {
        return Err("Gemini stopped the transcription for safety reasons".into());
    }
    Ok(GaiseTranscriptionResponse {
        text,
        language: None,
        duration_seconds: None,
        segments: if detailed { segments } else { Vec::new() },
        words,
        channels: Vec::new(),
        usage: response
            .usage_metadata
            .as_ref()
            .map(crate::contracts::models::map_google_usage),
        external_id: response.response_id,
    })
}

// ── Video (Veo) ───────────────────────────────────────────────────────

/// Most input reference images Veo 3.1 takes (`asset` references).
pub const MAX_VIDEO_REFERENCE_IMAGES: usize = 3;
/// Most videos one Veo 3.1 request returns.
pub const MAX_VIDEO_SAMPLES: u32 = 4;

/// A Veo media object: `{bytesBase64Encoded | gcsUri, mimeType}`. Inline
/// bytes and base64 `data:` URLs are sent inline; `gs://` URIs by
/// reference. Other URLs are rejected because Veo does not fetch them.
pub fn veo_media(source: &GaiseMediaSource, default_mime: &str) -> Result<Value, String> {
    match source {
        GaiseMediaSource::Bytes { data, mime_type } => {
            if data.is_empty() {
                return Err("media source bytes are empty".into());
            }
            let mime = mime_type
                .clone()
                .or_else(|| sniff_mime(data).map(str::to_string))
                .unwrap_or_else(|| default_mime.to_string());
            Ok(json!({
                "bytesBase64Encoded": base64::engine::general_purpose::STANDARD.encode(data),
                "mimeType": mime,
            }))
        }
        GaiseMediaSource::Url { url, mime_type } => {
            if let Some((mime, data)) = decode_data_url(url) {
                return veo_media(
                    &GaiseMediaSource::Bytes {
                        data,
                        mime_type: mime_type.clone().or(Some(mime).filter(|m| !m.is_empty())),
                    },
                    default_mime,
                );
            }
            if !url.starts_with("gs://") {
                return Err(format!(
                    "Veo on Vertex AI takes inline bytes or gs:// URIs, not `{url}`"
                ));
            }
            let mime = mime_type
                .clone()
                .or_else(|| mime_from_url(url).map(str::to_string))
                .unwrap_or_else(|| default_mime.to_string());
            Ok(json!({"gcsUri": url, "mimeType": mime}))
        }
    }
}

/// Build the `predictLongRunning` body for a Veo request:
/// `{instances: [{prompt, image, lastFrame, referenceImages, video}],
/// parameters: {sampleCount, durationSeconds, aspectRatio, resolution,
/// negativePrompt, generateAudio, seed, storageUri}}`. Reference images are
/// sent as `asset` references. Combinations Veo documents as invalid are
/// errors.
pub fn vertex_video_request(request: &GaiseVideoRequest) -> Result<Value, String> {
    request.validate()?;
    let has_prompt = request
        .prompt
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty());
    if request.image.is_some() && request.video.is_some() {
        return Err("Veo takes either a first-frame image or an input video, not both".into());
    }
    if request.last_frame.is_some() && request.image.is_none() {
        return Err("Veo needs a first-frame image when a last frame is given".into());
    }
    if !request.reference_images.is_empty() {
        if request.image.is_some() || request.video.is_some() || request.last_frame.is_some() {
            return Err(
                "Veo reference images cannot be combined with an image, last frame, or video"
                    .into(),
            );
        }
        if !has_prompt {
            return Err("Veo reference images need a prompt".into());
        }
        if request.reference_images.len() > MAX_VIDEO_REFERENCE_IMAGES {
            return Err(format!(
                "Veo takes at most {MAX_VIDEO_REFERENCE_IMAGES} reference images"
            ));
        }
    }
    if let Some(n) = request.n
        && !(1..=MAX_VIDEO_SAMPLES).contains(&n)
    {
        return Err(format!(
            "Veo returns 1 to {MAX_VIDEO_SAMPLES} videos per request"
        ));
    }
    if let Some(uri) = request.output_uri.as_deref()
        && !uri.starts_with("gs://")
    {
        return Err("Veo on Vertex AI writes to a gs:// output_uri".into());
    }

    let mut instance = Map::new();
    if let Some(prompt) = request.prompt.as_deref().filter(|p| !p.trim().is_empty()) {
        instance.insert("prompt".into(), json!(prompt));
    }
    if let Some(image) = &request.image {
        instance.insert("image".into(), veo_media(image, "image/png")?);
    }
    if let Some(last) = &request.last_frame {
        instance.insert("lastFrame".into(), veo_media(last, "image/png")?);
    }
    if !request.reference_images.is_empty() {
        let references = request
            .reference_images
            .iter()
            .map(|r| Ok(json!({"image": veo_media(r, "image/png")?, "referenceType": "asset"})))
            .collect::<Result<Vec<_>, String>>()?;
        instance.insert("referenceImages".into(), Value::Array(references));
    }
    if let Some(video) = &request.video {
        instance.insert("video".into(), veo_media(video, "video/mp4")?);
    }

    let mut parameters = Map::new();
    if let Some(n) = request.n {
        parameters.insert("sampleCount".into(), json!(n));
    }
    if let Some(seconds) = request.duration_seconds {
        parameters.insert("durationSeconds".into(), json!(seconds));
    }
    if let Some(ratio) = request.aspect_ratio.as_deref() {
        parameters.insert("aspectRatio".into(), json!(ratio.trim()));
    }
    if let Some(resolution) = request.resolution.as_deref() {
        parameters.insert(
            "resolution".into(),
            json!(resolution.trim().to_ascii_lowercase()),
        );
    }
    if let Some(negative) = request.negative_prompt.as_deref() {
        parameters.insert("negativePrompt".into(), json!(negative));
    }
    if let Some(audio) = request.generate_audio {
        parameters.insert("generateAudio".into(), json!(audio));
    }
    if let Some(seed) = request.seed {
        parameters.insert("seed".into(), json!(seed));
    }
    if let Some(uri) = request.output_uri.as_deref() {
        parameters.insert("storageUri".into(), json!(uri));
    }

    if let Some(options) = request.provider_options.as_ref() {
        let Value::Object(options) = options else {
            return Err("provider_options must be a JSON object".into());
        };
        for (key, value) in options {
            if key == "instance" {
                let Value::Object(fields) = value else {
                    return Err("provider_options.instance must be a JSON object".into());
                };
                for (k, v) in fields {
                    instance.insert(k.clone(), v.clone());
                }
            } else {
                parameters.insert(key.clone(), value.clone());
            }
        }
    }

    let mut body = json!({"instances": [Value::Object(instance)]});
    if !parameters.is_empty() {
        body["parameters"] = Value::Object(parameters);
    }
    Ok(body)
}

/// `fetchPredictOperation` body for a job id (the operation name).
pub fn vertex_video_poll_request(operation_name: &str) -> Value {
    json!({"operationName": operation_name})
}

/// Model id inside an operation name
/// (`projects/p/locations/l/publishers/google/models/{model}/operations/{id}`).
pub fn model_from_operation_name(name: &str) -> Option<&str> {
    let rest = name.split_once("/models/")?.1;
    let model = rest.split_once("/operations/").map_or(rest, |(m, _)| m);
    (!model.is_empty()).then_some(model)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexVideoOperation {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub error: Option<VertexOperationError>,
    #[serde(default)]
    pub response: Option<VertexVideoResult>,
}

#[derive(Debug, Default, Deserialize)]
pub struct VertexOperationError {
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexVideoResult {
    #[serde(default)]
    pub rai_media_filtered_count: Option<u32>,
    #[serde(default)]
    pub rai_media_filtered_reasons: Vec<String>,
    #[serde(default)]
    pub videos: Vec<VertexGeneratedVideo>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexGeneratedVideo {
    #[serde(default)]
    pub gcs_uri: Option<String>,
    #[serde(default)]
    pub bytes_base64_encoded: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
}

/// Map the `predictLongRunning` reply (`{name}`) to a queued job whose id is
/// the operation name, verbatim.
pub fn map_vertex_video_start(
    model: &str,
    operation: VertexVideoOperation,
) -> Result<GaiseVideoJob, String> {
    if operation.name.as_deref().is_none_or(str::is_empty) {
        return Err("Veo predictLongRunning response has no operation name".into());
    }
    let mut job = map_vertex_video_operation(model, operation)?;
    if !job.status.is_terminal() {
        job.status = GaiseJobStatus::Queued;
    }
    Ok(job)
}

/// Map a `fetchPredictOperation` reply. Not done → `running`; an operation
/// error → `failed`; done → `completed` with the videos (`gcsUri` as `url`,
/// inline bytes as `data`) and `raiMediaFilteredCount` as `filtered`. A done
/// operation whose every video was filtered is `failed`. `model` falls back
/// to the one named in the operation.
pub fn map_vertex_video_operation(
    model: &str,
    operation: VertexVideoOperation,
) -> Result<GaiseVideoJob, String> {
    let id = operation.name.clone().unwrap_or_default();
    let model = if model.trim().is_empty() {
        model_from_operation_name(&id)
            .unwrap_or_default()
            .to_string()
    } else {
        model.to_string()
    };
    let mut job = GaiseVideoJob {
        id,
        model,
        status: GaiseJobStatus::Running,
        ..Default::default()
    };
    if let Some(error) = operation.error {
        job.status = GaiseJobStatus::Failed;
        job.error = Some(match (error.code, error.message) {
            (Some(code), Some(message)) => format!("{code}: {message}"),
            (None, Some(message)) => message,
            (Some(code), None) => format!("operation failed with code {code}"),
            (None, None) => "operation failed".into(),
        });
        return Ok(job);
    }
    if !operation.done {
        return Ok(job);
    }
    let result = operation.response.unwrap_or_default();
    for video in result.videos {
        let data = video
            .bytes_base64_encoded
            .as_deref()
            .map(|b| base64::engine::general_purpose::STANDARD.decode(b))
            .transpose()
            .map_err(|e| format!("Veo video bytes are not valid base64: {e}"))?;
        job.videos.push(GaiseGeneratedMedia {
            data,
            url: video.gcs_uri,
            mime_type: video.mime_type.or_else(|| Some("video/mp4".into())),
            ..Default::default()
        });
    }
    job.filtered = result.rai_media_filtered_count.filter(|n| *n > 0);
    if job.videos.is_empty() {
        job.status = GaiseJobStatus::Failed;
        job.error = Some(if job.filtered.is_some() {
            let reasons = result.rai_media_filtered_reasons.join("; ");
            if reasons.is_empty() {
                "every generated video was filtered by Responsible AI".into()
            } else {
                format!("every generated video was filtered by Responsible AI: {reasons}")
            }
        } else {
            "the operation finished without videos".into()
        });
    } else {
        job.status = GaiseJobStatus::Completed;
        job.progress = Some(100);
    }
    Ok(job)
}
