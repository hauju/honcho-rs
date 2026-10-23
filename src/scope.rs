use std::collections::HashMap;

use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

use crate::error::Result;
use crate::options::ListOptions;
use crate::pagination::Page;
use crate::types::{ScopeBackfillJob, ScopeInfo};
use crate::{Honcho, Session};

/// Handle to a scope: a named set of sessions used to bound recall.
#[derive(Debug, Clone)]
pub struct Scope {
    honcho: Honcho,
    info: ScopeInfo,
}

#[derive(Deserialize)]
struct ScopeStatus {
    #[serde(default)]
    backfill_status: HashMap<String, ScopeBackfillJob>,
}

impl AsRef<str> for Scope {
    fn as_ref(&self) -> &str {
        &self.info.id
    }
}

impl Scope {
    pub(crate) fn new(honcho: Honcho, info: ScopeInfo) -> Self {
        Self { honcho, info }
    }

    pub fn id(&self) -> &str {
        &self.info.id
    }

    pub fn info(&self) -> &ScopeInfo {
        &self.info
    }

    /// Add up to 100 sessions to the scope.
    pub async fn add_sessions<I, S>(&self, sessions: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let ids: Vec<String> = sessions
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect();
        self.honcho
            .ws(Method::POST, &self.path("sessions"))
            .json(json!({ "session_ids": ids }))
            .send_empty()
            .await
    }

    pub async fn remove_session(&self, session: impl AsRef<str>) -> Result<()> {
        self.honcho
            .ws(
                Method::DELETE,
                &self.path(&format!("sessions/{}", session.as_ref())),
            )
            .send_empty()
            .await
    }

    /// Member sessions. `opts.filters` is ignored.
    pub async fn sessions(&self, opts: ListOptions) -> Result<Page<Session>> {
        let honcho = self.honcho.clone();
        self.honcho
            .list(
                Method::POST,
                &self.path("sessions/list"),
                opts,
                false,
                move |info| Session::new(honcho.clone(), info),
            )
            .await
    }

    /// Backfill state keyed by session ID.
    pub async fn status(&self) -> Result<HashMap<String, ScopeBackfillJob>> {
        let status: ScopeStatus = self
            .honcho
            .ws(Method::GET, &self.path("status"))
            .send()
            .await?;
        Ok(status.backfill_status)
    }

    fn path(&self, rest: &str) -> String {
        format!("scopes/{}/{rest}", self.id())
    }
}
