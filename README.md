# GAISe (Generative AI Service)

[![crates.io](https://img.shields.io/crates/v/gaise.svg)](https://crates.io/crates/gaise)
[![docs.rs](https://docs.rs/gaise/badge.svg)](https://docs.rs/gaise)
[![Rust](https://img.shields.io/badge/rust-1.91%2B-orange.svg)](https://www.rust-lang.org)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)

GAISe is a Rust-based abstraction service that standardizes requests and responses across multiple Generative AI service providers. Write your application once and switch between OpenAI, Anthropic, Gemini, Vertex AI, Bedrock, and Ollama with a single model string change.

Written by: Ian Knowles<br>
Project page: [BadAI Project Page](https://badai.company/open-source/gaise)

## Features

- **Standardized API**: Unified `GaiseClient` trait with `instruct`, `instruct_stream`, and `embeddings`.
- **Provider Agnostic**: Switch between cloud and local providers by changing `"provider::model"` string.
- **Reasoning / Thinking**: Unified `thinking_effort` and `thinking_tokens` mapped to each provider's native API.
- **Multi-modal Support**: Handle Text, Images, Audio, and Files seamlessly.
- **Tool Calling**: Function calling / tool use across all providers that support it.
- **Streaming**: SSE-based streaming with `GaiseStreamAccumulator` for chunk collection.
- **Async First**: Built on `tokio` and `async-trait`.

## Supported Providers

| Provider | Crate | Models |
|----------|-------|--------|
| **OpenAI** | [`gaise-provider-openai`](https://crates.io/crates/gaise-provider-openai) | GPT-5.x, GPT-4.x, o3, o4-mini |
| **Anthropic** | [`gaise-provider-anthropic`](https://crates.io/crates/gaise-provider-anthropic) | Claude Opus/Sonnet/Haiku 4.x (extended thinking) |
| **Gemini** | [`gaise-provider-gemini`](https://crates.io/crates/gaise-provider-gemini) | Gemini 3.x, 2.5 (thinking, tools, embeddings) |
| **Vertex AI** | [`gaise-provider-vertexai`](https://crates.io/crates/gaise-provider-vertexai) | Gemini models via Google Cloud |
| **Bedrock** | [`gaise-provider-bedrock`](https://crates.io/crates/gaise-provider-bedrock) | Claude, Titan via AWS |
| **Ollama** | [`gaise-provider-ollama`](https://crates.io/crates/gaise-provider-ollama) | Llama, Mistral, Qwen (local) |

## Installation

```toml
[dependencies]
gaise = "0.1"                        # Core trait and contracts
gaise-client = "0.1"                 # Router with all providers (or pick individual ones below)
# gaise-provider-openai = "0.1"      # OpenAI only
# gaise-provider-anthropic = "0.1"   # Anthropic only
# gaise-provider-gemini = "0.1"      # Google Gemini only
# gaise-provider-ollama = "0.1"      # Ollama (local) only
# gaise-provider-vertexai = "0.1"    # Google Vertex AI only
# gaise-provider-bedrock = "0.1"     # AWS Bedrock only
tokio = { version = "1", features = ["full"] }
```

## Quick Start — Provider Router

The simplest way to use GAISe is with `gaise-client`, which routes requests by model string:

```rust
use std::sync::Arc;
use gaise_client::{GaiseClientService, GaiseClientConfig};
use gaise_core::GaiseClient;
use gaise_core::contracts::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = GaiseClientConfig {
        openai_api_key: Some("sk-...".to_string()),
        gemini_api_key: Some("AIza...".to_string()),
        ..Default::default()
    };

    let service = GaiseClientService::new(config);

    let request = GaiseInstructRequest {
        model: "openai::gpt-4o".to_string(),  // or "gemini::gemini-2.5-flash", "anthropic::claude-sonnet-4-6", etc.
        input: OneOrMany::One(GaiseMessage {
            role: "user".to_string(),
            content: Some(OneOrMany::One(GaiseContent::Text {
                text: "What is the capital of France?".to_string(),
            })),
            ..Default::default()
        }),
        ..Default::default()
    };

    let response = service.instruct(&request).await?;
    println!("{:?}", response.output);
    Ok(())
}
```

## Usage Examples

### Direct Provider — Gemini

```rust
use gaise_core::GaiseClient;
use gaise_core::contracts::*;
use gaise_provider_gemini::gemini_client::GaiseClientGemini;

let client = GaiseClientGemini::new(
    "https://generativelanguage.googleapis.com/v1beta".to_string(),
    "your-gemini-api-key".to_string(),
);

let request = GaiseInstructRequest {
    model: "gemini-2.5-flash".to_string(),
    input: OneOrMany::One(GaiseMessage {
        role: "user".to_string(),
        content: Some(OneOrMany::One(GaiseContent::Text {
            text: "Hello from Gemini!".to_string(),
        })),
        ..Default::default()
    }),
    ..Default::default()
};

let response = client.instruct(&request).await?;
```

### Direct Provider — OpenAI

```rust
use gaise_provider_openai::openai_client::GaiseClientOpenAI;

let client = GaiseClientOpenAI::new(
    "https://api.openai.com/v1".to_string(),
    "sk-your-api-key".to_string(),
);
```

### Direct Provider — Anthropic Claude

```rust
use gaise_provider_anthropic::anthropic_client::GaiseClientAnthropic;

let client = GaiseClientAnthropic::new(
    "https://api.anthropic.com/v1".to_string(),
    "sk-ant-your-api-key".to_string(),
);
```

### Streaming

```rust
use futures_util::StreamExt;

let mut stream = client.instruct_stream(&request).await?;
while let Some(chunk_res) = stream.next().await {
    let response = chunk_res?;
    if let GaiseStreamChunk::Text(text) = response.chunk {
        print!("{}", text);
    }
}
```

### Reasoning / Thinking

Works identically across providers — just change the model string:

```rust
let request = GaiseInstructRequest {
    model: "openai::o3".to_string(),  // or "anthropic::claude-sonnet-4-6", "gemini::gemini-3-flash-preview"
    generation_config: Some(GaiseGenerationConfig {
        thinking_effort: Some("high".to_string()),
        max_tokens: Some(32000),
        ..Default::default()
    }),
    input: OneOrMany::One(GaiseMessage {
        role: "user".to_string(),
        content: Some(OneOrMany::One(GaiseContent::Text {
            text: "Prove that the square root of 2 is irrational.".to_string(),
        })),
        ..Default::default()
    }),
    ..Default::default()
};
```

| GAISe field | OpenAI | Anthropic | Gemini |
|---|---|---|---|
| `thinking_effort` | `reasoning_effort` | `thinking.type` | `thinkingConfig.thinkingLevel` |
| `thinking_tokens` | N/A | `thinking.budget_tokens` | `thinkingConfig.thinkingBudget` |
| `max_tokens` | `max_completion_tokens` | `max_tokens` | `maxOutputTokens` |

### Embeddings

```rust
let request = GaiseEmbeddingsRequest {
    model: "gemini::gemini-embedding-001".to_string(),  // or "openai::text-embedding-3-small"
    input: OneOrMany::One("Text to embed".to_string()),
    ..Default::default()
};

let response = service.embeddings(&request).await?;
println!("Dimensions: {}", response.output[0].len());
```

### Multi-modality

```rust
let message = GaiseMessage {
    role: "user".to_string(),
    content: Some(OneOrMany::Many(vec![
        GaiseContent::Text { text: "Describe this image.".to_string() },
        GaiseContent::Image {
            data: std::fs::read("photo.png")?,
            format: Some("image/png".to_string()),
        },
    ])),
    ..Default::default()
};
```

### Tool Calling

```rust
use std::collections::HashMap;

let mut properties = HashMap::new();
properties.insert("location".to_string(), GaiseToolParameter {
    r#type: Some("string".to_string()),
    description: Some("City and state, e.g. San Francisco, CA".to_string()),
    ..Default::default()
});

let request = GaiseInstructRequest {
    model: "gemini::gemini-2.5-flash".to_string(),
    tools: Some(vec![GaiseTool {
        name: "get_weather".to_string(),
        description: Some("Get current weather".to_string()),
        parameters: Some(GaiseToolParameter {
            r#type: Some("object".to_string()),
            properties: Some(properties),
            required: Some(vec!["location".to_string()]),
            ..Default::default()
        }),
    }]),
    input: OneOrMany::One(GaiseMessage {
        role: "user".to_string(),
        content: Some(OneOrMany::One(GaiseContent::Text {
            text: "What's the weather in London?".to_string(),
        })),
        ..Default::default()
    }),
    ..Default::default()
};
```

### HTTP Server (gaise-api)

```bash
export OPENAI_API_KEY="sk-..." GEMINI_API_KEY="AIza..." ANTHROPIC_API_KEY="sk-ant-..."
cargo run -p gaise-api  # Listening on 0.0.0.0:3000
```

```bash
curl -X POST http://localhost:3000/v1/instruct \
  -H "Content-Type: application/json" \
  -d '{"model":"gemini::gemini-2.5-flash","input":{"role":"user","content":{"type":"text","text":"Hello!"}}}'
```

## Project Structure

| Crate | Description |
|-------|-------------|
| [`gaise`](https://crates.io/crates/gaise) | Core `GaiseClient` trait and all shared contracts |
| [`gaise-client`](https://crates.io/crates/gaise-client) | Provider router — `"provider::model"` string routing |
| [`gaise-provider-openai`](https://crates.io/crates/gaise-provider-openai) | OpenAI Chat Completions + Embeddings |
| [`gaise-provider-anthropic`](https://crates.io/crates/gaise-provider-anthropic) | Anthropic Messages API + extended thinking |
| [`gaise-provider-gemini`](https://crates.io/crates/gaise-provider-gemini) | Google Gemini v1beta API |
| [`gaise-provider-vertexai`](https://crates.io/crates/gaise-provider-vertexai) | Google Vertex AI with service account auth |
| [`gaise-provider-bedrock`](https://crates.io/crates/gaise-provider-bedrock) | AWS Bedrock Runtime |
| [`gaise-provider-ollama`](https://crates.io/crates/gaise-provider-ollama) | Ollama local inference |
| [`gaise-api`](https://crates.io/crates/gaise-api) | Axum HTTP server with SSE streaming |
| `gaise-chatbot` | Sample CLI chatbot |

## Environment Variables

| Variable | Provider |
|----------|----------|
| `OPENAI_API_KEY` / `OPENAI_API_URL` | OpenAI |
| `ANTHROPIC_API_KEY` / `ANTHROPIC_API_URL` | Anthropic |
| `GEMINI_API_KEY` / `GEMINI_API_URL` | Gemini |
| `VERTEXAI_SA_PATH` / `VERTEXAI_API_URL` | Vertex AI |
| `BEDROCK_REGION` | Bedrock |
| `OLLAMA_URL` | Ollama |
| `GAISE_PORT` | API server (default: 3000) |

## License

AGPLv3
