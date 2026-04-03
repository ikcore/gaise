# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

GAISe (Generative AI Service) is a Rust workspace that abstracts multiple GenAI providers behind a unified interface. A single `GaiseClient` trait defines `instruct`, `instruct_stream`, and `embeddings` — each provider crate implements this trait by translating to/from its native API format.

## Build & Test Commands

```bash
cargo build                        # Build entire workspace
cargo test                         # Run all tests
cargo test -p gaise-provider-ollama  # Run tests for a single crate
cargo run -p gaise-api             # Start the Axum HTTP server (default port 3000)
cargo clippy                       # Lint
cargo fmt --check                  # Check formatting
```

## Architecture

### Workspace Crates

- **gaise-core** — `GaiseClient` trait, all shared contracts (request/response models, `OneOrMany<T>`, `GaiseContent` enum, tool definitions), logging trait (`IGaiseLogger`)
- **gaise-client** — `GaiseClientService` router that parses `"provider::model"` strings, lazy-loads provider clients, and delegates calls. Uses feature flags to conditionally compile providers.
- **gaise-provider-{ollama,openai,vertexai,bedrock,anthropic,gemini}** — Each implements `GaiseClient` using `From` impls to convert between Gaise contracts and provider-specific API types
- **gaise-api** — Axum web server exposing `/v1/instruct`, `/v1/instruct/stream` (SSE), `/v1/embeddings`
- **gaise-chatbot** — Sample CLI chatbot

### Key Patterns

**Model routing:** Requests use `"provider::model_name"` format (e.g. `"openai::gpt-4o"`). `GaiseClientService::parse_model()` splits this, strips the prefix, and routes to the correct provider client.

**Request/response translation:** Each provider crate uses `From<&GaiseInstructRequest> for ProviderRequest` impls to convert Gaise types to provider-native types and back. This is where most provider-specific logic lives.

**Content model:** `GaiseContent` is an enum with `Text`, `Image`, `Audio`, `File`, and `Parts` variants. Messages use `OneOrMany<GaiseContent>` for flexible single/multi-content payloads.

**Streaming:** Providers return `Pin<Box<dyn Stream<Item = Result<GaiseInstructStreamResponse>>>>`. The API layer converts these to SSE. `GaiseStreamAccumulator` can collect chunks into a complete `GaiseMessage`.

**Logging:** `IGaiseLogger` trait with `log_request`, `log_response`, `log_stream_chunk`. `ConsoleGaiseLogger` is the default. `GaiseClientService` automatically hooks logging around provider calls using correlation IDs.

### Test Pattern

Tests focus on mapping correctness between Gaise contracts and provider-specific models. Each provider has `tests/mapping_tests.rs` that verifies request/response conversion for text, tools, multi-modal content, and multi-turn conversations.

### Reasoning / Thinking

GAISe abstracts provider-specific reasoning/thinking via two fields on `GaiseGenerationConfig`:

- **`thinking_effort`** `Option<String>` — Controls how much reasoning the model performs. Standardised values: `"low"`, `"medium"`, `"high"`.
- **`thinking_tokens`** `Option<usize>` — Explicit token budget for thinking. Providers that support a numeric budget use this directly; others ignore it.

#### Provider Mapping

| GAISe field | OpenAI | Anthropic | Gemini (2.5) | Gemini (3.x) | Bedrock | Ollama |
|---|---|---|---|---|---|---|
| `thinking_effort` | `reasoning_effort` (`"low"` / `"medium"` / `"high"`) | `thinking.type` → `"enabled"` (or `"adaptive"` for Claude 4.6) | `thinkingConfig.thinkingBudget` (mapped to range) | `thinkingConfig.thinkingLevel` (`"LOW"` / `"MEDIUM"` / `"HIGH"`) | Passed to underlying provider | N/A |
| `thinking_tokens` | N/A (implicit in `max_completion_tokens`) | `thinking.budget_tokens` | `thinkingConfig.thinkingBudget` | N/A (use `thinkingLevel` instead) | Passed to underlying provider | N/A |
| `max_tokens` | `max_completion_tokens` (all models, includes reasoning + output) | `max_tokens` | `maxOutputTokens` | `maxOutputTokens` | `maxTokens` | `num_predict` |

> **Note on OpenAI `max_tokens`:** The legacy `max_tokens` parameter is deprecated across all OpenAI models and **not supported** on reasoning models (o3, o4-mini) or gpt-5.x. GAISe maps `max_tokens` → `max_completion_tokens` for all OpenAI requests. For reasoning models this budget covers both reasoning tokens and visible output — set it high enough (OpenAI recommends ≥ 25,000).

#### Effort-level semantics

| `thinking_effort` | Intent | OpenAI | Anthropic | Gemini 3.x |
|---|---|---|---|---|
| `None` | Provider default / no reasoning | No `reasoning_effort` | No `thinking` block | Model default |
| `"low"` | Quick tasks, minimal overhead | `"low"` | `"enabled"` + small `budget_tokens` | `"LOW"` |
| `"medium"` | Balanced reasoning | `"medium"` | `"enabled"` + moderate `budget_tokens` (or `"adaptive"`) | `"MEDIUM"` |
| `"high"` | Deep reasoning, complex tasks | `"high"` | `"enabled"` + large `budget_tokens` (or `"adaptive"`) | `"HIGH"` |

When `thinking_tokens` is also set alongside `thinking_effort`, providers that support an explicit budget (Anthropic, Gemini 2.5) use it directly. When only `thinking_effort` is set, providers use their own defaults for that effort level.

#### Example request

```json
{
  "model": "openai::o3",
  "input": { "role": "user", "content": { "type": "text", "text": "Prove that √2 is irrational" } },
  "generation_config": {
    "thinking_effort": "high",
    "max_tokens": 32000
  }
}
```

The same request works identically across providers — just change the model string:

| Model string | What happens |
|---|---|
| `openai::o3` | `reasoning_effort: "high"`, `max_completion_tokens: 32000` |
| `anthropic::claude-sonnet-4-6` | `thinking: { type: "adaptive" }`, `max_tokens: 32000` |
| `gemini::gemini-2.5-pro` | `thinkingConfig: { thinkingBudget: -1 }`, `maxOutputTokens: 32000` |
| `gemini::gemini-3-flash-preview` | `thinkingConfig: { thinkingLevel: "HIGH" }`, `maxOutputTokens: 32000` |

#### Models that support reasoning

| Provider | Reasoning models | Non-reasoning models |
|---|---|---|
| **OpenAI** | o3, o4-mini, gpt-5, gpt-5-mini, gpt-5-nano, gpt-5.2, gpt-5.4 | gpt-4o, gpt-4o-mini, gpt-4.1, gpt-4.1-mini, gpt-4.1-nano |
| **Anthropic** | All Claude 3.7+ (Sonnet 3.7, Sonnet/Opus 4, 4.1, 4.5, 4.6) | Haiku 3.5, older models |
| **Gemini** | All 2.5 and 3.x models | 2.0 and older |

## Conventions

- Use `#[serde(skip_serializing_if = "Option::is_none")]` and `#[serde(default)]` on optional fields. Implement `Default` trait to avoid verbose `None` declarations.
- All provider methods are async (`#[async_trait]`), built on `tokio` and `reqwest`.
- Provider-specific API types live in `src/contracts/models.rs` within each provider crate.

## Environment Variables (for gaise-api)

`OLLAMA_URL`, `VERTEXAI_API_URL`, `VERTEXAI_SA_PATH`, `OPENAI_API_KEY`, `OPENAI_API_URL`, `ANTHROPIC_API_KEY`, `ANTHROPIC_API_URL`, `GEMINI_API_KEY`, `GEMINI_API_URL`, `GAISE_PORT` (default 3000).
