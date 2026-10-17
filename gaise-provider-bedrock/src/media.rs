//! Image and video generation on Bedrock: pure request builders and response
//! mappers, kept free of the AWS SDK so they can be tested hermetically.
//!
//! Images go through `InvokeModel` on the Stability AI models
//! (`stability.sd3-5-large-v1:0`, `stability.stable-image-ultra-v1:1`,
//! `stability.stable-image-core-v1:1`; us-west-2). Video goes through
//! `StartAsyncInvoke` / `GetAsyncInvoke` on Luma Ray 2 (`luma.ray-v2:0`;
//! us-west-2). Amazon Nova Canvas and Nova Reel reached end of life on
//! 2026-09-30 and Titan Image Generator is gone, so those ids are rejected
//! with a pointer to the replacements.

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use gaise_core::contracts::{
    GaiseGeneratedMedia, GaiseImageRequest, GaiseImageResponse, GaiseJobStatus, GaiseMediaSource,
    GaiseVideoJob, GaiseVideoRequest,
};
use serde_json::{Map, Value, json};

/// Most images one `generate_images` call will request. Each Stability call
/// returns one image, so `n` is served by `n` sequential calls.
pub const MAX_IMAGES_PER_REQUEST: u32 = 10;

const STABILITY_ASPECT_RATIOS: &[&str] = &[
    "16:9", "1:1", "21:9", "2:3", "3:2", "4:5", "5:4", "9:16", "9:21",
];
const LUMA_ASPECT_RATIOS: &[&str] = &["1:1", "16:9", "9:16", "4:3", "3:4", "21:9", "9:21"];
const PROMPT_MAX_CHARS: usize = 10_000;
const LUMA_PROMPT_MAX_CHARS: usize = 5_000;

/// Which Stability request shape a model id takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StabilityFamily {
    /// Stable Diffusion 3.5 Large: text-to-image and image-to-image (`mode`),
    /// png/jpeg/webp, seed up to 4294967294.
    Sd35,
    /// Stable Image Ultra: text-to-image and image-to-image (`image` +
    /// `strength`, no `mode`), png/jpeg, seed up to 4294967295.
    Ultra,
    /// Stable Image Core: text-to-image only, png/jpeg.
    Core,
}

impl StabilityFamily {
    /// Classify a Bedrock model id (bare or cross-region profile). Unknown
    /// `stability.*` ids use the SD3.5 shape.
    pub fn from_model(model: &str) -> Option<Self> {
        let model = model.to_ascii_lowercase();
        if !model.contains("stability.") {
            return None;
        }
        Some(if model.contains("stable-image-ultra") {
            Self::Ultra
        } else if model.contains("stable-image-core") {
            Self::Core
        } else {
            Self::Sd35
        })
    }

    fn max_seed(self) -> u64 {
        match self {
            Self::Sd35 => 4_294_967_294,
            Self::Ultra | Self::Core => 4_294_967_295,
        }
    }

    fn supports_webp(self) -> bool {
        self == Self::Sd35
    }
}

fn retired_media_hint(model: &str) -> Option<&'static str> {
    let model = model.to_ascii_lowercase();
    if model.contains("nova-canvas") {
        Some(
            "Amazon Nova Canvas reached end of life on Bedrock on 2026-09-30; use a Stability AI model such as stability.sd3-5-large-v1:0 (us-west-2)",
        )
    } else if model.contains("titan-image") {
        Some(
            "Amazon Titan Image Generator is no longer available on Bedrock; use a Stability AI model such as stability.sd3-5-large-v1:0 (us-west-2)",
        )
    } else if model.contains("nova-reel") {
        Some(
            "Amazon Nova Reel reached end of life on Bedrock on 2026-09-30; use luma.ray-v2:0 (us-west-2)",
        )
    } else {
        None
    }
}

/// Merge the top-level keys of `provider_options` into `body`.
pub fn merge_provider_options(
    body: &mut Map<String, Value>,
    options: Option<&Value>,
) -> Result<(), String> {
    match options {
        None | Some(Value::Null) => Ok(()),
        Some(Value::Object(map)) => {
            for (key, value) in map {
                body.insert(key.clone(), value.clone());
            }
            Ok(())
        }
        Some(_) => Err("provider_options must be a JSON object".into()),
    }
}

/// Inline image bytes as base64 plus their MIME type. Bedrock's Stability and
/// Luma inputs take base64 only, so `data:` URLs are decoded and other URLs
/// are rejected.
pub fn inline_image(source: &GaiseMediaSource) -> Result<(String, Option<String>), String> {
    match source {
        GaiseMediaSource::Bytes { data, mime_type } => {
            if data.is_empty() {
                return Err("input image is empty".into());
            }
            let mime = mime_type.clone().or_else(|| sniff_image_mime(data));
            Ok((BASE64_STANDARD.encode(data), mime))
        }
        GaiseMediaSource::Url { url, mime_type } => {
            let rest = url.strip_prefix("data:").ok_or_else(|| {
                "Bedrock image and video models take inline image bytes; download the URL and pass GaiseMediaSource::Bytes".to_string()
            })?;
            let (meta, payload) = rest
                .split_once(',')
                .ok_or("malformed data URL for input image")?;
            let meta_mime = meta.strip_suffix(";base64").ok_or(
                "data URL input images must be base64-encoded (data:<mime>;base64,<data>)",
            )?;
            BASE64_STANDARD
                .decode(payload)
                .map_err(|e| format!("invalid base64 in data URL: {e}"))?;
            let mime = mime_type
                .clone()
                .or_else(|| (!meta_mime.is_empty()).then(|| meta_mime.to_string()));
            Ok((payload.to_string(), mime))
        }
    }
}

fn sniff_image_mime(data: &[u8]) -> Option<String> {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png".into())
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg".into())
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp".into())
    } else {
        None
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Stability `output_format` for a GAISe MIME type (or bare extension).
fn stability_output_format(format: &str, family: StabilityFamily) -> Result<&'static str, String> {
    match format.trim().to_ascii_lowercase().as_str() {
        "image/png" | "png" => Ok("png"),
        "image/jpeg" | "image/jpg" | "jpeg" | "jpg" => Ok("jpeg"),
        "image/webp" | "webp" if family.supports_webp() => Ok("webp"),
        other => Err(format!(
            "output_format `{other}` is not supported by this Stability model (png, jpeg{})",
            if family.supports_webp() { ", webp" } else { "" }
        )),
    }
}

/// MIME type of the images a Stability body will return.
pub fn stability_output_mime(body: &Value) -> &'static str {
    match body.get("output_format").and_then(Value::as_str) {
        Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

/// Build the `InvokeModel` body for one Stability image.
///
/// `n` is not part of the body (each call returns one image); callers loop.
/// Masks, more than one input image, `resolution`, and `quality` other than
/// `auto` are rejected because these models cannot honour them. `size`
/// (`WxH`) is accepted only when it reduces to a supported aspect ratio.
/// Image-to-image strength comes from `provider_options.strength` (default
/// 0.35, the Stable Image Ultra default; SD3.5 requires it).
pub fn stability_image_body(request: &GaiseImageRequest) -> Result<Value, String> {
    request.validate()?;
    let family = StabilityFamily::from_model(&request.model).ok_or_else(|| {
        retired_media_hint(&request.model)
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "Bedrock image generation supports the Stability AI models (stability.*); `{}` is not one",
                    request.model
                )
            })
    })?;
    if request.prompt.chars().count() > PROMPT_MAX_CHARS {
        return Err(format!(
            "Stability prompts are limited to {PROMPT_MAX_CHARS} characters"
        ));
    }
    if request.mask.is_some() {
        return Err("Bedrock Stability models do not support inpainting masks".into());
    }
    if request.images.len() > 1 {
        return Err("Bedrock Stability models take at most one input image".into());
    }
    if let Some(resolution) = &request.resolution {
        return Err(format!(
            "resolution `{resolution}` is not supported by Bedrock Stability models; use aspect_ratio"
        ));
    }
    if let Some(quality) = &request.quality
        && !quality.eq_ignore_ascii_case("auto")
    {
        return Err(format!(
            "quality `{quality}` is not supported by Bedrock Stability models"
        ));
    }

    let mut body = Map::new();
    body.insert("prompt".into(), json!(request.prompt));

    let aspect_ratio = match (&request.aspect_ratio, &request.size) {
        (Some(ratio), _) => Some(ratio.trim().to_string()),
        (None, Some(size)) => {
            let (w, h) = size
                .trim()
                .to_ascii_lowercase()
                .split_once('x')
                .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
                .filter(|(w, h)| *w > 0 && *h > 0)
                .ok_or_else(|| format!("size `{size}` is not WIDTHxHEIGHT"))?;
            let g = gcd(w, h);
            Some(format!("{}:{}", w / g, h / g))
        }
        (None, None) => None,
    };
    if let Some(ratio) = &aspect_ratio
        && !STABILITY_ASPECT_RATIOS.contains(&ratio.as_str())
    {
        return Err(format!(
            "aspect ratio `{ratio}` is not supported by Bedrock Stability models ({})",
            STABILITY_ASPECT_RATIOS.join(", ")
        ));
    }

    if let Some(image) = request.images.first() {
        if family == StabilityFamily::Core {
            return Err(
                "Stable Image Core on Bedrock is text-to-image only; use stability.sd3-5-large-v1:0 or stability.stable-image-ultra-v1:1 for image-to-image"
                    .into(),
            );
        }
        if aspect_ratio.is_some() {
            return Err(
                "aspect_ratio applies to text-to-image only; image-to-image keeps the input image's shape"
                    .into(),
            );
        }
        let (data, _) = inline_image(image)?;
        if family == StabilityFamily::Sd35 {
            body.insert("mode".into(), json!("image-to-image"));
        }
        body.insert("image".into(), json!(data));
        body.insert("strength".into(), json!(0.35));
    } else if let Some(ratio) = aspect_ratio {
        body.insert("aspect_ratio".into(), json!(ratio));
    }

    if let Some(negative) = request.negative_prompt.as_deref()
        && !negative.trim().is_empty()
    {
        if negative.chars().count() > PROMPT_MAX_CHARS {
            return Err(format!(
                "negative_prompt is limited to {PROMPT_MAX_CHARS} characters"
            ));
        }
        body.insert("negative_prompt".into(), json!(negative));
    }
    if let Some(seed) = request.seed {
        if seed > family.max_seed() {
            return Err(format!(
                "seed must be between 0 and {} for this model",
                family.max_seed()
            ));
        }
        body.insert("seed".into(), json!(seed));
    }
    if let Some(format) = &request.output_format {
        body.insert(
            "output_format".into(),
            json!(stability_output_format(format, family)?),
        );
    }

    merge_provider_options(&mut body, request.provider_options.as_ref())?;
    Ok(Value::Object(body))
}

/// The body for the `index`-th of `n` calls: a fixed seed is offset per call
/// so `n > 1` does not return `n` identical images.
pub fn stability_body_for_index(body: &Value, index: u32) -> Value {
    let mut body = body.clone();
    if index > 0
        && let Some(seed) = body.get("seed").and_then(Value::as_u64)
        && seed != 0
    {
        body["seed"] = json!(seed.wrapping_add(u64::from(index)) % 4_294_967_295);
    }
    body
}

/// Map one Stability `InvokeModel` response. Filtered outputs (finish reason
/// `Filter reason: …`) are counted in `filtered`; `Inference error` with no
/// image is an error.
pub fn map_stability_image_response(
    response: &Value,
    mime_type: &str,
) -> Result<GaiseImageResponse, String> {
    let reasons: Vec<Option<&str>> = response
        .get("finish_reasons")
        .and_then(Value::as_array)
        .map(|r| r.iter().map(Value::as_str).collect())
        .unwrap_or_default();
    let seeds: Vec<Option<u64>> = response
        .get("seeds")
        .and_then(Value::as_array)
        .map(|s| {
            s.iter()
                .map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                .collect()
        })
        .unwrap_or_default();

    let mut images = Vec::new();
    for (i, image) in response
        .get("images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(b64) = image.as_str() else { continue };
        let data = BASE64_STANDARD
            .decode(b64)
            .map_err(|e| format!("Stability returned invalid base64 image data: {e}"))?;
        images.push(GaiseGeneratedMedia {
            data: Some(data),
            mime_type: Some(mime_type.to_string()),
            seed: seeds.get(i).copied().flatten(),
            ..Default::default()
        });
    }

    let filtered = reasons
        .iter()
        .filter(|r| r.is_some_and(|r| r.starts_with("Filter reason")))
        .count() as u32;
    if images.is_empty()
        && filtered == 0
        && let Some(reason) = reasons.iter().flatten().next()
    {
        return Err(format!("Stability image generation failed: {reason}"));
    }
    if images.is_empty() && filtered == 0 {
        return Err("Stability response contained no images".into());
    }

    Ok(GaiseImageResponse {
        images,
        filtered: (filtered > 0).then_some(filtered),
        ..Default::default()
    })
}

/// Fold per-call responses for `n > 1` into one.
pub fn merge_image_responses(parts: Vec<GaiseImageResponse>) -> GaiseImageResponse {
    let mut merged = GaiseImageResponse::default();
    let mut filtered = 0;
    for part in parts {
        merged.images.extend(part.images);
        filtered += part.filtered.unwrap_or(0);
    }
    merged.filtered = (filtered > 0).then_some(filtered);
    merged
}

/// Validate and return the S3 output prefix for a video job.
pub fn video_output_uri(request: &GaiseVideoRequest) -> Result<String, String> {
    let uri = request.output_uri.as_deref().map(str::trim).unwrap_or("");
    if uri.is_empty() {
        return Err(
            "Bedrock video generation writes to S3: set output_uri to an s3:// prefix in a bucket in the same region"
                .into(),
        );
    }
    if !uri.starts_with("s3://") {
        return Err(format!(
            "Bedrock video output_uri must start with s3:// (got `{uri}`)"
        ));
    }
    Ok(uri.to_string())
}

fn luma_keyframe(source: &GaiseMediaSource) -> Result<Value, String> {
    let (data, mime) = inline_image(source)?;
    let mime = mime.unwrap_or_else(|| "image/jpeg".into());
    if mime != "image/jpeg" && mime != "image/png" {
        return Err(format!("Luma keyframes must be JPEG or PNG (got {mime})"));
    }
    Ok(json!({
        "type": "image",
        "source": {"type": "base64", "media_type": mime, "data": data}
    }))
}

/// Build the Luma Ray 2 `modelInput` for `StartAsyncInvoke`.
///
/// `image` becomes `keyframes.frame0`, `last_frame` `keyframes.frame1`.
/// `duration_seconds` must be 5 or 9; `resolution` 540p or 720p. Reference
/// images, source video, audio, negative prompts, seeds, and `n > 1` are
/// rejected (Ray 2 on Bedrock has none of them). `loop` and other Luma
/// fields go through `provider_options`.
pub fn luma_model_input(request: &GaiseVideoRequest) -> Result<Value, String> {
    request.validate()?;
    if !request.model.to_ascii_lowercase().contains("luma.") {
        return Err(retired_media_hint(&request.model)
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "Bedrock video generation supports Luma Ray (luma.ray-v2:0); `{}` is not supported",
                    request.model
                )
            }));
    }
    let prompt = request
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or("Luma Ray on Bedrock needs a prompt (1-5000 characters)")?;
    if prompt.chars().count() > LUMA_PROMPT_MAX_CHARS {
        return Err(format!(
            "Luma prompts are limited to {LUMA_PROMPT_MAX_CHARS} characters"
        ));
    }
    if !request.reference_images.is_empty() {
        return Err(
            "Luma Ray on Bedrock does not take reference images; use image / last_frame keyframes"
                .into(),
        );
    }
    if request.video.is_some() {
        return Err("Luma Ray on Bedrock cannot extend or edit a source video".into());
    }
    if request.generate_audio == Some(true) {
        return Err("Luma Ray on Bedrock does not generate audio".into());
    }
    if request.n.is_some_and(|n| n > 1) {
        return Err("Luma Ray on Bedrock generates one video per job; start several jobs".into());
    }
    if request.negative_prompt.is_some() {
        return Err("Luma Ray on Bedrock does not accept a negative prompt".into());
    }
    if request.seed.is_some() {
        return Err("Luma Ray on Bedrock does not accept a seed".into());
    }

    let mut input = Map::new();
    input.insert("prompt".into(), json!(prompt));
    if let Some(ratio) = request.aspect_ratio.as_deref().map(str::trim) {
        if !LUMA_ASPECT_RATIOS.contains(&ratio) {
            return Err(format!(
                "aspect ratio `{ratio}` is not supported by Luma Ray ({})",
                LUMA_ASPECT_RATIOS.join(", ")
            ));
        }
        input.insert("aspect_ratio".into(), json!(ratio));
    }
    if let Some(seconds) = request.duration_seconds {
        let duration = match seconds {
            5 => "5s",
            9 => "9s",
            other => {
                return Err(format!(
                    "Luma Ray videos are 5 or 9 seconds long (got {other})"
                ));
            }
        };
        input.insert("duration".into(), json!(duration));
    }
    if let Some(resolution) = request.resolution.as_deref() {
        let resolution = resolution.trim().to_ascii_lowercase();
        if resolution != "540p" && resolution != "720p" {
            return Err(format!(
                "resolution `{resolution}` is not supported by Luma Ray (540p, 720p)"
            ));
        }
        input.insert("resolution".into(), json!(resolution));
    }
    if request.image.is_some() || request.last_frame.is_some() {
        let mut keyframes = Map::new();
        if let Some(image) = &request.image {
            keyframes.insert("frame0".into(), luma_keyframe(image)?);
        }
        if let Some(image) = &request.last_frame {
            keyframes.insert("frame1".into(), luma_keyframe(image)?);
        }
        input.insert("keyframes".into(), Value::Object(keyframes));
    }

    merge_provider_options(&mut input, request.provider_options.as_ref())?;
    Ok(Value::Object(input))
}

/// Map a `GetAsyncInvoke` status string (`InProgress`, `Completed`,
/// `Failed`).
pub fn map_async_invoke_status(status: &str) -> GaiseJobStatus {
    match status {
        "Completed" => GaiseJobStatus::Completed,
        "Failed" => GaiseJobStatus::Failed,
        "InProgress" => GaiseJobStatus::Running,
        _ => GaiseJobStatus::Queued,
    }
}

/// Invocation id: the last path segment of an async-invoke ARN.
pub fn invocation_id(invocation_arn: &str) -> &str {
    invocation_arn.rsplit('/').next().unwrap_or(invocation_arn)
}

/// S3 location of the finished video. Bedrock writes async results into a
/// folder named after the invocation id under the requested prefix, and
/// `GetAsyncInvoke` reports that folder; the video is `output.mp4` inside it.
/// When the reported URI does not already end in the invocation id, the id
/// folder is appended.
pub fn async_video_uri(s3_uri: &str, invocation_arn: &str) -> String {
    let base = s3_uri.trim_end_matches('/');
    let id = invocation_id(invocation_arn);
    if base.ends_with(&format!("/{id}")) {
        format!("{base}/output.mp4")
    } else {
        format!("{base}/{id}/output.mp4")
    }
}

/// Build a [`GaiseVideoJob`] from `GetAsyncInvoke` fields.
pub fn map_async_invoke(
    invocation_arn: &str,
    model: &str,
    status: &str,
    failure_message: Option<&str>,
    output_s3_uri: Option<&str>,
) -> GaiseVideoJob {
    let status = map_async_invoke_status(status);
    let videos = match (status, output_s3_uri) {
        (GaiseJobStatus::Completed, Some(uri)) => vec![GaiseGeneratedMedia {
            url: Some(async_video_uri(uri, invocation_arn)),
            mime_type: Some("video/mp4".into()),
            ..Default::default()
        }],
        _ => Vec::new(),
    };
    let error = (status == GaiseJobStatus::Failed).then(|| {
        failure_message
            .unwrap_or("Bedrock async invocation failed")
            .to_string()
    });
    let filtered = failure_message
        .filter(|m| {
            let m = m.to_ascii_lowercase();
            m.contains("content filter") || m.contains("blocked") || m.contains("moderation")
        })
        .map(|_| 1);
    GaiseVideoJob {
        id: invocation_arn.to_string(),
        model: model.to_string(),
        status,
        videos,
        error,
        filtered,
        ..Default::default()
    }
}
