use reqwest::Method;
use reqwest::multipart::{Form, Part};
use serde_json::{Value, json};

use crate::error::Result;
use crate::options::{
    ContextOptions, FileUpload, ListOptions, QueueStatusOptions, RepresentationOptions,
    SearchOptions,
};
use crate::pagination::Page;
use crate::peer::representation;
use crate::types::{
    Message, MessageCreate, Metadata, QueueStatus, SessionConfiguration, SessionInfo,
    SessionPeerConfig, SessionSummaries,
};
use crate::{Honcho, Peer, SessionContext};

/// Handle to a session (a conversation between peers).
#[derive(Debug, Clone)]
pub struct Session {
    honcho: Honcho,
    info: SessionInfo,
}

impl AsRef<str> for Session {
    fn as_ref(&self) -> &str {
        &self.info.id
    }
}

impl Session {
    pub(crate) fn new(honcho: Honcho, info: SessionInfo) -> Self {
        Self { honcho, info }
    }

    pub fn id(&self) -> &str {
        &self.info.id
    }

    /// Server data as of the last fetch (creation, listing or [`Session::refresh`]).
    pub fn info(&self) -> &SessionInfo {
        &self.info
    }

    /// Add peers (by ID or [`Peer`]) with default session config.
    pub async fn add_peers<I, P>(&self, peers: I) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<str>,
    {
        self.peers_call(Method::POST, peer_config_map(peers)).await
    }

    /// Replace the session's peers.
    pub async fn set_peers<I, P>(&self, peers: I) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<str>,
    {
        self.peers_call(Method::PUT, peer_config_map(peers)).await
    }

    pub async fn remove_peers<I, P>(&self, peers: I) -> Result<()>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<str>,
    {
        let ids: Vec<String> = peers.into_iter().map(|p| p.as_ref().to_string()).collect();
        self.peers_call(Method::DELETE, json!(ids)).await
    }

    /// Peers in this session (paginated, 50 per page).
    pub async fn peers(&self) -> Result<Page<Peer>> {
        let honcho = self.honcho.clone();
        self.honcho
            .list(
                Method::GET,
                &self.path("peers"),
                ListOptions::default(),
                false,
                move |info| Peer::new(honcho.clone(), info),
            )
            .await
    }

    pub async fn get_peer_configuration(&self, peer: impl AsRef<str>) -> Result<SessionPeerConfig> {
        self.honcho
            .ws(Method::GET, &self.peer_config_path(peer.as_ref()))
            .send()
            .await
    }

    pub async fn set_peer_configuration(
        &self,
        peer: impl AsRef<str>,
        configuration: SessionPeerConfig,
    ) -> Result<()> {
        self.honcho
            .ws(Method::PUT, &self.peer_config_path(peer.as_ref()))
            .json(json!(configuration))
            .send_empty()
            .await
    }

    /// Add messages to the session; returns the stored messages.
    pub async fn add_messages(&self, messages: Vec<MessageCreate>) -> Result<Vec<Message>> {
        self.honcho
            .ws(Method::POST, &self.path("messages"))
            .json(json!({ "messages": messages }))
            .send()
            .await
    }

    pub async fn messages(&self, opts: ListOptions) -> Result<Page<Message>> {
        self.honcho
            .list(Method::POST, &self.path("messages/list"), opts, true, |m| m)
            .await
    }

    pub async fn get_message(&self, message_id: &str) -> Result<Message> {
        self.honcho
            .ws(Method::GET, &self.path(&format!("messages/{message_id}")))
            .send()
            .await
    }

    /// Replace a message's metadata.
    pub async fn update_message(&self, message_id: &str, metadata: Metadata) -> Result<Message> {
        self.honcho
            .ws(Method::PUT, &self.path(&format!("messages/{message_id}")))
            .json(json!({ "metadata": metadata }))
            .send()
            .await
    }

    /// Prompt-ready context: recent messages, summary and optional peer view.
    pub async fn context(&self, opts: ContextOptions) -> Result<SessionContext> {
        let mut call = self
            .honcho
            .ws(Method::GET, &self.path("context"))
            .query_opt("summary", opts.summary)
            .query("limit_to_session", opts.limit_to_session)
            .query_opt("tokens", opts.tokens)
            .query_opt("search_query", opts.search_query)
            .query_opt("peer_target", opts.peer_target)
            .query_opt("peer_perspective", opts.peer_perspective)
            .query_opt("scope", opts.scope)
            .query_opt("search_top_k", opts.search_top_k)
            .query_opt("search_max_distance", opts.search_max_distance)
            .query_opt("include_most_frequent", opts.include_most_frequent)
            .query_opt("max_conclusions", opts.max_conclusions);
        for s in opts.sessions.into_iter().flatten() {
            call = call.query("sessions", s);
        }
        call.send().await
    }

    pub async fn summaries(&self) -> Result<SessionSummaries> {
        self.honcho
            .ws(Method::GET, &self.path("summaries"))
            .send()
            .await
    }

    /// Search this session's messages. `opts.scope` is ignored.
    pub async fn search(&self, query: &str, opts: SearchOptions) -> Result<Vec<Message>> {
        self.honcho
            .ws(Method::POST, &self.path("search"))
            .json(opts.body(query, false))
            .send()
            .await
    }

    /// Upload a file (PDF, text, JSON, ...) that the server splits into messages.
    pub async fn upload_file(&self, file: FileUpload) -> Result<Vec<Message>> {
        let path = format!(
            "/v3/workspaces/{}/{}",
            self.honcho.workspace_id(),
            self.path("messages/upload")
        );
        let metadata = file.metadata.as_ref().map(|m| json!(m).to_string());
        let configuration = file.configuration.as_ref().map(|c| json!(c).to_string());
        let created_at = file.created_at.map(|t| t.to_rfc3339());
        let form = move || -> Result<Form> {
            let part = Part::stream(reqwest::Body::from(file.bytes.clone()))
                .file_name(file.filename.clone())
                .mime_str(&file.content_type)
                .map_err(|e| crate::Error::Config(format!("invalid content type: {e}")))?;
            let mut form = Form::new()
                .part("file", part)
                .text("peer_id", file.peer_id.clone());
            for (k, v) in [
                ("metadata", &metadata),
                ("configuration", &configuration),
                ("created_at", &created_at),
            ] {
                if let Some(v) = v {
                    form = form.text(k, v.clone());
                }
            }
            Ok(form)
        };
        form()?; // validate the MIME type once up front
        self.honcho
            .upload(&path, || form().expect("validated above"))
            .await
    }

    /// `peer`'s representation within this session.
    pub async fn representation(
        &self,
        peer: impl AsRef<str>,
        opts: RepresentationOptions,
    ) -> Result<String> {
        let opts = RepresentationOptions {
            session: Some(self.id().to_string()),
            ..opts
        };
        representation(&self.honcho, peer.as_ref(), opts.body()).await
    }

    /// Queue status for this session. `opts.session` is ignored.
    pub async fn queue_status(&self, opts: QueueStatusOptions) -> Result<QueueStatus> {
        self.honcho
            .queue_status(QueueStatusOptions {
                session: Some(self.id().to_string()),
                ..opts
            })
            .await
    }

    /// Copy this session (optionally only up to `message_id`) into a new one.
    /// Named `clone_session` to avoid shadowing [`Clone::clone`].
    pub async fn clone_session(&self, message_id: Option<&str>) -> Result<Session> {
        let info: SessionInfo = self
            .honcho
            .ws(Method::POST, &self.path("clone"))
            .query_opt("message_id", message_id)
            .send()
            .await?;
        Ok(Session::new(self.honcho.clone(), info))
    }

    pub async fn delete(&self) -> Result<()> {
        self.honcho
            .ws(Method::DELETE, &self.base_path())
            .send_empty()
            .await
    }

    pub async fn get_metadata(&self) -> Result<Metadata> {
        Ok(self.fetch().await?.metadata)
    }

    pub async fn set_metadata(&self, metadata: Metadata) -> Result<()> {
        self.update(json!({ "metadata": metadata })).await
    }

    pub async fn get_configuration(&self) -> Result<SessionConfiguration> {
        Ok(self.fetch().await?.configuration)
    }

    pub async fn set_configuration(&self, configuration: SessionConfiguration) -> Result<()> {
        self.update(json!({ "configuration": configuration })).await
    }

    /// Re-fetch this session's metadata and configuration.
    pub async fn refresh(&mut self) -> Result<()> {
        self.info = self.fetch().await?;
        Ok(())
    }

    async fn fetch(&self) -> Result<SessionInfo> {
        self.honcho
            .ws(Method::POST, "sessions")
            .json(json!({ "id": self.id() }))
            .send()
            .await
    }

    async fn update(&self, body: Value) -> Result<()> {
        self.honcho
            .ws(Method::PUT, &self.base_path())
            .json(body)
            .send_empty()
            .await
    }

    async fn peers_call(&self, method: Method, body: Value) -> Result<()> {
        self.honcho
            .ws(method, &self.path("peers"))
            .json(body)
            .send_empty()
            .await
    }

    fn base_path(&self) -> String {
        format!("sessions/{}", self.id())
    }

    fn path(&self, rest: &str) -> String {
        format!("sessions/{}/{rest}", self.id())
    }

    fn peer_config_path(&self, peer_id: &str) -> String {
        self.path(&format!("peers/{peer_id}/config"))
    }
}

fn peer_config_map<I, P>(peers: I) -> Value
where
    I: IntoIterator<Item = P>,
    P: AsRef<str>,
{
    Value::Object(
        peers
            .into_iter()
            .map(|p| (p.as_ref().to_string(), json!({})))
            .collect(),
    )
}
