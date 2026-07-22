# Clients and routing

GAISe can be used through `GaiseClientService` or through a direct provider client. The router is convenient for applications that choose providers at runtime; direct clients minimize enabled dependencies and use raw model IDs.

## Cargo features

```toml
[dependencies]
gaise-core = { package = "gaise", version = "0.1" }
gaise-client = {
  version = "0.1",
  default-features = false,
  features = ["openai", "anthropic", "gemini", "vertexai", "bedrock", "ollama", "live"]
}
```

`gaise-client` enables all six normal providers by default. The `live` feature adds live support only for whichever of `openai` and `gemini` are also enabled.

| Feature | Adapter |
|---|---|
| `openai` | Chat Completions and Embeddings |
| `anthropic` | Messages |
| `gemini` | Gemini generateContent and Embeddings |
| `vertexai` | Vertex AI Gemini and prediction embeddings |
| `bedrock` | Converse, ConverseStream, and InvokeModel |
| `ollama` | Local Chat and Embed APIs |
| `live` | OpenAI Realtime and/or Gemini Live |

## Multi-provider router

```rust
use gaise_client::{GaiseClientConfig, GaiseClientService};

let service = GaiseClientService::new(GaiseClientConfig {
    openai_api_url: Some("https://api.openai.com/v1".into()),
    openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
    anthropic_api_url: Some("https://api.anthropic.com/v1".into()),
    anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
    gemini_api_url: Some("https://generativelanguage.googleapis.com/v1beta".into()),
    gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
    vertexai_api_url: std::env::var("VERTEXAI_API_URL").ok(),
    vertexai_sa: service_account,
    bedrock_region: Some("eu-west-2".into()),
    ollama_url: Some("http://localhost:11434".into()),
    logger: None,
});
```

Fields are conditionally compiled by their corresponding features. `service_account` above is an `Option<ServiceAccount>` parsed by the application.

The router splits the first `::`:

| Routed ID | Provider receives |
|---|---|
| `openai::gpt-5.6-terra` | `gpt-5.6-terra` |
| `anthropic::claude-sonnet-5` | `claude-sonnet-5` |
| `gemini::gemini-3.6-flash` | `gemini-3.6-flash` |
| `vertexai::gemini-3.5-flash` | `gemini-3.5-flash` |
| `bedrock::us.anthropic.claude-sonnet-5` | `us.anthropic.claude-sonnet-5` |
| `ollama::qwen3:8b` | `qwen3:8b` |

Clients are initialized lazily and cached. Unknown prefixes and missing required configuration return errors.

## Common calls

Any normal client implements `GaiseClient`:

```rust
use gaise_core::GaiseClient;

let response = service.instruct(&request).await?;
let stream = service.instruct_stream(&request).await?;
let embeddings = service.embeddings(&embedding_request).await?;
```

`instruct_stream` returns a pinned asynchronous stream. See [request examples](request-examples.md#streaming) for collection and event handling.

## Direct clients

Direct clients receive an unprefixed model ID.

### OpenAI

```rust
use gaise_provider_openai::openai_client::GaiseClientOpenAI;

let client = GaiseClientOpenAI::new(
    "https://api.openai.com/v1".into(),
    std::env::var("OPENAI_API_KEY")?,
);
```

The normal client uses Chat Completions and Embeddings. It is deliberately not a Responses or Images API adapter.

### Anthropic

```rust
use gaise_provider_anthropic::anthropic_client::GaiseClientAnthropic;

let client = GaiseClientAnthropic::new(
    "https://api.anthropic.com/v1".into(),
    std::env::var("ANTHROPIC_API_KEY")?,
);
```

`with_version` can override the default Anthropic API version when an application explicitly needs to do so.

### Gemini API

```rust
use gaise_provider_gemini::gemini_client::GaiseClientGemini;

let client = GaiseClientGemini::new(
    "https://generativelanguage.googleapis.com/v1beta".into(),
    std::env::var("GEMINI_API_KEY")?,
);
```

### Vertex AI

```rust
use gaise_provider_vertexai::contracts::ServiceAccount;
use gaise_provider_vertexai::vertexai_client::GaiseClientVertexAI;

let sa: ServiceAccount = serde_json::from_str(
    &std::fs::read_to_string(std::env::var("VERTEXAI_SA_PATH")?)?,
)?;
let client = GaiseClientVertexAI::new(
    &sa,
    "https://us-central1-aiplatform.googleapis.com/v1/projects/PROJECT/locations/us-central1/publishers/google/models/{{MODEL}}".into(),
).await?;
```

The access token is refreshed behind an asynchronous mutex. Model availability, endpoint region, and lifecycle are Vertex-specific and must not be inferred from the Gemini API catalog.

### Amazon Bedrock

```rust
use gaise_provider_bedrock::bedrock_client::GaiseClientBedrock;

let client = GaiseClientBedrock::new_with_region(Some("eu-west-2".into())).await;
```

The AWS SDK credential chain remains authoritative. The constructor passes region configuration directly and does not mutate process-wide `AWS_REGION`.

### Ollama

```rust
use gaise_provider_ollama::ollama_client::GaiseClientOllama;

let client = GaiseClientOllama::new("http://localhost:11434".into());
```

Support is determined by the locally installed model tag.

## Direct live clients

```rust
use gaise_provider_openai::openai_live_client::GaiseClientOpenAILive;
use gaise_provider_gemini::gemini_live_client::GaiseClientGeminiLive;

let openai_live = GaiseClientOpenAILive::new(
    "https://api.openai.com".into(),
    std::env::var("OPENAI_API_KEY")?,
);
let gemini_live = GaiseClientGeminiLive::new(
    "https://generativelanguage.googleapis.com/v1beta".into(),
    std::env::var("GEMINI_API_KEY")?,
);
```

The router also implements `GaiseLiveClient` when built with `live`; use a routed model such as `openai::gpt-realtime-2.1` or `gemini::gemini-3.1-flash-live-preview`.

## Configuration reference

| Environment variable | Used by |
|---|---|
| `OPENAI_API_KEY`, `OPENAI_API_URL`, optional `OPENAI_API_TIER` | OpenAI |
| `ANTHROPIC_API_KEY`, `ANTHROPIC_API_URL` | Anthropic |
| `GEMINI_API_KEY`, `GEMINI_API_URL` | Gemini |
| `VERTEXAI_SA_PATH`, `VERTEXAI_API_URL`, optional `VERTEXAI_API_TIER` | Vertex AI |
| `BEDROCK_REGION` and standard AWS credential/profile variables | Bedrock |
| `OLLAMA_URL` | Ollama |

Never commit credentials or service-account JSON. GAISe does not need credentials for its local mapping and serialization tests.

