# Changelog

All notable changes to the GAISe crates are listed here. Every crate in the
workspace shares one version. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/): a new or changed model, a lifecycle
change, or an additive adapter feature is a minor release; a change to a public
contract (removed or renamed items, new required fields, changed serialization)
is a major release.

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

[4.1.0]: https://github.com/ikcore/gaise/commit/81116ed
[4.0.1]: https://github.com/ikcore/gaise/pull/15
[4.0.0]: https://github.com/ikcore/gaise/pull/14
[3.0.1]: https://github.com/ikcore/gaise/pull/13
[3.0.0]: https://github.com/ikcore/gaise/pull/12
