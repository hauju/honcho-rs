use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::types::{Message, Summary};

/// Prompt-ready session context (`SessionContext` schema).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionContext {
    /// Session ID.
    pub id: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub summary: Option<Summary>,
    #[serde(default)]
    pub peer_representation: Option<String>,
    #[serde(default)]
    pub peer_card: Option<Vec<String>>,
}

impl SessionContext {
    /// OpenAI chat messages: system messages for representation, card and
    /// summary, then `assistant`/`user` turns with `name` set to the peer ID.
    pub fn to_openai(&self, assistant: impl AsRef<str>) -> Vec<Value> {
        let assistant = assistant.as_ref();
        let mut out = self.preamble("system");
        out.extend(self.messages.iter().map(|m| {
            let role = if m.peer_id == assistant {
                "assistant"
            } else {
                "user"
            };
            json!({ "role": role, "name": m.peer_id, "content": m.content })
        }));
        out
    }

    /// Anthropic messages: representation, card and summary as `user` turns,
    /// then turns where non-assistant content is prefixed with `<peer_id>: `.
    pub fn to_anthropic(&self, assistant: impl AsRef<str>) -> Vec<Value> {
        let assistant = assistant.as_ref();
        let mut out = self.preamble("user");
        out.extend(self.messages.iter().map(|m| {
            if m.peer_id == assistant {
                json!({ "role": "assistant", "content": m.content })
            } else {
                json!({ "role": "user", "content": format!("{}: {}", m.peer_id, m.content) })
            }
        }));
        out
    }

    /// Number of messages plus one if a summary is present.
    pub fn len(&self) -> usize {
        self.messages.len() + usize::from(self.summary.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn preamble(&self, role: &str) -> Vec<Value> {
        let mut out = Vec::new();
        if let Some(r) = self
            .peer_representation
            .as_deref()
            .filter(|r| !r.is_empty())
        {
            out.push(json!({ "role": role, "content": format!("<peer_representation>{r}</peer_representation>") }));
        }
        if let Some(card) = self.peer_card.as_ref().filter(|c| !c.is_empty()) {
            out.push(json!({ "role": role, "content": format!("<peer_card>{}</peer_card>", py_list_repr(card)) }));
        }
        if let Some(s) = &self.summary {
            out.push(
                json!({ "role": role, "content": format!("<summary>{}</summary>", s.content) }),
            );
        }
        out
    }
}

/// Formats a list the way Python's `str(list[str])` does (`['a', "b's"]`),
/// so the output matches the Python SDK byte for byte.
fn py_list_repr(items: &[String]) -> String {
    let parts: Vec<String> = items.iter().map(|s| py_str_repr(s)).collect();
    format!("[{}]", parts.join(", "))
}

fn py_str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7f..0xa0).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
