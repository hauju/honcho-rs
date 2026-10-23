use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

use crate::error::Result;
use crate::options::{
    ChatOptions, ListOptions, PeerContextOptions, RepresentationOptions, SearchOptions,
};
use crate::pagination::Page;
use crate::sse::{ChatStream, chat_stream};
use crate::types::{
    ChatResponse, Message, MessageCreate, Metadata, PeerConfig, PeerContext, PeerInfo,
};
use crate::{ConclusionsView, Honcho, Session};

/// Handle to a peer (a user, agent or any other participant).
#[derive(Debug, Clone)]
pub struct Peer {
    honcho: Honcho,
    info: PeerInfo,
}

#[derive(Deserialize)]
pub(crate) struct PeerCardResponse {
    #[serde(default)]
    peer_card: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub(crate) struct RepresentationResponse {
    pub(crate) representation: String,
}

impl AsRef<str> for Peer {
    fn as_ref(&self) -> &str {
        &self.info.id
    }
}

impl Peer {
    pub(crate) fn new(honcho: Honcho, info: PeerInfo) -> Self {
        Self { honcho, info }
    }

    pub fn id(&self) -> &str {
        &self.info.id
    }

    /// Server data as of the last fetch (creation, listing or [`Peer::refresh`]).
    pub fn info(&self) -> &PeerInfo {
        &self.info
    }

    /// A message authored by this peer, ready for [`Session::add_messages`].
    pub fn message(&self, content: impl Into<String>) -> MessageCreate {
        MessageCreate {
            content: content.into(),
            peer_id: self.info.id.clone(),
            ..Default::default()
        }
    }

    /// Ask Honcho what this peer knows (dialectic chat).
    pub async fn chat(&self, query: &str, opts: ChatOptions) -> Result<ChatResponse> {
        self.honcho
            .ws(Method::POST, &self.path("chat"))
            .json(opts.body(query, false, true))
            .timeout(opts.timeout)
            .send()
            .await
    }

    /// Streaming variant of [`Peer::chat`].
    pub async fn chat_stream(&self, query: &str, opts: ChatOptions) -> Result<ChatStream> {
        let resp = self
            .honcho
            .ws(Method::POST, &self.path("chat"))
            .json(opts.body(query, true, true))
            .timeout(opts.timeout)
            .send_raw()
            .await?;
        Ok(chat_stream(resp))
    }

    /// Sessions this peer is a member of.
    pub async fn sessions(&self, opts: ListOptions) -> Result<Page<Session>> {
        let honcho = self.honcho.clone();
        self.honcho
            .list(
                Method::POST,
                &self.path("sessions"),
                opts,
                true,
                move |info| Session::new(honcho.clone(), info),
            )
            .await
    }

    /// Search this peer's messages. `opts.scope` is ignored.
    pub async fn search(&self, query: &str, opts: SearchOptions) -> Result<Vec<Message>> {
        self.honcho
            .ws(Method::POST, &self.path("search"))
            .json(opts.body(query, false))
            .send()
            .await
    }

    /// This peer's card, or its card about `target`.
    pub async fn get_card(&self, target: Option<&str>) -> Result<Option<Vec<String>>> {
        let resp: PeerCardResponse = self
            .honcho
            .ws(Method::GET, &self.path("card"))
            .query_opt("target", target)
            .send()
            .await?;
        Ok(resp.peer_card)
    }

    /// Replace this peer's card (or its card about `target`).
    pub async fn set_card(
        &self,
        peer_card: Vec<String>,
        target: Option<&str>,
    ) -> Result<Option<Vec<String>>> {
        let resp: PeerCardResponse = self
            .honcho
            .ws(Method::PUT, &self.path("card"))
            .query_opt("target", target)
            .json(json!({ "peer_card": peer_card }))
            .send()
            .await?;
        Ok(resp.peer_card)
    }

    /// Text representation of what this peer knows (about itself or `opts.target`).
    pub async fn representation(&self, opts: RepresentationOptions) -> Result<String> {
        representation(&self.honcho, self.id(), opts.body()).await
    }

    /// Representation plus peer card in one call.
    pub async fn context(&self, opts: PeerContextOptions) -> Result<PeerContext> {
        self.honcho
            .ws(Method::GET, &self.path("context"))
            .query_opt("target", opts.target)
            .query_opt("search_query", opts.search_query)
            .query_opt("search_top_k", opts.search_top_k)
            .query_opt("search_max_distance", opts.search_max_distance)
            .query_opt("include_most_frequent", opts.include_most_frequent)
            .query_opt("max_conclusions", opts.max_conclusions)
            .send()
            .await
    }

    /// Conclusions this peer has drawn about itself.
    pub fn conclusions(&self) -> ConclusionsView {
        ConclusionsView::new(self.honcho.clone(), self.id(), self.id())
    }

    /// Conclusions this peer has drawn about `target`.
    pub fn conclusions_of(&self, target: impl AsRef<str>) -> ConclusionsView {
        ConclusionsView::new(self.honcho.clone(), self.id(), target.as_ref())
    }

    pub async fn get_metadata(&self) -> Result<Metadata> {
        Ok(self.fetch().await?.metadata)
    }

    pub async fn set_metadata(&self, metadata: Metadata) -> Result<()> {
        self.update(json!({ "metadata": metadata })).await
    }

    pub async fn get_configuration(&self) -> Result<PeerConfig> {
        Ok(self.fetch().await?.configuration)
    }

    pub async fn set_configuration(&self, configuration: PeerConfig) -> Result<()> {
        self.update(json!({ "configuration": configuration })).await
    }

    /// Re-fetch this peer's metadata and configuration.
    pub async fn refresh(&mut self) -> Result<()> {
        self.info = self.fetch().await?;
        Ok(())
    }

    async fn fetch(&self) -> Result<PeerInfo> {
        self.honcho
            .ws(Method::POST, "peers")
            .json(json!({ "id": self.id() }))
            .send()
            .await
    }

    async fn update(&self, body: serde_json::Value) -> Result<()> {
        self.honcho
            .ws(Method::PUT, &format!("peers/{}", self.id()))
            .json(body)
            .send_empty()
            .await
    }

    fn path(&self, rest: &str) -> String {
        format!("peers/{}/{rest}", self.id())
    }
}

pub(crate) async fn representation(
    honcho: &Honcho,
    peer_id: &str,
    body: serde_json::Value,
) -> Result<String> {
    let resp: RepresentationResponse = honcho
        .ws(Method::POST, &format!("peers/{peer_id}/representation"))
        .json(body)
        .send()
        .await?;
    Ok(resp.representation)
}
