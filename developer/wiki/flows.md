# Request flows

These diagrams describe the current implementation boundaries.

## Client routing

```mermaid
flowchart TD
    A[Gaise request with provider::model] --> B{Split first ::}
    B -->|Known provider| C[Remove prefix]
    B -->|Missing or unknown| X[Return routing error]
    C --> D{Client cached?}
    D -->|Yes| E[Reuse Arc client]
    D -->|No| F[Initialize from GaiseClientConfig]
    F --> E
    E --> G[Call instruct, stream, embeddings, or live]
```

## Non-streaming instruct

```mermaid
sequenceDiagram
    participant App
    participant Core as GAISe contract
    participant Adapter
    participant Provider
    App->>Core: GaiseInstructRequest
    Core->>Adapter: Provider-neutral messages/config/tools
    Adapter->>Adapter: Flatten parts and map MIME, roles, tools, reasoning
    Adapter->>Provider: Provider request
    Provider-->>Adapter: Provider response + usage
    Adapter->>Adapter: Preserve order, IDs, signatures, media
    Adapter-->>Core: GaiseInstructResponse
    Core-->>App: Messages + external_id + input/output/total usage
```

## Multimodal mapping

```mermaid
flowchart LR
    P[Ordered GaiseContent] --> F[Recursive flatten]
    F --> T[Text]
    F --> I[Image + normalized MIME]
    F --> A[Audio + normalized MIME]
    F --> D[File + inferred MIME]
    F --> R[Reasoning + signature]
    F --> RR[Opaque redacted reasoning]
    T --> W[Provider wire parts]
    I --> W
    A --> W
    D --> W
    R --> W
    RR --> W
    W --> S{Selected endpoint supports part?}
    S -->|Yes| N[Native provider block]
    S -->|Safe text fallback| M[Explicit tagged/unsupported marker]
    S -->|No valid mapping| E[Return error]
```

Adapters do not silently discard unsupported content. Whether they can use a text marker or must fail depends on the provider's valid message schema.

## Streaming

```mermaid
flowchart TD
    A[HTTP response byte chunks] --> B[Provider framing buffer]
    B --> C{Complete SSE/NDJSON frame?}
    C -->|No| B
    C -->|Yes| D[Deserialize provider event]
    D --> E{Event kind}
    E -->|Text delta| T[GaiseStreamChunk::Text]
    E -->|Reasoning/media| M[GaiseStreamChunk::Content]
    E -->|Tool delta| F[Indexed ToolCall chunk]
    E -->|Usage snapshot| U[GaiseStreamChunk::Usage]
    T --> A2[GaiseStreamAccumulator]
    M --> A2
    F --> A2
    U --> A2
    A2 --> O[Ordered message + assembled tool calls]
    A2 --> US[Latest usage counters by key]
```

The raw transport chunks are not assumed to align with SSE lines or JSON objects. Buffers retain an incomplete trailing frame until the next network chunk.

## Tool-call loop

```mermaid
sequenceDiagram
    participant App
    participant GAISe
    participant Model
    participant Tool
    App->>GAISe: Request + recursive tool schema
    GAISe->>Model: Provider-native tool declaration
    Model-->>GAISe: Call ID + name + arguments + optional signature
    GAISe-->>App: GaiseToolCall
    App->>Tool: Validate and execute arguments
    Tool-->>App: JSON and/or supported media result
    App->>GAISe: History + assistant call + tool result with ID and name
    GAISe->>Model: Provider-native function/tool result
    Model-->>GAISe: Final text/reasoning/media + usage
    GAISe-->>App: Normalized response
```

Parallel streaming tool calls are correlated by their provider index. Gemini/Vertex names and thought signatures must be preserved alongside IDs.

## Usage normalization

```mermaid
flowchart TD
    P[Provider usage object] --> I[Input map]
    P --> O[Output map]
    P --> T[Request-wide total map]
    I --> IA[Aggregate provider input counter]
    I --> IM[Reported text/image/audio/video/document details]
    I --> IC[Reported cache and tool-prompt details]
    O --> OA[Aggregate provider output counter]
    O --> OM[Reported text/image/audio details]
    O --> OR[Reported reasoning/prediction/tool details]
    T --> TT[total_tokens]
    IA --> G[GaiseUsage]
    IM --> G
    IC --> G
    OA --> G
    OM --> G
    OR --> G
    TT --> G
```

Detail counters overlap aggregates. Missing modality detail stays missing; it is never inferred from content type or aggregate tokens.

## Embeddings

```mermaid
flowchart LR
    A[OneOrMany text strings] --> B{Provider}
    B --> O[OpenAI embeddings]
    B --> G[Gemini batchEmbedContents]
    B --> V[Vertex prediction endpoint]
    B --> D[Bedrock Titan/Cohere InvokeModel]
    B --> L[Ollama api/embed]
    O --> N[Vec of float vectors]
    G --> N
    V --> N
    D --> N
    L --> N
    N --> R[GaiseEmbeddingsResponse + available usage]
```

Anthropic has no embeddings branch. The common embedding input is currently text-only.

## Live session

```mermaid
sequenceDiagram
    participant App
    participant Live as GaiseLiveClient
    participant WS as Provider WebSocket
    App->>Live: live_connect(config)
    Live->>WS: Connect and send setup/session update
    WS-->>Live: Setup/session confirmation
    Live-->>App: GaiseLiveSession + SessionStarted
    par Input task
        App->>Live: Text / audio / image / control
        Live->>WS: Provider client event
        App->>Live: ToolResponse
        Live->>WS: Provider function response
    and Output task
        WS-->>Live: Text/audio/transcript/tool/usage events
        Live-->>App: GaiseLiveEvent stream
    end
    App->>Live: Close
    Live->>WS: Close frame
    WS-->>Live: Closed
    Live-->>App: SessionEnded
```

OpenAI uses the current GA nested audio/session shape. Gemini uses `setup`, `realtimeInput`, and `toolResponse`; images are sent as realtime video frames. A setup error or early socket close is surfaced instead of being reported as a started session.

## Retry and errors

```mermaid
flowchart TD
    A[Send request] --> B{HTTP status}
    B -->|Success| C[Parse response]
    B -->|Transient status| D{Retry budget remains?}
    D -->|Yes| E[Backoff] --> A
    D -->|No| F[Return provider error]
    B -->|Caller/schema/auth error| F
    C -->|Valid shape| G[Map response]
    C -->|Invalid shape| H[Return parse error with bounded context]
```

Retries are adapter-specific and limited to transient classes. Invalid inputs, authentication failures, and unsupported content are not made to look successful.

