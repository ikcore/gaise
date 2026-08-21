# CLAUDE.md

This file provides repository guidance for coding agents.

## Project overview

GAISe is a Rust workspace that translates one provider-neutral contract to OpenAI, Anthropic, Gemini, Vertex AI, Amazon Bedrock, Ollama, and ElevenLabs. The core `GaiseClient` trait exposes `instruct`, `instruct_stream`, `embeddings`, and `list_models`; `GaiseSpeechClient` exposes `speech` and `speech_stream`; `GaiseLiveClient` exposes `live_connect`. `gaise-client` routes `provider::model-id` strings to feature-gated adapters.

## Safe build and test commands

```powershell
cargo build --workspace
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

The default suite must remain hermetic. Tests that need credentials, provider APIs, a running Ollama service, or external credential-provider initialization must use `#[ignore]` with a clear reason. Do not run ignored/live tests unless the user explicitly authorizes external traffic and possible cost.

## Workspace layout

- `gaise-core/`: package `gaise`, library crate `gaise_core`; shared contracts, `GaiseClient`, stream accumulator, logging, and the bundled `model-registry.toml` (`gaise_core::registry`).
- `gaise-client/`: feature-gated provider router and environment configuration.
- `gaise-provider-openai/`: Chat Completions, Embeddings, and Realtime.
- `gaise-provider-anthropic/`: Messages API.
- `gaise-provider-gemini/`: Gemini generateContent, Embeddings, and Live.
- `gaise-provider-vertexai/`: Vertex AI generateContent and Embeddings.
- `gaise-provider-bedrock/`: Converse/ConverseStream and InvokeModel embeddings.
- `gaise-provider-ollama/`: local Chat and Embeddings.
- `gaise-provider-elevenlabs/`: text-to-speech, streaming speech, realtime voice WebSocket (`live`), and model listing. No chat or embeddings surface.
- `gaise-api/`: Axum JSON, SSE, and WebSocket server.
- `gaise-chatbot/`: example CLI.
- `wiki/`: the developer wiki — `README.md` index, `api.md`, `sdk.md`, `capabilities.md`, `models.md` (generated from the registry), `flows.md`, `examples.md`, `releasing.md`, and one `vendor-{provider}.md` per adapter. Keep it in step with behaviour changes; every page deep-links to source.

## Core contracts

`GaiseContent` variants are:

- `Text { text }`
- `Image { data, format }`
- `Audio { data, format }`
- `File { data, name }`
- `Reasoning { text, signature }`
- `Parts { parts }`

Messages use `OneOrMany<GaiseContent>`. Provider mappers must recursively flatten `Parts`, preserve content order, normalize MIME types through the helpers in `gaise_content.rs`, and fail or emit an explicit fallback when the destination API cannot represent a modality. Never invent a provider block shape.

Tool-result messages carry `tool_call_id` and optional `tool_name`. Preserve both when the provider returns a call ID: Gemini and Vertex function responses require the name as well as the optional ID, while OpenAI, Anthropic, and Bedrock can resolve by ID alone.

Streaming uses `GaiseStreamChunk::{Text, Content, ToolCall, Usage}`. `Content` carries returned reasoning and generated media. Tool calls and reasoning may include opaque provider signatures; preserve them for later turns. Stream parsers must tolerate JSON/SSE/NDJSON frames split across arbitrary byte chunks.

`GaiseGenerationConfig` includes sampling, output limits, reasoning controls, returned-thought selection, output modalities, image configuration, OpenAI image-input detail, and cache keys. Add common fields only when they have a defensible provider-neutral meaning.

### Per-family parameter rules

Adapters filter every request to what the target model family accepts instead of forwarding invalid combinations; the rules live next to the request builders (`openai_chat_rules`, `claude_family_rules`, `claude_rules` on Bedrock, `thinking_levels_for` / `thinking_budget_for` on Gemini and Vertex) and each provider crate has a `parameter_matrix_tests` file that pins them. When a vendor publishes a new family, add it to the rule table and the matrix test in the same change. Unknown model ids fall back to pass-through profiles. `thinking_effort` is interpreted through `GaiseReasoningEffort` (`gaise_reasoning.rs`): canonical ladder `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra` plus `auto` and fixed aliases; `ultra` means "the highest level this model supports" and is never sent verbatim; unsupported levels snap to the nearest accepted one (ties upward); `none` on a family that cannot disable thinking becomes its lowest level; custom strings pass through. Never add ad hoc string matching for effort in an adapter — parse with `GaiseReasoningEffort` and clamp against the family table, then regenerate `wiki/reasoning.md` (`cargo run -p gaise-client --example reasoning_matrix --all-features`). Manual budgets are raised to provider minimums and kept below `max_tokens`.

### Embeddings

Embeddings are standardised like reasoning. `GaiseEmbeddingTask` (`gaise_embeddings_request.rs`) is the canonical task vocabulary (`document`, `query`, `classification`, `clustering`, `similarity`, `code_query`, `fact_verification`, `question_answering`, aliases, and a `Custom` pass-through for vendor values). Per-model facts live as data in `model-registry.toml` under `[models.embedding]` (`EmbeddingProfile`: `dimension_rule` fixed/range/set, `default_dimensions`, limits, `normalized_output`, `normalizes_truncation`, `task_control`, `single_input`) and are read with `gaise_core::registry::embedding_profile`. Every adapter builds its body through `resolve_embedding(request, profile, provider_default_control)`, which returns the prefixed texts, the wire task value, snapped dimensions, and whether to run `normalize_l2` locally; adapters only do wire shape (`openai_embed_request`, `gemini_embed_request`, `vertex_embed_request`, `embedding_bodies` on Bedrock, `ollama_embed_request`). Never add ad hoc dimension clamps, task string matches, or prefix logic in an adapter — add or fix the registry profile, then regenerate the matrix in `wiki/embeddings.md` (`cargo run -p gaise-client --example embedding_matrix --all-features`). Models without a profile pass `dimensions` through and get the provider default control.

### Connection overrides

Every request type carries `connection: Option<GaiseConnection>` (`api_url`, `api_key`, `region`, `service_account`). `GaiseClientService` resolves clients with `get_client_with(provider, connection)` (and the live/speech equivalents), caching per distinct connection via `GaiseConnection::cache_key`; overrides win field-by-field over `GaiseClientConfig`. Strip `connection` before forwarding to adapters and always log requests through `redact_secrets` — never serialize a request straight into a logger.

### Speech

`GaiseSpeechRequest` / `GaiseSpeechResponse` / `GaiseSpeechChunk` (`gaise_speech.rs`) are the text-to-speech contract. `format` accepts a MIME type or a provider-native format string; adapters resolve it and always report the returned MIME and sample rate. Realtime text-in/audio-out reuses `GaiseLiveClient` (`GaiseLiveInput::Text` → `GaiseLiveEvent::Audio`). A missing voice is an explicit error, never a default pick.

### Model discovery

`GaiseClient::list_models` returns `GaiseModel` records (`gaise_model.rs`). Adapters return bare provider IDs and only what the provider API actually reports, tagged `GaiseMetadataSource::Provider`; name-based inferences are tagged `Heuristic`. `GaiseSupport` is tri-state and empty modality lists mean *unknown* — never invent `Unsupported`. The router (`GaiseClientService`) rewrites IDs to `provider::id`, overlays `gaise_core::registry` (modalities are unioned; operations, flags, and lifecycle are filled only when unknown), filters by operation after enrichment, and reports per-provider failures in `errors` rather than dropping them. `operations` means "which GAISe trait methods can drive this model", not what the vendor advertises. Extra per-model requests (Ollama `/api/show`) must stay behind `include_details`. Every adapter's `list_models` needs a JSON-fixture test of the provider payload mapped to `GaiseModel`.

## Provider-specific boundaries

### OpenAI

The instruct adapter targets Chat Completions, not Responses. Chat message content supports text, image, and supported audio blocks, but not Responses-style `input_file`. UTF-8 files may be represented as tagged text; binary file input must return the explicit Responses limitation. OpenAI image generation, persisted reasoning, pro mode, hosted tools, and native file input need a future Responses adapter.

`max_tokens` maps to `max_completion_tokens`. `reasoning_effort` is sent only to reasoning families and clamped to each family's accepted set; GPT-5.x accept `temperature`/`top_p` only while the effective effort is `none` (GPT-5/mini/nano and the o-series never). Image input uses MIME-aware data URLs and `input_image_detail`; `original` is sent only to GPT-5.4/5.5/5.6 and degrades to `high` elsewhere. Responses-only models (`*-pro`, `gpt-5.6-cyber`, Daybreak) fail fast on `instruct`.

On Chat Completions, GPT-5.6 function-tool requests require
`reasoning_effort: "none"`. Preserve configured reasoning for tool-free
requests, preempt the known incompatibility for that family, and retry only the
matching structured `reasoning_effort` API error for forward compatibility.

### Anthropic

System messages become the top-level system prompt. Prompt caching applies ephemeral cache control at stable boundaries. PDFs and supported text documents become document blocks; unsupported binary/Office input must remain explicit.

Reasoning is model-aware:

- Opus 5, Fable 5, Mythos 5, Opus 4.8/4.7, and Sonnet 5 use adaptive thinking; Fable 5, Mythos 5, and Mythos Preview cannot disable it, Opus 5 only at effort `high` or below.
- Opus/Sonnet 4.6 support adaptive thinking; manual budgets are deprecated there.
- Older compatible Claude models use manual `budget_tokens` (≥ 1024 and < `max_tokens`; the adapter adjusts both). Output ceilings: 64k for the 4.5 family, 128k otherwise.
- Effort maps to `output_config.effort` where supported.
- `include_thoughts` maps to `thinking.display` (`summarized` or `omitted`).
- Newer fixed-sampling models (Opus 5, Opus 4.7/4.8, Sonnet 5, Fable 5, Mythos 5) must not receive non-default temperature/top-p fields; Claude 4.5 must not receive both temperature and top-p.

### Gemini and Vertex AI

Gemini 2.5 uses `thinkingBudget` (Pro 128–32,768, Flash 0–24,576, Flash-Lite 0 or 512–24,576); Gemini 3.x uses `thinkingLevel` with per-family sets (3.7 Flash and the Pro families reject `minimal`; image models accept only `minimal`/`high`; 3 Pro Image only `high`). If reasoning is requested and `include_thoughts` is absent, request summaries by default. Sampling parameters are deprecated on every Gemini 3.x model and are omitted there.

`response_modalities` maps to `responseModalities`; `image_config` maps to the current `responseFormat.image` shape. Returned `inlineData` becomes `GaiseContent::Image`, `Audio`, or `File`. Preserve `thoughtSignature` on reasoning and tool calls.

Gemini API and Vertex AI have separate model lifecycles. Do not copy retirement dates between them; `model-registry.toml` records this distinction.

### Bedrock

Use Converse/ConverseStream for chat and InvokeModel for supported embedding families. Claude families follow the direct-API rules above; `top_k` travels through `additionalModelRequestFields` (Claude `top_k`, Nova `inferenceConfig.topK` ≤ 128); Opus 4.5 effort needs `anthropic_beta: ["effort-2025-11-24"]`; Nova v1 accepts either `temperature` or `topP` with a 5K `maxTokens` cap (Nova 2 Lite 65K). A Bedrock document block must be accompanied by a text block. Model IDs and inference profiles are region-specific; avoid hard-coded global allowlists and rely on AWS lifecycle/discovery APIs (`ListFoundationModels` / `ListInferenceProfiles` via the `aws-sdk-bedrock` control-plane client; the runtime client alone cannot list).

Do not mutate process-wide AWS environment variables in request routing. Pass region configuration into the SDK builder.

### ElevenLabs

Speech only. `eleven_multilingual_v2` rejects `language_code`; `eleven_v3*` realtime uses the text-to-dialogue WebSocket, every other model uses `stream-input`; `speed` is clamped to 0.7–1.2; default voices expire 2026-12-31 so nothing hard-codes a voice id. `/stream/with-timestamps` is newline-delimited JSON parsed defensively (`data:` prefix or bare). The `character-cost` header is the billing signal.

### Ollama

The installed catalog is dynamic (`GET /api/tags`). Thinking is usually a boolean, while GPT-OSS accepts `low`, `medium`, or `high`. Vision and tool support depend on the installed tag. Keep UTF-8 file fallback explicit and never assume every local model accepts images.

## Mapping and test conventions

- Provider request/response types belong in each provider's `src/contracts/` module.
- Prefer pure conversion helpers that can be tested without HTTP.
- Use recursive tool schemas (`properties`, `items`, `required`) and deterministic maps.
- Add tests for both serialized provider JSON and mapped common output.
- Add split-frame fixtures for stream parsers.
- Use `#[serde(skip_serializing_if = "Option::is_none")]` on optional outbound fields so unsupported parameters are omitted rather than serialized as `null`.
- Keep usage counters provider-named inside the common input/output maps.
- Treat `gaise-core/model-registry.toml` as advisory; arbitrary model IDs are intentional for forward compatibility. Its `capabilities` vocabulary is closed (see the file header) and `cargo test -p gaise` fails on unknown terms or unmapped statuses.

## Model references

Model names and lifecycle dates change independently of the crate. Before updating hard-coded model behavior, verify the official sources linked from `gaise-core/model-registry.toml` and update that registry, regenerate `wiki/models.md` (`cargo run -p gaise --example registry_json` feeds the tables), refresh the affected `wiki/vendor-*.md` page, and update the audit report when appropriate.
