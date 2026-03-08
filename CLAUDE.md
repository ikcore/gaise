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
- **gaise-provider-{ollama,openai,vertexai,bedrock,anthropic}** — Each implements `GaiseClient` using `From` impls to convert between Gaise contracts and provider-specific API types
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

## Conventions

- Use `#[serde(skip_serializing_if = "Option::is_none")]` and `#[serde(default)]` on optional fields. Implement `Default` trait to avoid verbose `None` declarations.
- All provider methods are async (`#[async_trait]`), built on `tokio` and `reqwest`.
- Provider-specific API types live in `src/contracts/models.rs` within each provider crate.

## Environment Variables (for gaise-api)

`OLLAMA_URL`, `VERTEXAI_API_URL`, `VERTEXAI_SA_PATH`, `OPENAI_API_KEY`, `OPENAI_API_URL`, `ANTHROPIC_API_KEY`, `ANTHROPIC_API_URL`, `GAISE_PORT` (default 3000).
