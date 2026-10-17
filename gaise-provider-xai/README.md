# gaise-provider-xai

[![crates.io](https://img.shields.io/crates/v/gaise-provider-xai.svg)](https://crates.io/crates/gaise-provider-xai)
[![docs.rs](https://docs.rs/gaise-provider-xai/badge.svg)](https://docs.rs/gaise-provider-xai)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/ikcore/gaise#license)

This is the xAI (Grok) provider for [GAISe](https://crates.io/crates/gaise). It implements `GaiseClient` and `GaiseSpeechClient`, and `GaiseLiveClient` with the `live` feature.

## Supported features

- **Instruct and streaming** through Chat Completions: text, image input, function tools, and `reasoning_content` returned as reasoning parts.
- **Reasoning effort** with per-family rules:
  - grok-4.7 and grok-4.6: low to xhigh.
  - grok-4.5: low to high.
  - grok-4.3: none to xhigh.
- **Usage** with cached, image, and reasoning tokens, plus `cost_in_usd_ticks`.
- **Text to speech** (`xai::tts`, `POST /v1/tts`): mp3, wav, pcm, mulaw, and alaw output, voice and speed selection, and character timestamps.
- **Speech to speech** (`grok-voice-3.0` / `grok-voice-latest`, feature `live`): the realtime WebSocket with 24 kHz PCM16, server VAD or manual turns, function tools, and input transcription.
- **Model listing** with aliases, context length, and accepted reasoning efforts.

The Responses API is not used. That excludes encrypted reasoning, server-side tools, and the multi-agent model. Image, video, and speech-to-text endpoints have no GAISe surface.

## Usage

```rust
use gaise_core::GaiseClient;
use gaise_core::contracts::*;
use gaise_provider_xai::xai_client::GaiseClientXai;

let client = GaiseClientXai::new(
    "https://api.x.ai/v1".to_string(),
    std::env::var("XAI_API_KEY")?,
);

let request = GaiseInstructRequest {
    model: "grok-4.7".to_string(),
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

Through `gaise-client`, use `xai::grok-4.7`, `xai::tts`, or `xai::grok-voice-latest`.

See the [xAI vendor page](https://github.com/ikcore/gaise/blob/main/wiki/vendor-xai.md) for the full mapping.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
