# gaise-client

`gaise-client` is a provider aggregator for the GAISe (Generative AI Service) project. It allows you to use multiple AI providers (OpenAI, Anthropic, Gemini, VertexAI, Ollama, Bedrock, ElevenLabs, TypeSafe AI) through a single interface, routing requests based on a model naming convention.

## Features

- **Provider Aggregation**: Manage multiple providers in one service.
- **Unified Interface**: Implements the `GaiseClient` trait.
- **Dynamic Routing**: Route requests using the `provider::model` format.
- **Lazy Initialization**: Providers are initialized only when first requested.
- **Feature Flags**: Enable only the providers you need to keep dependencies lean.

## Feature Flags

`gaise-client` uses feature flags to reduce the number of dependencies. By default, all providers are enabled.

- `openai`: Enables the OpenAI provider.
- `vertexai`: Enables the Google VertexAI provider.
- `ollama`: Enables the Ollama provider.
- `bedrock`: Enables the AWS Bedrock provider.
- `anthropic`: Enables the Anthropic Claude provider.
- `gemini`: Enables the Google Gemini provider.
- `typesafe`: Enables Jev typed decisions via `system_one` (`typesafe::jev`).
- `live`: Enables real-time WebSocket sessions via `GaiseLiveClient` (currently supports `openai` and `gemini`).

To use only specific providers, disable default features in your `Cargo.toml`:

```toml
[dependencies]
gaise-client = { version = "3.0.1", default-features = false, features = ["openai"] }
```

To enable live/realtime sessions:

```toml
[dependencies]
gaise-client = { version = "3.0.1", features = ["live"] }
```

## Supported Providers

| Provider key | crates.io package |
| --- | --- |
| `openai` | [`gaise-provider-openai`](https://crates.io/crates/gaise-provider-openai) |
| `vertexai` | [`gaise-provider-vertexai`](https://crates.io/crates/gaise-provider-vertexai) |
| `ollama` | [`gaise-provider-ollama`](https://crates.io/crates/gaise-provider-ollama) |
| `bedrock` | [`gaise-provider-bedrock`](https://crates.io/crates/gaise-provider-bedrock) |
| `anthropic` | [`gaise-provider-anthropic`](https://crates.io/crates/gaise-provider-anthropic) |
| `gemini` | [`gaise-provider-gemini`](https://crates.io/crates/gaise-provider-gemini) |
| `elevenlabs` | [`gaise-provider-elevenlabs`](https://crates.io/crates/gaise-provider-elevenlabs) |
| `typesafe` | [`gaise-provider-typesafe`](https://crates.io/crates/gaise-provider-typesafe) |

## Usage

### Configuration

First, set up the `GaiseClientConfig` with the necessary credentials and URLs. Note that fields in `GaiseClientConfig` are conditionally compiled based on enabled features.

```rust
use gaise_client::{GaiseClientConfig, GaiseClientService};

let config = GaiseClientConfig {
    #[cfg(feature = "openai")]
    openai_api_key: Some("your-openai-key".to_string()),
    #[cfg(feature = "anthropic")]
    anthropic_api_key: Some("your-anthropic-key".to_string()),
    #[cfg(feature = "ollama")]
    ollama_url: Some("http://localhost:11434".to_string()),
    ..Default::default()
};

let service = GaiseClientService::new(config);
```

### Making Requests

Use the `provider::model` format in the `model` field of your requests.

```rust
use gaise_core::contracts::{GaiseInstructRequest, GaiseMessage, GaiseContent, OneOrMany};
use gaise_core::GaiseClient;

let request = GaiseInstructRequest {
    model: "openai::gpt-5.6-terra".to_string(),
    input: OneOrMany::One(GaiseMessage {
        role: "user".to_owned(),
        content: Some(OneOrMany::One(GaiseContent::Text { 
            text: "Hello, how are you?".to_owned() 
        })),
        ..Default::default()
    }),
    ..Default::default()
};

let response = service.instruct(&request).await?;
```

## How it works

The `GaiseClientService` parses the `model` string to identify the provider.
1. It looks for the first occurrence of `::`.
2. The part before `::` is used as the provider ID.
3. The part after `::` is passed to the specific provider as the actual model name.

If you request `ollama::qwen3.8`, the service will:
1. Initialize (or retrieve) the Ollama client.
2. Call the Ollama client with `model: "qwen3.8"`.

### Live / Realtime Sessions (feature = "live")

With the `live` feature enabled, `GaiseClientService` also implements `GaiseLiveClient` for real-time bidirectional WebSocket sessions (audio + text streaming). Currently supported by `openai` and `gemini`.

```rust
use gaise_core::GaiseLiveClient;
use gaise_core::contracts::*;
use futures_util::StreamExt;

let config = GaiseLiveConfig {
    model: "gemini::gemini-3.1-flash-live-preview".to_string(),
    voice: Some("Puck".to_string()),
    modalities: vec![GaiseLiveModality::Audio, GaiseLiveModality::Text],
    ..Default::default()
};

let session = service.live_connect(&config).await?;

// Send inputs via session.tx (audio, text, tool responses)
// Receive events via session.rx (audio, text, transcripts, tool calls, etc.)
while let Some(event) = session.rx.next().await {
    match event? {
        GaiseLiveEvent::Audio { data, sample_rate } => { /* play audio */ }
        GaiseLiveEvent::Text { text } => { /* display text */ }
        GaiseLiveEvent::ToolCall { id, function } => { /* handle tool call */ }
        GaiseLiveEvent::TurnComplete => { /* model finished responding */ }
        _ => {}
    }
}
```

### Example with Anthropic

```rust
let request = GaiseInstructRequest {
    model: "anthropic::claude-sonnet-5".to_string(),
    input: OneOrMany::One(GaiseMessage {
        role: "user".to_owned(),
        content: Some(OneOrMany::One(GaiseContent::Text {
            text: "Hello, Claude!".to_owned()
        })),
        ..Default::default()
    }),
    ..Default::default()
};

let response = service.instruct(&request).await?;
```

## TypeSafe System One

Call `system_one` with `typesafe::jev` to evaluate typed Noul, Choice, and Score
questions about shared state. The adapter maps `jev` to upstream `jev-latest`.

### Rust example

```toml
[dependencies]
gaise-core = { package = "gaise", version = "3.0.1" }
gaise-client = { version = "3.0.1", default-features = false, features = ["typesafe"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use gaise_client::{GaiseClientConfig, GaiseClientService};
use gaise_core::{
    GaiseClient,
    contracts::{GaiseAnswer, GaiseSystemOneRequest},
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = GaiseClientService::new(GaiseClientConfig {
        typesafe_api_key: Some(std::env::var("TYPESAFE_API_KEY")?),
        ..Default::default()
    });
    let request: GaiseSystemOneRequest = serde_json::from_value(json!({
        "model": "typesafe::jev",
        "state": {"ticket": "I was charged twice. Please fix this today."},
        "questions": {
            "urgent": {"type": "noul", "instructions": "Is this urgent?"}
        }
    }))?;
    let response = client.system_one(&request).await?;
    if let Some(GaiseAnswer::Noul { noul }) = response.answers.get("urgent") {
        println!("Urgency probability: {noul}");
    }
    Ok(())
}
```

The same response map can contain `GaiseAnswer::Choice` and `GaiseAnswer::Score`.
The [TypeSafe provider crate](https://crates.io/crates/gaise-provider-typesafe)
also exposes `GaiseClientTypeSafe::new(api_url, api_key)` for direct use with the
bare model name `jev`.

Set `typesafe_api_key` and optional `typesafe_api_url` in the service config.
Per-request `connection` fields take precedence. See the [complete request,
response, configuration, and error reference](https://crates.io/crates/gaise-provider-typesafe).
