# Changelog

All notable changes to the GAISe crates are listed here. Every crate in the
workspace shares one version. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/): a new or changed model, a lifecycle
change, or an additive adapter feature is a minor release; a change to a public
contract (removed or renamed items, new required fields, changed serialization)
is a major release.

## [4.3.0] - 2026-10-09

Model audit of every provider, plus OpenAI's Decisions API (public beta since
2026-10-06), image input for decisions, and multimodal embeddings for
`gemini-embedding-2`. Every change is additive.

### Added
- OpenAI Decisions API: `GaiseClient::decision` on `openai::gpt-6-luna` sends
  `POST /v1/decisions`. Questions map to named `predicate` / `choice` / `score`
  questions, answers are matched back by name, and a refused question fails the
  call. New `gaise_provider_openai::decisions` module (`decision_request`,
  `decision_request_with_images`, `map_decision_answers`, `map_decision_usage`).
- `GaiseClient::decision_with_images` (default: unsupported unless the image list
  is empty), implemented for OpenAI (base64 data URLs, up to 128 images) and
  Ollama (`images` on `/v1/systemone` for Clef and Clef Flash), and forwarded by
  the router. `POST /v1/decision` accepts an optional `images` array.
- `GaiseContentEmbeddingsRequest` and `GaiseClient::embed_contents`: one embedding
  per `GaiseContent` item (a `parts` item is one aggregated embedding). Gemini
  sends images, audio, video (`File` named `*.mp4` / `*.mov`), and PDFs to
  `gemini-embedding-2`; text-only requests fall back to `embeddings` on every
  client. New route `POST /v1/embeddings/contents`.
- Anthropic: `claude-haiku-5-5`. Bedrock: `anthropic.claude-haiku-5-5`,
  `zai.glm-5.3`, and `twelvelabs.pegasus-1-5-v1:0` (listed only).
- Gemini API and Vertex AI: `gemini-nano-banana-2.1`.
- Ollama: `clef`, `clef-flash`, and `laya` decision models, `mistral-large-4`
  (cloud), and `embeddinggemma-2`. ElevenLabs: `scribe_v2_medical` (listed only).

### Changed
- `claude-haiku-5*` ids (Anthropic and Bedrock) use the Opus 5 / Sonnet 5 rules:
  adaptive thinking only, five effort levels, thinking can be turned off, and no
  sampling parameters. Before this release they fell through to manual budgets.
- `gemini-nano-banana-*` ids (Gemini and Vertex) use `thinkingLevel` with MINIMAL,
  MEDIUM, and HIGH and never receive sampling parameters, instead of the
  Gemini 2.5 `thinkingBudget` path.
- `gpt-6-luna` gains the `decision` capability and operation.
- Lifecycle: `gpt-5.1` and `gpt-5.4-nano` deprecated (shutdown 2027-04-01);
  `gpt-image-1` and `gpt-image-1.5` now point to GPT Image 2.5; `claude-haiku-4-5`
  is Legacy; Gemini API `gemini-3.1-flash-image` deprecated, and new shutdown
  dates for the 3.1 Flash Live, 2.5 native audio, 3.1 Flash TTS, Omni Flash
  previews, and 2.5 Flash Image; Vertex `gemini-3.6-flash` (2026-11-19) and
  `gemini-3.7-flash` (2027-01-28) retirement dates; Bedrock Claude Sonnet 4.5
  and Llama 4 Maverick are Legacy, and the Haiku 4.5, Opus 4.5, and Sonnet 5.5
  notes are corrected.

## [4.2.0] - 2026-10-01

Model audit of every provider. Gemini 4 Argon was announced on 2026-09-30 but
has no API model id yet, so it has no registry row; Gemini 4 ids get the Gemini 3
request rules in advance.

### Added
- Gemini API: `gemini-3.8-live`, `gemini-3.8-live-extended-thinking`,
  `gemini-3.8-flash-tts`, and `gemini-3.8-flash-lite-tts`.
- Vertex AI: `gemini-3.8-flash-cyber` (allowlist) and `gemini-3.8-live` (listed
  only; the Vertex crate has no Live transport).
- Bedrock: `openai.gpt-6.1-sol`, `openai.gpt-6-sol`, `openai.gpt-6-luna`,
  `moonshotai.kimi-k3`, and `xai.grok-4.7`.
- ElevenLabs: `eleven_v4` and `eleven_v4_turbo`, served through the
  text-to-dialogue WebSocket.
- `gemini_major_version` (Gemini and Vertex crates) and
  `live_model_rejects_thinking_config` (Gemini crate).
- Ollama listings map the `/api/show` `decision` capability (Ollama 0.35.1) to the
  `decision` operation.

### Changed
- Gemini 4 and later ids (`gemini-<major>` with major 4 or more) use
  `thinkingLevel` without MINIMAL and never receive sampling parameters, instead of
  the Gemini 2.5 `thinkingBudget` path. This is a forward guard until Google
  publishes a Gemini 4 contract.
- The Gemini Live transport no longer sends `thinkingConfig` to `gemini-3.8-live`,
  and clamps MINIMAL to LOW on `gemini-3.8-live-extended-thinking`.
- Replacement models and dates updated for Gemini 3.1 Flash Live, the 2.5 native
  audio and TTS previews, Vertex Gemini 3.6/3.7 Flash and 2.5, and the Vertex
  2.5 Flash Image retirement (now 2027-03-15).
- The Bedrock gpt-oss note reflects AWS's correction: Responses is served on
  bedrock-mantle only.

### Deprecated
- `claude-sonnet-4-5-20250929`: retires on the Claude API on 2026-11-30;
  replacement `claude-sonnet-5-5`.

### Removed
- Marked retired (ids still resolve): Gemini API `gemini-omni-flash-preview`, and
  Bedrock Nova Premier, Nova Sonic v1, Nova Reel v1, and Nova Canvas.

## [4.1.0] - 2026-09-30

### Added
- Ollama decision models: `ollama::nimble` and `ollama::tev1` work through
  `GaiseClient::decision`, using Ollama's `POST /v1/systemone` (Ollama 0.35+).
- Registry rows `nimble:*` and `tev1:*`; Ollama model listings mark them with the
  `decision` operation.
- Shared System One wire helpers in `gaise`: `DecisionWireRequest`,
  `DecisionWireResponse`, `DecisionWireUsage`, `GaiseDecisionRequest::wire`, and
  `GaiseDecisionRequest::check_answers`.

### Changed
- `POST /v1/systemone` is documented as a supported alias of `POST /v1/decision`
  instead of a deprecated one.

## [4.0.1] - 2026-09-30

### Added
- GPT-6 Astra and GPT-6.1 Sol tool calling: tool conversations for these models
  are sent to the OpenAI Responses API (stateless, with reasoning replayed through
  `GaiseContent::Reasoning`). Tool-free requests stay on Chat Completions.

### Changed
- `gpt-6-astra` and `gpt-6.1-sol` are marked tool-capable in the registry.

## [4.0.0] - 2026-09-30

### Changed
- **Breaking:** the typed-decision operation is renamed from `system_one` to
  `decision`: `GaiseClient::decision`, `GaiseDecisionRequest` /
  `GaiseDecisionResponse`, `GaiseOperation::Decision` (serialized `decision`),
  `POST /v1/decision`, and the `decision` registry capability. The old Rust names
  remain as deprecated aliases.
- Opus 5.5 and Sonnet 5.5 use the always-on thinking profile on the Anthropic and
  Bedrock adapters; GPT-6 Sol and Luna follow the GPT-5.6 Chat Completions rules.

### Added
- Models `gpt-6.1-sol`, `gpt-6-sol`, `gpt-6-luna`, `claude-opus-5-5`,
  `claude-sonnet-5-5`, `anthropic.claude-opus-5-5`, and
  `anthropic.claude-sonnet-5-5`.

### Deprecated
- `claude-opus-5` and `claude-sonnet-5` moved to `legacy` (still served).

## [3.0.1]

### Changed
- Documentation for System One and the provider packages.

## [3.0.0]

### Added
- TypeSafe AI provider (`gaise-provider-typesafe`) with Jev typed decisions
  (System One).

## [0.2.3]

### Changed
- Relicensed to MIT OR Apache-2.0. Releases 0.2.2 and earlier remain AGPL.

## [0.2.2] - 2026-09-12

### Added
- Model audit 2026-09-12: GPT-Live 1, GPT-6 Astra general availability, Ollama
  GLM 5.3 and Granite 4.2. Fixed Gemini Live VAD sensitivity values.

## [0.2.1] - 2026-09-04

### Added
- Model audit 2026-09-04: GPT-6 Astra, Claude 5.1, Gemini 3.8, and the Bedrock
  catalog.

## [0.2.0]

### Added
- Model discovery, per-family parameter rules, a shared reasoning vocabulary,
  ElevenLabs speech, standardized embeddings, the model limits matrix
  (`/v1/models/limits`), and the wiki.

[4.3.0]: https://github.com/ikcore/gaise/pull/18
[4.2.0]: https://github.com/ikcore/gaise/pull/17
[4.1.0]: https://github.com/ikcore/gaise/commit/81116ed
[4.0.1]: https://github.com/ikcore/gaise/pull/15
[4.0.0]: https://github.com/ikcore/gaise/pull/14
[3.0.1]: https://github.com/ikcore/gaise/pull/13
[3.0.0]: https://github.com/ikcore/gaise/pull/12
