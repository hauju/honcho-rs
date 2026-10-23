//! Async Rust SDK for [Honcho](https://honcho.dev), the memory layer for
//! agents. Mirrors the official Python SDK.
//!
//! # Quickstart
//!
//! ```no_run
//! use honcho::{ContextOptions, ChatOptions, Honcho};
//!
//! # async fn run() -> honcho::Result<()> {
//! // Managed service uses api.honcho.dev by default. For self-hosted, call
//! // .base_url("http://localhost:8000") or set HONCHO_URL.
//! let honcho = Honcho::builder()
//!     .workspace_id("my-app-testing")
//!     .api_key(std::env::var("HONCHO_API_KEY").unwrap())
//!     .build()?;
//!
//! // 1. Store: peers and messages on a session
//! let alice = honcho.peer("alice").await?;
//! let tutor = honcho.peer("tutor").await?;
//! let session = honcho.session("session-1").await?;
//! session
//!     .add_messages(vec![
//!         alice.message("Hey there — can you help me with my math homework?"),
//!         tutor.message("Absolutely. Send me your first problem!"),
//!     ])
//!     .await?;
//!
//! // 2. Reason: happens asynchronously in the background.
//!
//! // 3. Query: ask Honcho what it knows, or pull prompt-ready context.
//! let answer = alice
//!     .chat("What learning styles does the user respond to best?", ChatOptions::default())
//!     .await?;
//! let context = session
//!     .context(ContextOptions { summary: Some(true), tokens: Some(10_000), ..Default::default() })
//!     .await?;
//!
//! // 4. Inject: hand the context to your model of choice.
//! let messages = context.to_openai(&tutor); // Vec<serde_json::Value>
//! # let _ = (answer, messages);
//! # Ok(())
//! # }
//! ```
//!
//! # Streaming
//!
//! ```no_run
//! use futures::StreamExt;
//! use honcho::{ChatChunk, ChatOptions, Honcho};
//!
//! # async fn run(alice: honcho::Peer) -> honcho::Result<()> {
//! let mut stream = alice.chat_stream("What does alice like?", ChatOptions::default()).await?;
//! while let Some(chunk) = stream.next().await {
//!     match chunk? {
//!         ChatChunk::Content(text) => print!("{text}"),
//!         ChatChunk::Done { .. } => break,
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Background reasoning is asynchronous: newly added messages may take a
//! moment to show up in chat and representation responses.

mod client;
mod conclusions;
mod error;
mod options;
mod pagination;
mod peer;
mod scope;
mod session;
mod session_context;
mod sse;
pub mod types;

pub use client::{Honcho, HonchoBuilder};
pub use conclusions::{ConclusionsView, WorkspaceConclusions};
pub use error::{ApiError, Error, Result};
pub use options::{
    ChatOptions, ConclusionQueryOptions, ContextOptions, FileUpload, ListOptions,
    PeerContextOptions, QueueStatusOptions, RepresentationOptions, SearchOptions,
};
pub use pagination::Page;
pub use peer::Peer;
pub use scope::Scope;
pub use session::Session;
pub use session_context::SessionContext;
pub use sse::ChatStream;
pub use types::*;
