# Common contracts

The public contracts live under `gaise_core::contracts`. Provider-neutral examples normally start with:

```rust
use gaise_core::{GaiseClient, GaiseLiveClient};
use gaise_core::contracts::*;
```

## One or many

`OneOrMany<T>` lets callers use a single value or a vector without changing the surrounding request type. It is used for request messages and message content.

```rust
let one = OneOrMany::One(GaiseMessage::default());
let many = OneOrMany::Many(vec![GaiseMessage::default(), GaiseMessage::default()]);
```

In JSON, the same field accepts either an object or an array.

## Instruct request

```rust
pub struct GaiseInstructRequest {
    pub model: String,
    pub input: OneOrMany<GaiseMessage>,
    pub generation_config: Option<GaiseGenerationConfig>,
    pub tools: Option<Vec<GaiseTool>>,
    pub tool_config: Option<GaiseToolConfig>,
    pub correlation_id: Option<String>,
}
```

Use `provider::model` with `GaiseClientService`; use the provider's raw model ID with a direct provider client.

## Messages

`GaiseMessage` carries:

| Field | Purpose |
|---|---|
| `role` | Usually `system`, `user`, `assistant`, or `tool` |
| `content` | One or many ordered `GaiseContent` values |
| `tool_calls` | Assistant-requested function calls |
| `tool_call_id` | Provider call ID when this message returns a tool result |
| `tool_name` | Function name; required for portable Gemini/Vertex tool responses |

System messages are lifted into a provider's top-level system-instruction shape where required. Multiple system blocks are retained in order.

## Content

| Variant | Fields | Use |
|---|---|---|
| `Text` | `text` | Ordinary prompts and returned text |
| `Image` | `data`, `format` | Image input or provider-generated image output |
| `Audio` | `data`, `format` | Audio input or returned audio where mapped |
| `File` | `data`, `name` | Documents and other named files |
| `Reasoning` | `text`, optional `signature` | Provider-returned thought/reasoning summary |
| `RedactedReasoning` | `data` | Opaque encrypted/redacted reasoning to replay unchanged |
| `Parts` | `parts` | Nest content while retaining order; adapters recursively flatten it |

Byte fields serialize as integer arrays in JSON. Image/audio `format` accepts a MIME type or common shorthand. File MIME type is inferred from the filename extension.

```rust
let content = OneOrMany::Many(vec![
    GaiseContent::Text {
        text: "Compare these inputs".into(),
    },
    GaiseContent::Image {
        data: image_bytes,
        format: Some("image/png".into()),
    },
    GaiseContent::Audio {
        data: audio_bytes,
        format: Some("audio/wav".into()),
    },
    GaiseContent::File {
        data: pdf_bytes,
        name: Some("report.pdf".into()),
    },
]);
```

Never display or edit `RedactedReasoning`. Preserve `Reasoning.signature`, `RedactedReasoning.data`, and a tool call's `thought_signature` when continuing a provider conversation.

## Generation configuration

All fields are optional and adapters omit controls unsupported by the selected model.

| Field | Meaning |
|---|---|
| `temperature`, `top_p`, `top_k` | Sampling controls |
| `max_tokens` | Maximum output/completion budget |
| `thinking_tokens` | Manual reasoning budget, or a level approximation where that is the provider's only interface |
| `thinking_effort` | Provider-neutral effort such as `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, or `max` |
| `include_thoughts` | Ask for summaries/thought parts when the provider can expose them |
| `response_modalities` | Requested outputs such as `TEXT`, `IMAGE`, or `AUDIO` |
| `image_config` | Output `aspect_ratio` and `image_size` |
| `input_image_detail` | OpenAI image detail: `auto`, `low`, `high`, or supported `original` |
| `input_media_resolution` | Gemini/Vertex input resolution: `low`, `medium`, `high`, or full `MEDIA_RESOLUTION_*` |
| `cache_key` | Stable provider cache key/hint |

Newer reasoning models can reject sampling settings. The adapters make model-aware omissions rather than forwarding invalid combinations.

## Tools

`GaiseToolParameter` recursively supports JSON-schema-like `type`, `description`, `properties`, `items`, and `required`. This covers nested objects and arrays.

```rust
use std::collections::BTreeMap;

let tool = GaiseTool {
    name: "lookup".into(),
    description: Some("Look up an item".into()),
    parameters: Some(GaiseToolParameter {
        r#type: Some("object".into()),
        properties: Some(BTreeMap::from([(
            "ids".into(),
            GaiseToolParameter {
                r#type: Some("array".into()),
                items: Some(Box::new(GaiseToolParameter {
                    r#type: Some("string".into()),
                    ..Default::default()
                })),
                ..Default::default()
            },
        )])),
        required: Some(vec!["ids".into()]),
        ..Default::default()
    }),
};
```

Return a tool result as a message with both `tool_call_id` and `tool_name`. Gemini and Vertex can carry text, image/PDF, or other supported inline content inside a function response; Anthropic and Bedrock also retain multimodal tool results.

## Responses and streaming

`GaiseInstructResponse` contains one or more assistant messages, an optional provider `external_id`, and optional `GaiseUsage`.

Streaming emits `GaiseStreamChunk`:

- `Text(String)` for text deltas.
- `Content(GaiseContent)` for complete reasoning/media parts.
- `ToolCall` for indexed, incrementally assembled function calls.
- `Usage(GaiseUsage)` for usage snapshots.

`GaiseStreamAccumulator` preserves response order, coalesces adjacent text/reasoning deltas, assembles parallel indexed tool calls, and replaces repeated usage counters. Usage events are cumulative snapshots, not arithmetic deltas.

## Usage

```rust
pub struct GaiseUsage {
    pub input: Option<HashMap<String, usize>>,
    pub output: Option<HashMap<String, usize>>,
    pub total: Option<HashMap<String, usize>>,
}
```

The three maps are separate by design. `total_tokens` belongs in `total`, never in `output`. Values within a map may overlap and must not all be summed: an aggregate prompt/output count includes its modality, reasoning, and/or cache subsets.

Common detail keys include:

| Side | Counters returned when available |
|---|---|
| Input | `prompt_tokens` or `input_tokens`, `text_tokens`, `image_tokens`, `audio_tokens`, `video_tokens`, `document_tokens`, `cached_tokens`, cache TTL counters, `tool_prompt_tokens` |
| Output | `completion_tokens`, `candidates_tokens`, `response_tokens`, `text_tokens`, `image_tokens`, `audio_tokens`, `reasoning_tokens`, prediction counters |
| Total/request-wide | `total_tokens`, server-tool request counters |

Provider-native aggregate names are retained because their cache semantics differ. `effective_input_tokens` is supplied where Anthropic or Bedrock reports uncached input separately from cache reads/writes. GAISe never derives a text/image/audio count from an undifferentiated aggregate.

OpenAI Realtime input-transcription billing is a separate usage object. Its counters use the `transcription_*` prefix so accumulating them does not overwrite the model turn's own input/output totals. Duration-billed transcription is normalized to integer `transcription_audio_milliseconds`, avoiding a lossy whole-second conversion in the integer counter contract.

## Embeddings

`GaiseEmbeddingsRequest` accepts one or many strings. `GaiseEmbeddingsResponse.output` is a vector of embedding vectors and can carry input/total usage. The common contract is text-oriented even when a provider advertises multimodal embeddings.

## Live contracts

`GaiseLiveConfig` contains the raw or routed model ID, optional system instruction and voice, text/audio output modalities, tools, generation controls, VAD configuration, transcription flags, and a correlation ID.

Client inputs:

| Variant | Meaning |
|---|---|
| `Text` | Realtime text input |
| `Audio` | PCM audio bytes and sample rate |
| `Image` | Still image/video-frame bytes, MIME type, and optional OpenAI detail |
| `ToolResponse` | Call ID, function name, and JSON result |
| `ActivityStart`, `ActivityEnd` | Manual activity boundaries where supported |
| `AudioStreamEnd` | Temporarily end/flush the current audio stream |
| `ClearAudio` | Clear uncommitted input audio where supported |
| `CancelResponse` | Cancel current generation where supported |
| `Close` | End the session |

Server events include session start/end, text, audio, transcripts, returned reasoning, tool calls/cancellations, usage, interruption, turn completion, and explicit errors. An operation unsupported by a provider becomes an error rather than a silent success.
