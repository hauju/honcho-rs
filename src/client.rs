use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

use crate::error::{Error, Result};
use crate::options::{ChatOptions, ListOptions, QueueStatusOptions, SearchOptions};
use crate::pagination::{ListRequest, Page};
use crate::sse::{ChatStream, chat_stream};
use crate::types::{
    ChatResponse, Message, Metadata, PeerInfo, QueueStatus, ScopeInfo, SessionInfo,
    WorkspaceConfiguration, WorkspaceInfo,
};
use crate::{Peer, Scope, Session, WorkspaceConclusions};

const DEFAULT_BASE_URL: &str = "https://api.honcho.dev";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RETRIES: u32 = 2;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(500);
const RETRY_STATUS_CODES: [u16; 5] = [429, 500, 502, 503, 504];

/// Honcho API client, scoped to one workspace.
///
/// Cheap to clone: all clones share one connection pool and one
/// workspace-bootstrap state.
#[derive(Clone)]
pub struct Honcho {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    base_url: String,
    workspace_id: String,
    workspace_ensured: OnceCell<()>,
}

impl std::fmt::Debug for Honcho {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Honcho")
            .field("base_url", &self.inner.base_url)
            .field("workspace_id", &self.inner.workspace_id)
            .finish()
    }
}

/// Builder for [`Honcho`]. Unset values fall back to `HONCHO_API_KEY`,
/// `HONCHO_URL` and `HONCHO_WORKSPACE_ID`, then to the defaults
/// (`https://api.honcho.dev`, workspace `"default"`, 60s timeout).
#[derive(Debug, Default)]
pub struct HonchoBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    workspace_id: Option<String>,
    timeout: Option<Duration>,
    headers: HeaderMap,
}

impl HonchoBuilder {
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// API base URL, e.g. `http://localhost:8000` for self-hosted Honcho.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn workspace_id(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }

    /// Per-request timeout (default 60s).
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Extra headers sent with every request. They override the SDK defaults
    /// (e.g. `X-Honcho-Host`).
    pub fn default_headers(mut self, headers: HeaderMap) -> Self {
        self.headers.extend(headers);
        self
    }

    pub fn build(self) -> Result<Honcho> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let api_key = self.api_key.or_else(|| env("HONCHO_API_KEY"));
        let base_url = self
            .base_url
            .or_else(|| env("HONCHO_URL"))
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let workspace_id = self
            .workspace_id
            .or_else(|| env("HONCHO_WORKSPACE_ID"))
            .unwrap_or_else(|| "default".to_string());

        let mut headers = HeaderMap::new();
        let host = format!(
            "honcho-rust/{} ({})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS
        );
        headers.insert(
            HeaderName::from_static("x-honcho-host"),
            HeaderValue::from_str(&host).map_err(|e| Error::Config(e.to_string()))?,
        );
        if let Some(key) = api_key {
            let mut value = HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| Error::Config("API key is not a valid header value".into()))?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        headers.extend(self.headers);

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(self.timeout.unwrap_or(DEFAULT_TIMEOUT))
            .build()
            .map_err(|e| Error::Config(e.to_string()))?;

        Ok(Honcho {
            inner: Arc::new(Inner {
                http,
                base_url: base_url.trim_end_matches('/').to_string(),
                workspace_id,
                workspace_ensured: OnceCell::new(),
            }),
        })
    }
}

impl Honcho {
    pub fn builder() -> HonchoBuilder {
        HonchoBuilder::default()
    }

    /// Client configured purely from `HONCHO_*` environment variables.
    pub fn from_env() -> Result<Self> {
        Self::builder().build()
    }

    pub fn workspace_id(&self) -> &str {
        &self.inner.workspace_id
    }

    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    /// Get or create a peer.
    pub async fn peer(&self, id: impl Into<String>) -> Result<Peer> {
        let info: PeerInfo = self
            .ws(Method::POST, "peers")
            .json(json!({ "id": id.into() }))
            .send()
            .await?;
        Ok(Peer::new(self.clone(), info))
    }

    /// List peers in the workspace.
    pub async fn peers(&self, opts: ListOptions) -> Result<Page<Peer>> {
        let honcho = self.clone();
        self.list(Method::POST, "peers/list", opts, true, move |info| {
            Peer::new(honcho.clone(), info)
        })
        .await
    }

    /// Get or create a session.
    pub async fn session(&self, id: impl Into<String>) -> Result<Session> {
        let info: SessionInfo = self
            .ws(Method::POST, "sessions")
            .json(json!({ "id": id.into() }))
            .send()
            .await?;
        Ok(Session::new(self.clone(), info))
    }

    /// List sessions in the workspace.
    pub async fn sessions(&self, opts: ListOptions) -> Result<Page<Session>> {
        let honcho = self.clone();
        self.list(Method::POST, "sessions/list", opts, true, move |info| {
            Session::new(honcho.clone(), info)
        })
        .await
    }

    /// Get or create a scope.
    pub async fn scope(&self, id: impl Into<String>) -> Result<Scope> {
        let info: ScopeInfo = self
            .ws(Method::POST, "scopes")
            .json(json!({ "id": id.into() }))
            .send()
            .await?;
        Ok(Scope::new(self.clone(), info))
    }

    /// Fetch an existing scope (404 if it does not exist).
    pub async fn get_scope(&self, id: &str) -> Result<Scope> {
        let info: ScopeInfo = self.ws(Method::GET, &format!("scopes/{id}")).send().await?;
        Ok(Scope::new(self.clone(), info))
    }

    /// List scopes. `opts.filters` is ignored (the endpoint takes no filters).
    pub async fn scopes(&self, opts: ListOptions) -> Result<Page<Scope>> {
        let honcho = self.clone();
        self.list(Method::POST, "scopes/list", opts, false, move |info| {
            Scope::new(honcho.clone(), info)
        })
        .await
    }

    /// List all workspaces the API key can access.
    pub async fn workspaces(&self, opts: ListOptions) -> Result<Page<WorkspaceInfo>> {
        Page::fetch(
            self.clone(),
            ListRequest::new(
                Method::POST,
                "/v3/workspaces/list".into(),
                false,
                opts,
                true,
            ),
            |w| w,
        )
        .await
    }

    /// Delete a workspace by ID.
    pub async fn delete_workspace(&self, workspace_id: &str) -> Result<()> {
        self.call(Method::DELETE, format!("/v3/workspaces/{workspace_id}"))
            .send_empty()
            .await
    }

    /// Workspace-level dialectic chat. `target` and `sessions` in `opts` are
    /// ignored (the workspace endpoint does not accept them).
    pub async fn chat(&self, query: &str, opts: ChatOptions) -> Result<ChatResponse> {
        self.ws(Method::POST, "chat")
            .json(opts.body(query, false, false))
            .timeout(opts.timeout)
            .send()
            .await
    }

    /// Streaming variant of [`Honcho::chat`].
    pub async fn chat_stream(&self, query: &str, opts: ChatOptions) -> Result<ChatStream> {
        let resp = self
            .ws(Method::POST, "chat")
            .json(opts.body(query, true, false))
            .timeout(opts.timeout)
            .send_raw()
            .await?;
        Ok(chat_stream(resp))
    }

    /// Search messages across the workspace.
    pub async fn search(&self, query: &str, opts: SearchOptions) -> Result<Vec<Message>> {
        self.ws(Method::POST, "search")
            .json(opts.body(query, true))
            .send()
            .await
    }

    /// Workspace-level conclusions (list / get / get_many).
    pub fn conclusions(&self) -> WorkspaceConclusions {
        WorkspaceConclusions::new(self.clone())
    }

    /// Background processing queue status.
    pub async fn queue_status(&self, opts: QueueStatusOptions) -> Result<QueueStatus> {
        let mut call = self.ws(Method::GET, "queue/status");
        call = call.query_opt("observer_id", opts.observer);
        call = call.query_opt("sender_id", opts.sender);
        call = call.query_opt("session_id", opts.session);
        call.send().await
    }

    /// Schedule an `omni` dream for `observer` (observing `observed`, which
    /// defaults to `observer`), optionally within one session.
    pub async fn schedule_dream(
        &self,
        observer: &str,
        session: Option<&str>,
        observed: Option<&str>,
    ) -> Result<()> {
        self.ws(Method::POST, "schedule_dream")
            .json(json!({
                "observer": observer,
                "observed": observed.unwrap_or(observer),
                "session_id": session,
                "dream_type": "omni",
            }))
            .send_empty()
            .await
    }

    /// Fresh workspace metadata (get-or-create).
    pub async fn get_metadata(&self) -> Result<Metadata> {
        Ok(self.fetch_workspace().await?.metadata)
    }

    pub async fn set_metadata(&self, metadata: Metadata) -> Result<()> {
        self.update_workspace(json!({ "metadata": metadata })).await
    }

    /// Fresh workspace configuration (get-or-create).
    pub async fn get_configuration(&self) -> Result<WorkspaceConfiguration> {
        Ok(self.fetch_workspace().await?.configuration)
    }

    pub async fn set_configuration(&self, configuration: WorkspaceConfiguration) -> Result<()> {
        self.update_workspace(json!({ "configuration": configuration }))
            .await
    }

    async fn fetch_workspace(&self) -> Result<WorkspaceInfo> {
        self.call(Method::POST, "/v3/workspaces".into())
            .json(json!({ "id": self.workspace_id() }))
            .send()
            .await
    }

    async fn update_workspace(&self, body: Value) -> Result<()> {
        self.call(
            Method::PUT,
            format!("/v3/workspaces/{}", self.workspace_id()),
        )
        .json(body)
        .send_empty()
        .await
    }

    /// POSTs the workspace once per client (get-or-create), like the Python SDK.
    async fn ensure_workspace(&self) -> Result<()> {
        self.inner
            .workspace_ensured
            .get_or_try_init(|| async {
                // Direct `execute` (not `Call`) to keep `Call::send_raw` non-recursive.
                let url = self.url("/v3/workspaces");
                let body = json!({ "id": self.workspace_id() });
                self.execute(|http| http.post(&url).json(&body)).await?;
                Ok::<_, Error>(())
            })
            .await?;
        Ok(())
    }

    pub(crate) async fn list<R, T>(
        &self,
        method: Method,
        suffix: &str,
        opts: ListOptions,
        send_filters: bool,
        map: impl Fn(R) -> T + Send + Sync + 'static,
    ) -> Result<Page<T>>
    where
        R: DeserializeOwned + Send + 'static,
        T: Send + 'static,
    {
        let path = format!("/v3/workspaces/{}/{suffix}", self.workspace_id());
        Page::fetch(
            self.clone(),
            ListRequest::new(method, path, true, opts, send_filters),
            map,
        )
        .await
    }

    /// Request against `/v3/workspaces/{workspace_id}/{suffix}`, ensuring the
    /// workspace exists first.
    pub(crate) fn ws(&self, method: Method, suffix: &str) -> Call<'_> {
        self.call(
            method,
            format!("/v3/workspaces/{}/{suffix}", self.workspace_id()),
        )
        .ensure_workspace()
    }

    pub(crate) fn call(&self, method: Method, path: String) -> Call<'_> {
        Call {
            honcho: self,
            method,
            path,
            query: Vec::new(),
            body: None,
            timeout: None,
            ensure: false,
        }
    }

    /// Sends a request, retrying 429/5xx and transport errors with exponential
    /// backoff (500ms, 1s) or the server's `Retry-After`.
    pub(crate) async fn execute(
        &self,
        make: impl Fn(&reqwest::Client) -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            let backoff = INITIAL_RETRY_DELAY * 2u32.pow(attempt);
            let can_retry = attempt < MAX_RETRIES;
            match make(&self.inner.http).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(resp),
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let retry_after = parse_retry_after(resp.headers());
                    if can_retry && RETRY_STATUS_CODES.contains(&status) {
                        tokio::time::sleep(retry_after.unwrap_or(backoff)).await;
                        attempt += 1;
                        continue;
                    }
                    let body = resp.text().await.unwrap_or_default();
                    return Err(Error::from_response(status, body, retry_after));
                }
                Err(e) if can_retry && (e.is_timeout() || e.is_connect() || e.is_request()) => {
                    tokio::time::sleep(backoff).await;
                    attempt += 1;
                }
                Err(e) => return Err(Error::from_transport(e)),
            }
        }
    }

    /// Multipart POST; `form` is rebuilt for every attempt.
    pub(crate) async fn upload<T: DeserializeOwned>(
        &self,
        path: &str,
        form: impl Fn() -> reqwest::multipart::Form,
    ) -> Result<T> {
        self.ensure_workspace().await?;
        let url = self.url(path);
        decode(
            self.execute(|http| http.post(&url).multipart(form()))
                .await?,
        )
        .await
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.inner.base_url)
    }
}

/// Internal request builder.
pub(crate) struct Call<'a> {
    honcho: &'a Honcho,
    method: Method,
    path: String,
    query: Vec<(&'static str, String)>,
    body: Option<Value>,
    timeout: Option<Duration>,
    ensure: bool,
}

impl Call<'_> {
    pub(crate) fn query(mut self, key: &'static str, value: impl ToString) -> Self {
        self.query.push((key, value.to_string()));
        self
    }

    pub(crate) fn query_opt(self, key: &'static str, value: Option<impl ToString>) -> Self {
        match value {
            Some(v) => self.query(key, v),
            None => self,
        }
    }

    pub(crate) fn json(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    pub(crate) fn json_opt(mut self, body: Option<Value>) -> Self {
        self.body = body;
        self
    }

    pub(crate) fn ensure_workspace(mut self) -> Self {
        self.ensure = true;
        self
    }

    pub(crate) fn timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    pub(crate) async fn send_raw(self) -> Result<reqwest::Response> {
        if self.ensure {
            self.honcho.ensure_workspace().await?;
        }
        let url = self.honcho.url(&self.path);
        self.honcho
            .execute(|http| {
                let mut req = http.request(self.method.clone(), &url);
                if !self.query.is_empty() {
                    req = req.query(&self.query);
                }
                if let Some(body) = &self.body {
                    req = req.json(body);
                }
                if let Some(t) = self.timeout {
                    req = req.timeout(t);
                }
                req
            })
            .await
    }

    pub(crate) async fn send<T: DeserializeOwned>(self) -> Result<T> {
        decode(self.send_raw().await?).await
    }

    pub(crate) async fn send_empty(self) -> Result<()> {
        self.send_raw().await?;
        Ok(())
    }
}

async fn decode<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    let bytes = resp.bytes().await.map_err(Error::from_transport)?;
    let bytes: &[u8] = if bytes.is_empty() { b"null" } else { &bytes };
    Ok(serde_json::from_slice(bytes)?)
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(secs) = value.parse::<f64>() {
        return Duration::try_from_secs_f64(secs.max(0.0)).ok();
    }
    let at = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let delta = at.signed_duration_since(chrono::Utc::now());
    Some(delta.to_std().unwrap_or(Duration::ZERO))
}
