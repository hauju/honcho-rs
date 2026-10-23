# honcho

Async Rust SDK for the [Honcho](https://github.com/plastic-labs/honcho) memory API (v3).
Mirrors the official Python SDK. Built on `tokio` + `reqwest` (rustls); fits
Axum / Dioxus fullstack servers.

```toml
[dependencies]
honcho = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quickstart

```rust,no_run
use honcho::{ChatOptions, ContextOptions, Honcho};

#[tokio::main]
async fn main() -> honcho::Result<()> {
    // Managed service uses api.honcho.dev by default. For self-hosted, call
    // .base_url("http://localhost:8000") or set HONCHO_URL.
    let honcho = Honcho::builder()
        .workspace_id("my-app-testing")
        .api_key(std::env::var("HONCHO_API_KEY").unwrap())
        .build()?;

    // 1. Store: peers and messages on a session
    let alice = honcho.peer("alice").await?;
    let tutor = honcho.peer("tutor").await?;
    let session = honcho.session("session-1").await?;
    session
        .add_messages(vec![
            alice.message("Hey there — can you help me with my math homework?"),
            tutor.message("Absolutely. Send me your first problem!"),
        ])
        .await?;

    // 2. Reason: happens asynchronously in the background.

    // 3. Query: ask Honcho what it knows, or pull prompt-ready context.
    let answer = alice
        .chat("What learning styles does the user respond to best?", ChatOptions::default())
        .await?;
    let context = session
        .context(ContextOptions { summary: Some(true), tokens: Some(10_000), ..Default::default() })
        .await?;

    // 4. Inject: hand the context to your model of choice.
    let messages = context.to_openai(&tutor); // Vec<serde_json::Value>
    Ok(())
}
```

Background reasoning is asynchronous: newly added messages may take a moment
to show up in chat and representation responses.

## Configuration

| Builder method   | Env fallback          | Default                  |
|------------------|-----------------------|--------------------------|
| `api_key`        | `HONCHO_API_KEY`      | none                     |
| `base_url`       | `HONCHO_URL`          | `https://api.honcho.dev` |
| `workspace_id`   | `HONCHO_WORKSPACE_ID` | `default`                |
| `timeout`        |                       | 60s                      |
| `default_headers`|                       |                          |

`Honcho::from_env()` uses only the env fallbacks. Requests retry twice on
429/500/502/503/504 and transport errors (500ms, 1s backoff, or `Retry-After`).

## Streaming chat

```rust,no_run
use futures::StreamExt;
use honcho::{ChatChunk, ChatOptions};

# async fn run(alice: honcho::Peer) -> honcho::Result<()> {
let mut stream = alice.chat_stream("What does alice like?", ChatOptions::default()).await?;
while let Some(chunk) = stream.next().await {
    match chunk? {
        ChatChunk::Content(text) => print!("{text}"),
        ChatChunk::Done { evidence } => { let _ = evidence; }
    }
}
# Ok(())
# }
```

## License

MIT OR Apache-2.0
