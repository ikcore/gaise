# gaise-provider-anthropic

[![crates.io](https://img.shields.io/crates/v/gaise-provider-anthropic.svg)](https://crates.io/crates/gaise-provider-anthropic)
[![docs.rs](https://docs.rs/gaise-provider-anthropic/badge.svg)](https://docs.rs/gaise-provider-anthropic)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)

Anthropic Claude provider for [GAISe](https://crates.io/crates/gaise) — implements the `GaiseClient` trait using the Anthropic Messages API.

## Supported Features

- Text and multimodal (image) instruct requests
- Streaming via SSE
- System message extraction (moved to top-level `system` field)
- Function calling / tool use (`tool_use` / `tool_result` content blocks)
- Extended thinking (`thinking.type` with optional `budget_tokens`)
- Claude 4.6 adaptive thinking support

## Usage

```rust
use gaise_core::GaiseClient;
use gaise_core::contracts::*;
use gaise_provider_anthropic::anthropic_client::GaiseClientAnthropic;

let client = GaiseClientAnthropic::new(
    "https://api.anthropic.com/v1".to_string(),
    "sk-ant-your-api-key".to_string(),
);

let request = GaiseInstructRequest {
    model: "claude-sonnet-4-6".to_string(),
    input: OneOrMany::One(GaiseMessage {
        role: "user".to_string(),
        content: Some(OneOrMany::One(GaiseContent::Text {
            text: "Hello Claude!".to_string(),
        })),
        ..Default::default()
    }),
    ..Default::default()
};

let response = client.instruct(&request).await?;
```

### With Extended Thinking

```rust
let request = GaiseInstructRequest {
    model: "claude-sonnet-4-6".to_string(),
    generation_config: Some(GaiseGenerationConfig {
        thinking_effort: Some("high".to_string()),
        thinking_tokens: Some(10000),  // optional budget
        max_tokens: Some(16000),
        ..Default::default()
    }),
    // ...
};
```

Maps `thinking_effort` to `thinking.type: "enabled"` and `thinking_tokens` to `budget_tokens`.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_URL` | API base URL (default: `https://api.anthropic.com/v1`) |
| `ANTHROPIC_API_KEY` | Your Anthropic API key |

## Note

Anthropic does not support embeddings. Calling `embeddings()` returns an error.

## Part of [GAISe](https://github.com/ikcore/gaise)

License: AGPL-3.0-only
