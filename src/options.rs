//! Option structs for calls with many optional parameters. All derive
//! `Default`, so callers write `..Default::default()`.

use std::time::Duration;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::types::{MessageConfiguration, Metadata, ReasoningLevel};

/// Paging and filtering for list endpoints.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Filter object, e.g. `json!({"metadata": {"topic": "math"}})`.
    pub filters: Option<Value>,
    /// 1-based page number (default 1).
    pub page: Option<u32>,
    /// Page size, 1..=100 (default 50).
    pub size: Option<u32>,
    pub reverse: bool,
}

/// Scope selection for chat and representation calls. The two shapes mean
/// different things to the server: a single scope answers from that scope's
/// own representation of the target (including conclusions reasoned within
/// it), while a list keeps the peer's own view and only restricts recall to
/// the scopes' member sessions.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ScopeSelector {
    One(String),
    Many(Vec<String>),
}

impl From<&str> for ScopeSelector {
    fn from(s: &str) -> Self {
        Self::One(s.to_owned())
    }
}

impl From<String> for ScopeSelector {
    fn from(s: String) -> Self {
        Self::One(s)
    }
}

impl From<Vec<String>> for ScopeSelector {
    fn from(v: Vec<String>) -> Self {
        Self::Many(v)
    }
}

/// Options for dialectic chat.
#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    /// Ask what this peer knows about `target` instead of about itself (peer chat only).
    pub target: Option<String>,
    pub session: Option<String>,
    /// Confine recall to a scope or scopes; see [`ScopeSelector`].
    pub scope: Option<ScopeSelector>,
    /// Restrict recall to these sessions (peer chat only).
    pub sessions: Option<Vec<String>>,
    pub reasoning_level: Option<ReasoningLevel>,
    /// JSON schema for structured output; the answer `content` is then a JSON string.
    pub response_format: Option<Value>,
    pub include_evidence: bool,
    /// Per-request timeout override.
    pub timeout: Option<Duration>,
}

impl ChatOptions {
    pub(crate) fn body(&self, query: &str, stream: bool, peer_level: bool) -> Value {
        let mut m = Map::new();
        m.insert("query".into(), json!(query));
        m.insert("stream".into(), json!(stream));
        if peer_level {
            put(&mut m, "target", &self.target);
            if let Some(sessions) = &self.sessions {
                m.insert("filters".into(), json!({ "session_id": sessions }));
            }
        }
        put(&mut m, "session_id", &self.session);
        put(&mut m, "scope", &self.scope);
        put(&mut m, "reasoning_level", &self.reasoning_level);
        put(&mut m, "response_format", &self.response_format);
        if self.include_evidence {
            m.insert("include_evidence".into(), json!(true));
        }
        Value::Object(m)
    }
}

/// Options for representation calls (`Peer::representation`,
/// `Session::representation`, `ConclusionsView::representation`).
#[derive(Debug, Clone, Default)]
pub struct RepresentationOptions {
    pub session: Option<String>,
    pub target: Option<String>,
    pub search_query: Option<String>,
    pub search_top_k: Option<u32>,
    pub search_max_distance: Option<f64>,
    pub include_most_frequent: Option<bool>,
    pub max_conclusions: Option<u32>,
    /// Confine the representation to a scope or scopes; see [`ScopeSelector`].
    pub scope: Option<ScopeSelector>,
    /// Restrict to these sessions (sent as a `session_id` filter).
    pub sessions: Option<Vec<String>>,
}

impl RepresentationOptions {
    pub(crate) fn body(&self) -> Value {
        let mut m = Map::new();
        put(&mut m, "session_id", &self.session);
        put(&mut m, "target", &self.target);
        put(&mut m, "search_query", &self.search_query);
        put(&mut m, "search_top_k", &self.search_top_k);
        put(&mut m, "search_max_distance", &self.search_max_distance);
        put(&mut m, "include_most_frequent", &self.include_most_frequent);
        put(&mut m, "max_conclusions", &self.max_conclusions);
        put(&mut m, "scope", &self.scope);
        if let Some(sessions) = &self.sessions {
            m.insert("filters".into(), json!({ "session_id": sessions }));
        }
        Value::Object(m)
    }
}

/// Options for [`Peer::context`](crate::Peer::context).
#[derive(Debug, Clone, Default)]
pub struct PeerContextOptions {
    pub target: Option<String>,
    pub search_query: Option<String>,
    pub search_top_k: Option<u32>,
    pub search_max_distance: Option<f64>,
    pub include_most_frequent: Option<bool>,
    pub max_conclusions: Option<u32>,
}

/// Options for [`Session::context`](crate::Session::context).
#[derive(Debug, Clone, Default)]
pub struct ContextOptions {
    /// Include the session summary if available (server default: true).
    pub summary: Option<bool>,
    /// Token budget for the context.
    pub tokens: Option<u32>,
    /// Include representation and card for this peer.
    pub peer_target: Option<String>,
    /// View `peer_target` from this peer's perspective.
    pub peer_perspective: Option<String>,
    /// Use a scope as the perspective source (exclusive with `peer_perspective`).
    pub scope: Option<String>,
    /// Session allowlist for `peer_target`'s representation.
    pub sessions: Option<Vec<String>>,
    pub limit_to_session: bool,
    pub search_query: Option<String>,
    pub search_top_k: Option<u32>,
    pub search_max_distance: Option<f64>,
    pub include_most_frequent: Option<bool>,
    pub max_conclusions: Option<u32>,
}

/// Options for message search.
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub filters: Option<Value>,
    /// 1..=100 (server default 10).
    pub limit: Option<u32>,
    /// Restrict to a scope's sessions (workspace search only).
    pub scope: Option<String>,
}

impl SearchOptions {
    pub(crate) fn body(&self, query: &str, workspace_level: bool) -> Value {
        let mut m = Map::new();
        m.insert("query".into(), json!(query));
        put(&mut m, "filters", &self.filters);
        put(&mut m, "limit", &self.limit);
        if workspace_level {
            put(&mut m, "scope", &self.scope);
        }
        Value::Object(m)
    }
}

/// Filters for queue status.
#[derive(Debug, Clone, Default)]
pub struct QueueStatusOptions {
    pub observer: Option<String>,
    pub sender: Option<String>,
    /// Ignored by [`Session::queue_status`](crate::Session::queue_status), which uses its own ID.
    pub session: Option<String>,
}

/// Options for [`ConclusionsView::query`](crate::ConclusionsView::query).
#[derive(Debug, Clone, Default)]
pub struct ConclusionQueryOptions {
    /// 1..=100 (server default 10).
    pub top_k: Option<u32>,
    pub distance: Option<f64>,
    pub filters: Option<Value>,
}

/// A file to turn into messages via [`Session::upload_file`](crate::Session::upload_file).
#[derive(Debug, Clone, Default)]
pub struct FileUpload {
    pub filename: String,
    /// MIME type, e.g. `application/pdf` or `text/plain`.
    pub content_type: String,
    pub bytes: Bytes,
    /// Peer the resulting messages are attributed to.
    pub peer_id: String,
    pub metadata: Option<Metadata>,
    pub configuration: Option<MessageConfiguration>,
    pub created_at: Option<DateTime<Utc>>,
}

pub(crate) fn put<T: Serialize>(m: &mut Map<String, Value>, key: &str, value: &Option<T>) {
    if let Some(v) = value {
        m.insert(key.into(), json!(v));
    }
}
