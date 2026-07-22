# Provider support

“Supported” below means the current adapter maps the capability on its stated API surface. A provider or model may offer additional features through another endpoint that GAISe does not currently wrap.

## Normal instruct and embeddings

| Capability | OpenAI Chat | Anthropic Messages | Gemini API | Vertex AI | Bedrock | Ollama |
|---|---|---|---|---|---|---|
| Text input/output | Yes | Yes | Yes | Yes | Yes | Yes |
| Image input | Data URL + detail | Native source block | `inlineData` | `inlineData` | Converse image block | `images` field |
| Audio input | `input_audio` where model permits | Explicit unsupported marker | `inlineData` | `inlineData` | Converse audio block where model permits | Explicit unsupported marker |
| PDF/document input | UTF-8 tagged text; binary marker | Native PDF; supported text documents | Inline MIME data/text | Inline MIME data/text | Converse document block + companion text | UTF-8 text fallback |
| Nested content parts | Flattened in order | Flattened in order | Flattened in order | Flattened in order | Flattened in order | Flattened in order |
| Tool calling | Yes, parallel deltas | Yes | Yes, IDs/names/signatures | Yes, IDs/names/signatures | Yes | Model-dependent |
| Multimodal tool result | Do not rely on Chat tool messages | Text/image/document blocks | Text/image/PDF inline parts | Text/image/PDF inline parts | Text/image/document; audio becomes explicit marker | Text; images routed where possible |
| Reasoning control | `reasoning_effort` | Adaptive/manual + effort/display | 2.5 budget, 3.x level | 2.5 budget, 3.x level | Claude/Nova mappings | Boolean or GPT-OSS level |
| Returned reasoning | Reasoning usage; Chat summary content not mapped | Summary/signature and redacted blocks | Thought text/signature | Thought text/signature | Reasoning text/signature/redacted | `thinking` text |
| Generated image output | No; use Images/Responses | No native Messages image output | Yes for image-output models | Yes for image-output models | Parsed where selected API/model returns it | Not normalized as a generated-image API |
| Returned audio/file content | Chat audio output not mapped | No audio output | Inline audio/file mapped | Inline audio/file mapped | Non-stream response blocks mapped where exposed | Not exposed by current mapper |
| Streaming | Buffered SSE text/tools/usage | Buffered SSE text/reasoning/tools/usage | Buffered SSE all parts/tools/usage | Buffered SSE all parts/tools/usage | ConverseStream text/reasoning/tools/images/usage | Buffered NDJSON text/reasoning/tools/usage |
| Embeddings | `text-embedding-3-*` | Unsupported | Batch text embeddings | Prediction text embeddings + metadata | Titan/Cohere text embeddings | `/api/embed` |

## Live / realtime

| Capability | OpenAI Realtime | Gemini Live |
|---|---|---|
| Current example model | `gpt-realtime-2.1` | `gemini-3.1-flash-live-preview` |
| Text input/output | Yes | Yes |
| Audio input/output | 24 kHz PCM common input; configured output voice/format | Realtime audio input and native audio output |
| Still image/video-frame input | PNG/JPEG conversation image | Realtime `video` frame |
| Tools | Calls, cancellations, responses | Calls, cancellations, responses |
| Reasoning | Effort configuration; the current response-usage schema has no reasoning subcounter | Thinking configuration, returned thought event, reasoning usage |
| Input/output transcription | Configurable | Configurable |
| Manual activity | Commit/request response when VAD is disabled | `activityStart`/`activityEnd` when automatic detection is disabled |
| End audio stream | Commit in manual OpenAI mode | `audioStreamEnd` when automatic detection is enabled |
| Clear/cancel | Supported | Explicit unsupported error in the current adapter |
| Usage detail | Input text/image/audio/cache (including cached modalities); output text/audio; separate transcription billing; total | Input/output modality arrays, cache/tool/reasoning, total |

## Usage by provider

| Provider surface | Input counters | Output counters | Request total |
|---|---|---|---|
| OpenAI Chat | Prompt, audio, cached, cache-write | Completion, audio, reasoning, accepted/rejected prediction | Reported |
| OpenAI Realtime | Aggregate + text/image/audio/cache and prefixed transcription usage | Aggregate + text/audio and prefixed transcription usage | Reported separately for turns/transcription |
| Anthropic | Uncached input, effective input, cache read/create and TTL | Output and reasoning | Web fetch/search requests; no aggregate token total |
| Gemini generateContent | Prompt/cache/tool plus modality detail | Candidates, reasoning, modality detail | Reported |
| Gemini Live | Prompt/cache/tool plus modality detail | Response, reasoning, modality detail | Reported |
| Vertex AI generateContent | Prompt/cache/tool plus modality detail | Candidates, reasoning, modality detail | Reported |
| Vertex AI embeddings | Input tokens and billable characters | None | Input token total |
| Bedrock Converse | Input, effective input, cache read/write and TTL | Output | Reported provider total |
| Ollama | Prompt | Completion | Derived exact prompt + completion |

An absent image/audio/text counter means the provider did not return that breakdown. It does not mean the modality cost was zero.

## Provider notes

### OpenAI

The instruct client intentionally uses Chat Completions. It accepts image/audio inputs supported by that endpoint, but native binary `input_file`, hosted image generation, persisted reasoning, pro-mode workflows, hosted tools, and native Chat audio output need a future Responses/Images adapter. GPT-5.5 Pro is non-streaming. The separate Realtime client uses the current GA nested session/audio shape rather than the retired beta format.

### Anthropic

The adapter combines multiple system blocks, supports cache controls on system/messages/tools, and selects adaptive versus manual thinking by model family. `input_tokens` remains Anthropic's uncached counter; `effective_input_tokens` adds cache read/create counts. Audio is not a Messages content type and becomes an explicit marker.

### Gemini API

The adapter uses `generateContent`, `streamGenerateContent`, `batchEmbedContents`, and optional Live WebSockets. Current image-output controls serialize under `generationConfig.responseFormat.image`; the legacy `imageConfig` wire field is not emitted. Tool responses retain function name and ID and can include supported inline media.

### Vertex AI

The payload is similar to Gemini but authentication, endpoints, availability, quotas, and lifecycle are Google Cloud-specific. Vertex embedding response metadata is mapped to usage. Service-tier headers are applied only when configured.

### Bedrock

Converse/ConverseStream provide the common chat path, while Titan/Cohere embeddings use InvokeModel. Exact model IDs and inference profiles vary by region. Standard Bedrock Runtime endpoints are supported; alternative platform endpoints such as Mantle remain deployment/configuration concerns rather than assumed defaults.

### Ollama

Capabilities are tag-dependent and discovered locally. GAISe does not maintain a false central retirement list. Vision, tools, and thinking only work when the installed model implements them, and Ollama does not provide per-modality token usage in its Chat response.
