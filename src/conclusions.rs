use reqwest::Method;
use serde_json::{Map, Value, json};

use crate::Honcho;
use crate::error::{ApiError, Error, Result};
use crate::options::{ConclusionQueryOptions, ListOptions, RepresentationOptions};
use crate::pagination::Page;
use crate::peer::representation;
use crate::types::{Conclusion, NewConclusion};

/// The list endpoint's maximum page size, used to chunk `get_many`.
const LIST_PAGE_CAP: usize = 100;

/// Workspace-wide conclusion access (from [`Honcho::conclusions`]).
#[derive(Debug, Clone)]
pub struct WorkspaceConclusions {
    honcho: Honcho,
}

impl WorkspaceConclusions {
    pub(crate) fn new(honcho: Honcho) -> Self {
        Self { honcho }
    }

    pub async fn list(&self, opts: ListOptions) -> Result<Page<Conclusion>> {
        list(&self.honcho, opts).await
    }

    pub async fn get(&self, conclusion_id: &str) -> Result<Conclusion> {
        get(&self.honcho, conclusion_id).await
    }

    /// Fetch several conclusions by ID (missing IDs are skipped).
    pub async fn get_many(&self, conclusion_ids: &[&str]) -> Result<Vec<Conclusion>> {
        get_many(&self.honcho, conclusion_ids, Map::new()).await
    }
}

/// Conclusions one peer (observer) holds about another (observed), from
/// [`Peer::conclusions`](crate::Peer::conclusions) or
/// [`Peer::conclusions_of`](crate::Peer::conclusions_of).
#[derive(Debug, Clone)]
pub struct ConclusionsView {
    honcho: Honcho,
    observer: String,
    observed: String,
}

impl ConclusionsView {
    pub(crate) fn new(honcho: Honcho, observer: &str, observed: &str) -> Self {
        Self {
            honcho,
            observer: observer.to_string(),
            observed: observed.to_string(),
        }
    }

    pub fn observer(&self) -> &str {
        &self.observer
    }

    pub fn observed(&self) -> &str {
        &self.observed
    }

    /// List this pair's conclusions. Filter by session with
    /// `filters: Some(json!({"session_id": "..."}))`; observer/observed keys
    /// are always set by the view.
    pub async fn list(&self, opts: ListOptions) -> Result<Page<Conclusion>> {
        let filters = self.filters(opts.filters.clone());
        list(
            &self.honcho,
            ListOptions {
                filters: Some(filters),
                ..opts
            },
        )
        .await
    }

    /// Semantic search over this pair's conclusions.
    pub async fn query(
        &self,
        query: &str,
        opts: ConclusionQueryOptions,
    ) -> Result<Vec<Conclusion>> {
        let mut body = json!({
            "query": query,
            "top_k": opts.top_k.unwrap_or(10),
            "filters": self.filters(opts.filters),
        });
        if let Some(d) = opts.distance {
            body["distance"] = json!(d);
        }
        self.honcho
            .ws(Method::POST, "conclusions/query")
            .json(body)
            .send()
            .await
    }

    /// Fetch one conclusion; `NotFound` if it belongs to a different pair.
    pub async fn get(&self, conclusion_id: &str) -> Result<Conclusion> {
        let c = get(&self.honcho, conclusion_id).await?;
        if c.observer_id != self.observer || c.observed_id != self.observed {
            return Err(Error::NotFound(ApiError {
                status: 404,
                message: "Conclusion not found".into(),
                body: String::new(),
            }));
        }
        Ok(c)
    }

    pub async fn get_many(&self, conclusion_ids: &[&str]) -> Result<Vec<Conclusion>> {
        get_many(&self.honcho, conclusion_ids, self.pair()).await
    }

    /// Conclusions derived from `conclusion_id` (those listing it in `source_ids`).
    pub async fn derived(
        &self,
        conclusion_id: &str,
        opts: ListOptions,
    ) -> Result<Page<Conclusion>> {
        let mut filters = self.pair();
        filters.insert("source_ids".into(), json!({ "contains": conclusion_id }));
        list(
            &self.honcho,
            ListOptions {
                filters: Some(Value::Object(filters)),
                ..opts
            },
        )
        .await
    }

    /// Create conclusions for this observer/observed pair.
    pub async fn create(&self, conclusions: Vec<NewConclusion>) -> Result<Vec<Conclusion>> {
        let items: Vec<Value> = conclusions
            .into_iter()
            .map(|c| {
                let mut item = json!({
                    "content": c.content,
                    "observer_id": self.observer,
                    "observed_id": self.observed,
                });
                if let Some(s) = c.session_id {
                    item["session_id"] = json!(s);
                }
                item
            })
            .collect();
        self.honcho
            .ws(Method::POST, "conclusions")
            .json(json!({ "conclusions": items }))
            .send()
            .await
    }

    pub async fn delete(&self, conclusion_id: &str) -> Result<()> {
        self.honcho
            .ws(Method::DELETE, &format!("conclusions/{conclusion_id}"))
            .send_empty()
            .await
    }

    /// The observer's representation of the observed peer.
    /// `opts.target` is replaced by the view's observed peer.
    pub async fn representation(&self, opts: RepresentationOptions) -> Result<String> {
        let opts = RepresentationOptions {
            target: Some(self.observed.clone()),
            ..opts
        };
        representation(&self.honcho, &self.observer, opts.body()).await
    }

    fn pair(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("observer_id".into(), json!(self.observer));
        m.insert("observed_id".into(), json!(self.observed));
        m
    }

    fn filters(&self, user: Option<Value>) -> Value {
        let mut m = match user {
            Some(Value::Object(m)) => m,
            _ => Map::new(),
        };
        m.extend(self.pair());
        Value::Object(m)
    }
}

async fn list(honcho: &Honcho, opts: ListOptions) -> Result<Page<Conclusion>> {
    honcho
        .list(Method::POST, "conclusions/list", opts, true, |c| c)
        .await
}

async fn get(honcho: &Honcho, conclusion_id: &str) -> Result<Conclusion> {
    honcho
        .ws(Method::GET, &format!("conclusions/{conclusion_id}"))
        .send()
        .await
}

async fn get_many(
    honcho: &Honcho,
    ids: &[&str],
    extra_filters: Map<String, Value>,
) -> Result<Vec<Conclusion>> {
    let mut out = Vec::new();
    for chunk in ids.chunks(LIST_PAGE_CAP) {
        let mut filters = extra_filters.clone();
        filters.insert("id".into(), json!({ "in": chunk }));
        let page = list(
            honcho,
            ListOptions {
                filters: Some(Value::Object(filters)),
                page: Some(1),
                size: Some(chunk.len() as u32),
                reverse: false,
            },
        )
        .await?;
        out.extend(page.items);
    }
    Ok(out)
}
