//! Serde models for the Honcho v3 API, mirroring the OpenAPI schemas.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Free-form JSON object used for metadata.
pub type Metadata = serde_json::Map<String, Value>;

/// Workspace as returned by the API (`Workspace` schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: String,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub configuration: WorkspaceConfiguration,
    pub created_at: DateTime<Utc>,
}

/// Peer as returned by the API (`Peer` schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    #[serde(default)]
    pub uid: Option<String>,
    pub workspace_id: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub configuration: PeerConfig,
}

/// Session as returned by the API (`Session` schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    #[serde(default)]
    pub uid: Option<String>,
    pub is_active: bool,
    pub workspace_id: String,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub configuration: SessionConfiguration,
    pub created_at: DateTime<Utc>,
}

/// Scope as returned by the API (`Scope` schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeInfo {
    pub id: String,
    #[serde(default)]
    pub metadata: Metadata,
    pub created_at: DateTime<Utc>,
}

/// A message stored in a session (`Message` schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub peer_id: String,
    pub session_id: String,
    #[serde(default)]
    pub metadata: Metadata,
    pub created_at: DateTime<Utc>,
    pub workspace_id: String,
    pub token_count: i64,
}

/// A message to add to a session (`MessageCreate` schema). Build one with
/// [`Peer::message`](crate::Peer::message).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageCreate {
    pub content: String,
    pub peer_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

impl MessageCreate {
    /// Attach metadata to the message.
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Attach per-message configuration.
    pub fn configuration(mut self, configuration: MessageConfiguration) -> Self {
        self.configuration = Some(configuration);
        self
    }

    /// Override the message timestamp.
    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }
}

/// `MessageConfiguration` schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfiguration>,
}

/// `ReasoningConfiguration` schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReasoningConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
}

/// `PeerCardConfiguration` schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PeerCardConfiguration {
    #[serde(default, rename = "use", skip_serializing_if = "Option::is_none")]
    pub use_card: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
}

/// `SummaryConfiguration` schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SummaryConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages_per_short_summary: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages_per_long_summary: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
}

/// `DreamConfiguration` schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DreamConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
}

/// `DialecticConfiguration` schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DialecticConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
}

/// `WorkspaceConfiguration` schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_card: Option<PeerCardConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dream: Option<DreamConfiguration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialectic: Option<DialecticConfiguration>,
}

/// `SessionConfiguration` schema (same shape as [`WorkspaceConfiguration`]).
pub type SessionConfiguration = WorkspaceConfiguration;

/// Peer-level configuration. The spec types this as a free-form object; the
/// server (and the Python SDK) only understand `observe_me`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PeerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observe_me: Option<bool>,
}

/// `SessionPeerConfig` schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionPeerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observe_me: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observe_others: Option<bool>,
}

/// `Summary` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub content: String,
    pub message_id: String,
    pub summary_type: String,
    pub created_at: String,
    pub token_count: i64,
}

/// `SessionSummaries` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSummaries {
    pub id: String,
    #[serde(default)]
    pub short_summary: Option<Summary>,
    #[serde(default)]
    pub long_summary: Option<Summary>,
}

/// `PeerContext` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeerContext {
    pub peer_id: String,
    pub target_id: String,
    #[serde(default)]
    pub representation: Option<String>,
    #[serde(default)]
    pub peer_card: Option<Vec<String>>,
}

/// `QueueStatus` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueStatus {
    pub total_work_units: i64,
    pub completed_work_units: i64,
    pub in_progress_work_units: i64,
    pub pending_work_units: i64,
    #[serde(default)]
    pub sessions: Option<HashMap<String, SessionQueueStatus>>,
}

/// `SessionQueueStatus` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionQueueStatus {
    #[serde(default)]
    pub session_id: Option<String>,
    pub total_work_units: i64,
    pub completed_work_units: i64,
    pub in_progress_work_units: i64,
    pub pending_work_units: i64,
}

/// Backfill state for one session in a scope. The spec types this as a
/// free-form object; fields follow the Python SDK's `ScopeBackfillJob`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeBackfillJob {
    /// `pending`, `completed` or `failed`.
    pub state: String,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub docs_copied: Option<i64>,
}

/// Level of a conclusion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConclusionLevel {
    #[default]
    Explicit,
    Deductive,
    Inductive,
    Contradiction,
}

/// `Conclusion` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conclusion {
    pub id: String,
    pub content: String,
    pub observer_id: String,
    pub observed_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub level: ConclusionLevel,
    #[serde(default)]
    pub source_ids: Vec<String>,
    #[serde(default = "one")]
    pub times_derived: i64,
    pub created_at: DateTime<Utc>,
}

fn one() -> i64 {
    1
}

/// A conclusion to create through a [`ConclusionsView`](crate::ConclusionsView);
/// observer and observed come from the view.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewConclusion {
    pub content: String,
    pub session_id: Option<String>,
}

/// Reasoning effort for dialectic chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningLevel {
    Minimal,
    Low,
    Medium,
    High,
    Max,
}

/// Chat response (`DialecticResponse` schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    /// The answer. A JSON string when `response_format` was set.
    #[serde(default)]
    pub content: Option<String>,
    /// Present only when `include_evidence` was requested.
    #[serde(default)]
    pub evidence: Option<Evidence>,
}

/// One item of a streamed chat answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatChunk {
    /// A piece of the answer text.
    Content(String),
    /// Terminal event; carries evidence when it was requested.
    Done { evidence: Option<Evidence> },
}

/// `Evidence` schema: what a chat answer was built from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    #[serde(default)]
    pub conclusions: Vec<EvidenceObservation>,
    #[serde(default)]
    pub messages: Vec<EvidenceMessageRef>,
    #[serde(default)]
    pub tool_calls: Vec<EvidenceToolCall>,
    #[serde(default)]
    pub reasoning_trace_id: Option<String>,
}

/// `EvidenceObservation` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceObservation {
    pub id: String,
    pub level: ConclusionLevel,
    pub content: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub observer_id: String,
    pub observed_id: String,
    #[serde(default)]
    pub source_ids: Vec<String>,
}

/// `EvidenceMessageRef` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceMessageRef {
    pub id: String,
    pub session_id: String,
    pub peer_id: String,
    pub created_at: DateTime<Utc>,
}

/// `EvidenceToolCall` schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceToolCall {
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Metadata,
}
