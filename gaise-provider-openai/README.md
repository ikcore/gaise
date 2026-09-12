# gaise-provider-openai

[![crates.io](https://img.shields.io/crates/v/gaise-provider-openai.svg)](https://crates.io/crates/gaise-provider-openai)
[![docs.rs](https://docs.rs/gaise-provider-openai/badge.svg)](https://docs.rs/gaise-provider-openai)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/ikcore/gaise#license)

OpenAI provider for [GAISe](https://crates.io/crates/gaise) — implements the `GaiseClient` trait using the OpenAI Chat Completions and Embeddings APIs.

## Supported Features

- Text and multimodal (image, audio) instruct requests
- Streaming via SSE
- Embeddings (`text-embedding-3-small`, `text-embedding-3-large`)
- Function calling / tool use
- Reasoning (`reasoning_effort` for current GPT-5 families)
- `max_completion_tokens` (replaces deprecated `max_tokens`)
- Prompt caching (`prompt_cache_key`)
- Usage with separate input/output/total counters plus reported audio, cache, prediction, and reasoning details
- **Live / Realtime sessions** (feature = `live`) — GA WebSocket text, 24 kHz PCM audio, PNG/JPEG image input, tools, reasoning controls, and modality usage

The instruct adapter targets Chat Completions. Native binary file input, hosted image generation, persisted reasoning, and native audio output require other OpenAI API surfaces and are not claimed here.

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
    model: "gpt-6-astra".to_string(),
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
    model: "gpt-6-astra".to_string(),
    generation_config: Some(GaiseGenerationConfig {
        thinking_effort: Some("high".to_string()),
        max_tokens: Some(32000),
        ..Default::default()
    }),
    // ...
};
```

Maps `thinking_effort` to `reasoning_effort` and `max_tokens` to `max_completion_tokens`.

On Chat Completions, the GPT-5.6 family currently requires
`reasoning_effort: "none"` when function tools are present. The adapter applies
that value automatically and retries once when a newer model or alias returns
the same structured compatibility error. Tool-free requests retain the
configured reasoning effort; use the Responses API when reasoning and tools
must be combined.

### Live / Realtime (feature = "live")

Enable the `live` feature to use the OpenAI Realtime API for bidirectional audio and text streaming:

```toml
[dependencies]
gaise-provider-openai = { version = "0.2", features = ["live"] }
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
    model: "gpt-realtime-2.1".to_string(),
    voice: Some("alloy".to_string()),
    modalities: vec![GaiseLiveModality::Audio, GaiseLiveModality::Text],
    ..Default::default()
};

let session = client.live_connect(&config).await?;
// session.tx — send audio/text/images, controls, and tool responses
// session.rx — receive audio/text/transcripts/tools/usage (including reasoning tokens)
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `OPENAI_API_URL` | API base URL (default: `https://api.openai.com/v1`) |
| `OPENAI_API_KEY` | Your OpenAI API key |

## Part of [GAISe](https://github.com/ikcore/gaise)

License: MIT OR Apache-2.0, at your option (see `LICENSE-APACHE` and `LICENSE-MIT`). Versions 0.2.2 and earlier remain AGPL-3.0-only on crates.io.
