//! Server-sent-events parsing for streamed chat answers.

use std::collections::VecDeque;
use std::pin::Pin;

use bytes::Bytes;
use futures::stream::{self, BoxStream, Stream, StreamExt};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::types::ChatChunk;

/// Stream of chat chunks returned by `chat_stream`.
pub type ChatStream = BoxStream<'static, Result<ChatChunk>>;

type ByteStream = Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send>>;

struct State {
    bytes: ByteStream,
    buf: Vec<u8>,
    pending: VecDeque<ChatChunk>,
    done: bool,
}

pub(crate) fn chat_stream(resp: reqwest::Response) -> ChatStream {
    let state = State {
        bytes: Box::pin(resp.bytes_stream()),
        buf: Vec::new(),
        pending: VecDeque::new(),
        done: false,
    };
    stream::unfold(state, |mut st| async move {
        loop {
            if let Some(chunk) = st.pending.pop_front() {
                return Some((Ok(chunk), st));
            }
            if st.done {
                return None;
            }
            match st.bytes.next().await {
                Some(Ok(bytes)) => {
                    st.buf.extend_from_slice(&bytes);
                    while !st.done {
                        let Some(pos) = st.buf.iter().position(|b| *b == b'\n') else {
                            break;
                        };
                        let line: Vec<u8> = st.buf.drain(..=pos).collect();
                        st.handle_line(&line);
                    }
                }
                Some(Err(e)) => {
                    st.done = true;
                    return Some((Err(Error::from_transport(e)), st));
                }
                None => {
                    let rest = std::mem::take(&mut st.buf);
                    st.handle_line(&rest);
                    st.done = true;
                }
            }
        }
    })
    .boxed()
}

impl State {
    /// Handles one `data: {...}` line: `{"delta": {"content": "..."}}` yields
    /// content, `{"done": true, "evidence": ...}` ends the stream.
    fn handle_line(&mut self, line: &[u8]) {
        let line = String::from_utf8_lossy(line);
        let line = line.trim_end_matches(['\n', '\r']);
        let Some(data) = line.strip_prefix("data:") else {
            return;
        };
        let Ok(Value::Object(event)) = serde_json::from_str::<Value>(data.trim_start_matches(' '))
        else {
            return;
        };
        if event.get("done").and_then(Value::as_bool).unwrap_or(false) {
            let evidence = event
                .get("evidence")
                .filter(|v| v.is_object())
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            self.pending.push_back(ChatChunk::Done { evidence });
            self.done = true;
            return;
        }
        if let Some(content) = event
            .get("delta")
            .and_then(|d| d.get("content"))
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
        {
            self.pending
                .push_back(ChatChunk::Content(content.to_string()));
        }
    }
}
