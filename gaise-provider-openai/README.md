# gaise-provider-openai

[![crates.io](https://img.shields.io/crates/v/gaise-provider-openai.svg)](https://crates.io/crates/gaise-provider-openai)
[![docs.rs](https://docs.rs/gaise-provider-openai/badge.svg)](https://docs.rs/gaise-provider-openai)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)

OpenAI provider for [GAISe](https://crates.io/crates/gaise) — implements the `GaiseClient` trait using the OpenAI Chat Completions and Embeddings APIs.

## Supported Features

- Text and multimodal (image, audio) instruct requests
- Streaming via SSE
- Embeddings (`text-embedding-3-small`, `text-embedding-3-large`)
- Function calling / tool use
- Reasoning (`reasoning_effort` for o3, o4-mini, GPT-5 family)
- `max_completion_tokens` (replaces deprecated `max_tokens`)
- Prompt caching (`prompt_cache_key`)
- **Live / Realtime sessions** (feature = `live`) — bidirectional WebSocket audio + text via the OpenAI Realtime API

## Usage

```rust
use gaise_core::GaiseClient;
use gaise_core::contracts::*;
use gaise_provider_openai::openai_client::GaiseClientOpenAI;

let client = GaiseClientOpenAI::new(
    "https://api.openai.com/v1".to_string(),
    "sk-your-api-key".to_string(),
);

let request = GaiseInstructRequest {
    model: "gpt-4o".to_string(),
    input: OneOrMany::One(GaiseMessage {
        role: "user".to_string(),
        content: Some(OneOrMany::One(GaiseContent::Text {
            text: "Hello from GAISe!".to_string(),
        })),
        ..Default::default()
    }),
    ..Default::default()
};

let response = client.instruct(&request).await?;
```

### With Reasoning

```rust
let request = GaiseInstructRequest {
    model: "o3".to_string(),
    generation_config: Some(GaiseGenerationConfig {
        thinking_effort: Some("high".to_string()),
        max_tokens: Some(32000),
        ..Default::default()
    }),
    // ...
};
```

Maps `thinking_effort` to `reasoning_effort` and `max_tokens` to `max_completion_tokens`.

### Live / Realtime (feature = "live")

Enable the `live` feature to use the OpenAI Realtime API for bidirectional audio and text streaming:

```toml
[dependencies]
gaise-provider-openai = { version = "0.1", features = ["live"] }
```

```rust
use gaise_core::GaiseLiveClient;
use gaise_core::contracts::*;
use gaise_provider_openai::openai_live_client::GaiseClientOpenAILive;

let client = GaiseClientOpenAILive::new(
    "https://api.openai.com".to_string(),
    "sk-your-api-key".to_string(),
);

let config = GaiseLiveConfig {
    model: "gpt-4o-realtime-preview".to_string(),
    voice: Some("alloy".to_string()),
    modalities: vec![GaiseLiveModality::Audio, GaiseLiveModality::Text],
    ..Default::default()
};

let session = client.live_connect(&config).await?;
// session.tx — send audio/text/tool responses
// session.rx — receive audio/text/transcripts/tool calls
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `OPENAI_API_URL` | API base URL (default: `https://api.openai.com/v1`) |
| `OPENAI_API_KEY` | Your OpenAI API key |

## Part of [GAISe](https://github.com/ikcore/gaise)

License: AGPL-3.0-only
