use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{self, Stream, StreamExt, TryStreamExt};
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::Honcho;
use crate::error::Result;
use crate::options::ListOptions;

type Fetch<T> = Arc<dyn Fn(u32) -> BoxFuture<'static, Result<Page<T>>> + Send + Sync>;

/// One page of a paginated list.
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub size: u32,
    pub total: u64,
    pub pages: u32,
    fetch: Fetch<T>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for Page<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("items", &self.items)
            .field("page", &self.page)
            .field("size", &self.size)
            .field("total", &self.total)
            .field("pages", &self.pages)
            .finish()
    }
}

#[derive(Deserialize)]
struct RawPage<R> {
    items: Vec<R>,
    page: u32,
    size: u32,
    total: u64,
    pages: u32,
}

#[derive(Clone)]
pub(crate) struct ListRequest {
    method: Method,
    path: String,
    ensure_workspace: bool,
    body: Option<Value>,
    first_page: u32,
    size: u32,
    reverse: bool,
}

impl ListRequest {
    pub(crate) fn new(
        method: Method,
        path: String,
        ensure_workspace: bool,
        opts: ListOptions,
        send_filters: bool,
    ) -> Self {
        let body = opts
            .filters
            .filter(|_| send_filters)
            .map(|f| json!({ "filters": f }));
        Self {
            method,
            path,
            ensure_workspace,
            body,
            first_page: opts.page.unwrap_or(1),
            size: opts.size.unwrap_or(50),
            reverse: opts.reverse,
        }
    }
}

impl<T: Send + 'static> Page<T> {
    /// Whether another page exists after this one.
    pub fn has_next_page(&self) -> bool {
        self.page < self.pages
    }

    /// Fetch the next page, or `None` if this is the last one.
    pub async fn next_page(&self) -> Result<Option<Page<T>>> {
        if !self.has_next_page() {
            return Ok(None);
        }
        (self.fetch)(self.page + 1).await.map(Some)
    }

    /// Stream every item from this page onwards, fetching pages lazily.
    pub fn into_stream(self) -> impl Stream<Item = Result<T>> + Send {
        enum State<T> {
            Ready(Page<T>),
            Fetch(Fetch<T>, u32),
            Done,
        }
        stream::try_unfold(State::Ready(self), |state| async move {
            let page = match state {
                State::Ready(page) => page,
                State::Fetch(fetch, n) => fetch(n).await?,
                State::Done => return Ok::<_, crate::Error>(None),
            };
            let next = if page.has_next_page() {
                State::Fetch(page.fetch, page.page + 1)
            } else {
                State::Done
            };
            Ok(Some((stream::iter(page.items).map(Ok), next)))
        })
        .try_flatten()
    }

    pub(crate) async fn fetch<R, F>(honcho: Honcho, req: ListRequest, map: F) -> Result<Self>
    where
        R: DeserializeOwned + Send + 'static,
        F: Fn(R) -> T + Send + Sync + 'static,
    {
        let page = req.first_page;
        load(Arc::new((honcho, req, map)), page).await
    }
}

fn load<R, T, F>(
    ctx: Arc<(Honcho, ListRequest, F)>,
    page: u32,
) -> BoxFuture<'static, Result<Page<T>>>
where
    R: DeserializeOwned + Send + 'static,
    T: Send + 'static,
    F: Fn(R) -> T + Send + Sync + 'static,
{
    Box::pin(async move {
        let (honcho, req, map) = &*ctx;
        let mut call = honcho
            .call(req.method.clone(), req.path.clone())
            .query("page", page)
            .query("size", req.size)
            .json_opt(req.body.clone());
        if req.reverse {
            call = call.query("reverse", "true");
        }
        if req.ensure_workspace {
            call = call.ensure_workspace();
        }
        let raw: RawPage<R> = call.send().await?;
        let next_ctx = ctx.clone();
        Ok(Page {
            items: raw.items.into_iter().map(map).collect(),
            page: raw.page,
            size: raw.size,
            total: raw.total,
            pages: raw.pages,
            fetch: Arc::new(move |n| load(next_ctx.clone(), n)),
        })
    })
}
