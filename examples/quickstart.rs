//! Port of the Honcho README quickstart.
//!
//! ```sh
//! HONCHO_API_KEY=... cargo run --example quickstart
//! ```

use honcho::{ChatOptions, ContextOptions, Honcho};

#[tokio::main]
async fn main() -> honcho::Result<()> {
    // Managed service uses api.honcho.dev by default. For self-hosted, call
    // .base_url("http://localhost:8000") or set HONCHO_URL.
    let honcho = Honcho::builder().workspace_id("my-app-testing").build()?;

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
        .chat(
            "What learning styles does the user respond to best?",
            ChatOptions::default(),
        )
        .await?;
    println!("answer: {:?}", answer.content);

    let context = session
        .context(ContextOptions {
            summary: Some(true),
            tokens: Some(10_000),
            ..Default::default()
        })
        .await?;

    // 4. Inject: hand the context to your model of choice.
    let messages = context.to_openai(&tutor);
    println!("{}", serde_json::to_string_pretty(&messages).unwrap());
    Ok(())
}
