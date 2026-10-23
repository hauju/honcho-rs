use std::time::{Duration, Instant};

use futures::TryStreamExt;
use honcho::*;
use serde_json::{Value, json};
use wiremock::matchers::{body_json, body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const WS: &str = "test-ws";
const TS: &str = "2025-01-01T00:00:00Z";

fn workspace_json() -> Value {
    json!({ "id": WS, "uid": "ws-uid", "metadata": {}, "configuration": {}, "created_at": TS })
}

fn peer_json(id: &str) -> Value {
    json!({ "id": id, "uid": format!("{id}-uid"), "workspace_id": WS, "created_at": TS,
            "metadata": {}, "configuration": {} })
}

fn session_json(id: &str) -> Value {
    json!({ "id": id, "uid": format!("{id}-uid"), "is_active": true, "workspace_id": WS,
            "metadata": {}, "configuration": {}, "created_at": TS })
}

fn message_json(id: &str, peer: &str, content: &str) -> Value {
    json!({ "id": id, "content": content, "peer_id": peer, "session_id": "s1",
            "metadata": {}, "created_at": TS, "workspace_id": WS, "token_count": 5 })
}

fn conclusion_json(id: &str, observer: &str, observed: &str) -> Value {
    json!({ "id": id, "content": "likes math", "observer_id": observer, "observed_id": observed,
            "session_id": null, "level": "explicit", "source_ids": [], "times_derived": 1,
            "created_at": TS })
}

fn ws_path(rest: &str) -> String {
    format!("/v3/workspaces/{WS}/{rest}")
}

/// Mock server with the workspace bootstrap mounted (expected exactly once).
async fn setup() -> (MockServer, Honcho) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v3/workspaces"))
        .and(body_json(json!({ "id": WS })))
        .respond_with(ResponseTemplate::new(200).set_body_json(workspace_json()))
        .expect(1)
        .named("workspace ensure")
        .mount(&server)
        .await;
    let honcho = Honcho::builder()
        .base_url(server.uri())
        .api_key("test-key")
        .workspace_id(WS)
        .build()
        .unwrap();
    (server, honcho)
}

async fn mount_peer(server: &MockServer, id: &str) {
    Mock::given(method("POST"))
        .and(path(ws_path("peers")))
        .and(body_json(json!({ "id": id })))
        .respond_with(ResponseTemplate::new(200).set_body_json(peer_json(id)))
        .mount(server)
        .await;
}

async fn mount_session(server: &MockServer, id: &str) {
    Mock::given(method("POST"))
        .and(path(ws_path("sessions")))
        .and(body_json(json!({ "id": id })))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_json(id)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn bootstrap_once_with_auth_and_host_headers() {
    let (server, honcho) = setup().await;
    let host = format!(
        "honcho-rust/{} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    Mock::given(method("POST"))
        .and(path(ws_path("peers")))
        .and(header("authorization", "Bearer test-key"))
        .and(header("x-honcho-host", host.as_str()))
        .and(body_json(json!({ "id": "alice" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(peer_json("alice")))
        .expect(2)
        .mount(&server)
        .await;
    mount_session(&server, "s1").await;

    let alice = honcho.peer("alice").await.unwrap();
    let clone = honcho.clone();
    let again = clone.peer("alice").await.unwrap();
    let session = honcho.session("s1").await.unwrap();

    assert_eq!(alice.id(), "alice");
    assert_eq!(again.info().uid.as_deref(), Some("alice-uid"));
    assert_eq!(session.id(), "s1");
    assert!(session.info().is_active);
    // `expect(1)` on the workspace mock is verified when `server` drops.
}

#[tokio::test]
async fn add_messages_body_shape() {
    let (server, honcho) = setup().await;
    mount_peer(&server, "alice").await;
    mount_peer(&server, "tutor").await;
    mount_session(&server, "s1").await;
    Mock::given(method("POST"))
        .and(path(ws_path("sessions/s1/messages")))
        .and(body_json(json!({ "messages": [
            { "content": "hi", "peer_id": "alice", "metadata": { "k": "v" },
              "created_at": "2025-01-01T00:00:00Z" },
            { "content": "hello", "peer_id": "tutor" },
        ]})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([
            message_json("m1", "alice", "hi"),
            message_json("m2", "tutor", "hello"),
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let alice = honcho.peer("alice").await.unwrap();
    let tutor = honcho.peer("tutor").await.unwrap();
    let session = honcho.session("s1").await.unwrap();
    let mut meta = Metadata::new();
    meta.insert("k".into(), json!("v"));
    let msgs = session
        .add_messages(vec![
            alice
                .message("hi")
                .metadata(meta)
                .created_at(TS.parse().unwrap()),
            tutor.message("hello"),
        ])
        .await
        .unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].peer_id, "tutor");
}

#[tokio::test]
async fn pagination_next_page_and_stream() {
    let (server, honcho) = setup().await;
    for (page, ids) in [(1, ["a", "b"]), (2, ["c", "d"])] {
        Mock::given(method("POST"))
            .and(path(ws_path("peers/list")))
            .and(query_param("page", page.to_string()))
            .and(query_param("size", "2"))
            .and(query_param("reverse", "true"))
            .and(body_json(json!({ "filters": { "metadata": { "x": 1 } } })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": ids.iter().map(|i| peer_json(i)).collect::<Vec<_>>(),
                "page": page, "size": 2, "total": 4, "pages": 2,
            })))
            .mount(&server)
            .await;
    }

    let opts = ListOptions {
        filters: Some(json!({ "metadata": { "x": 1 } })),
        size: Some(2),
        reverse: true,
        ..Default::default()
    };
    let first = honcho.peers(opts.clone()).await.unwrap();
    assert_eq!((first.page, first.pages, first.total), (1, 2, 4));
    assert!(first.has_next_page());
    let second = first.next_page().await.unwrap().unwrap();
    assert_eq!(second.items[0].id(), "c");
    assert!(second.next_page().await.unwrap().is_none());

    let all: Vec<String> = honcho
        .peers(opts)
        .await
        .unwrap()
        .into_stream()
        .map_ok(|p| p.id().to_string())
        .try_collect()
        .await
        .unwrap();
    assert_eq!(all, ["a", "b", "c", "d"]);
}

#[tokio::test]
async fn error_401_is_not_retried() {
    let (server, honcho) = setup().await;
    Mock::given(method("POST"))
        .and(path(ws_path("peers")))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "detail": "bad key" })))
        .expect(1)
        .mount(&server)
        .await;
    let err = honcho.peer("alice").await.unwrap_err();
    match err {
        Error::Authentication(e) => {
            assert_eq!(e.status, 401);
            assert_eq!(e.message, "bad key");
            assert!(e.body.contains("bad key"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn error_404_maps_to_not_found() {
    let (server, honcho) = setup().await;
    mount_session(&server, "s1").await;
    Mock::given(method("GET"))
        .and(path(ws_path("sessions/s1/messages/nope")))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({ "detail": "Message not found" })),
        )
        .mount(&server)
        .await;
    let session = honcho.session("s1").await.unwrap();
    let err = session.get_message("nope").await.unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
    assert_eq!(err.status(), Some(404));
    assert_eq!(err.api_error().unwrap().message, "Message not found");
}

#[tokio::test]
async fn rate_limit_honors_retry_after_then_succeeds() {
    let (server, honcho) = setup().await;
    Mock::given(method("POST"))
        .and(path(ws_path("peers")))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_json(json!({ "detail": "slow down" })),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    mount_peer(&server, "alice").await;

    let start = Instant::now();
    let alice = honcho.peer("alice").await.unwrap();
    assert_eq!(alice.id(), "alice");
    // Retry-After: 0 must win over the 500ms default backoff.
    assert!(start.elapsed() < Duration::from_millis(400));
}

#[tokio::test]
async fn server_errors_exhaust_retries() {
    let (server, honcho) = setup().await;
    Mock::given(method("POST"))
        .and(path(ws_path("peers")))
        .respond_with(ResponseTemplate::new(503).set_body_string("unavailable"))
        .expect(3) // initial attempt + 2 retries
        .mount(&server)
        .await;
    let err = honcho.peer("alice").await.unwrap_err();
    match err {
        Error::Server(e) => {
            assert_eq!(e.status, 503);
            assert_eq!(e.message, "HTTP 503");
            assert_eq!(e.body, "unavailable");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn peer_chat_non_stream() {
    let (server, honcho) = setup().await;
    mount_peer(&server, "alice").await;
    Mock::given(method("POST"))
        .and(path(ws_path("peers/alice/chat")))
        .and(body_json(json!({
            "query": "what does alice like?",
            "stream": false,
            "target": "bob",
            "session_id": "s1",
            "reasoning_level": "medium",
            "include_evidence": true,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": "Math.",
            "evidence": {
                "conclusions": [{ "id": "c1", "level": "deductive", "content": "likes math",
                    "created_at": TS, "session_id": "s1", "observer_id": "alice",
                    "observed_id": "bob", "source_ids": ["c0"] }],
                "messages": [{ "id": "m1", "session_id": "s1", "peer_id": "bob", "created_at": TS }],
                "tool_calls": [{ "tool_name": "search", "tool_input": { "q": "math" } }],
                "reasoning_trace_id": null,
            },
        })))
        .mount(&server)
        .await;

    let alice = honcho.peer("alice").await.unwrap();
    let resp = alice
        .chat(
            "what does alice like?",
            ChatOptions {
                target: Some("bob".into()),
                session: Some("s1".into()),
                reasoning_level: Some(ReasoningLevel::Medium),
                include_evidence: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(resp.content.as_deref(), Some("Math."));
    let ev = resp.evidence.unwrap();
    assert_eq!(ev.conclusions[0].level, ConclusionLevel::Deductive);
    assert_eq!(ev.tool_calls[0].tool_name, "search");
}

#[tokio::test]
async fn workspace_chat_body_omits_peer_only_fields() {
    let (server, honcho) = setup().await;
    Mock::given(method("POST"))
        .and(path(ws_path("chat")))
        .and(body_json(
            json!({ "query": "q", "stream": false, "scope": ["team"] }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "content": null })))
        .mount(&server)
        .await;
    let resp = honcho
        .chat(
            "q",
            ChatOptions {
                target: Some("ignored".into()),
                scope: Some(vec!["team".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(resp.content, None);
}

#[tokio::test]
async fn peer_chat_stream_parses_sse() {
    let (server, honcho) = setup().await;
    mount_peer(&server, "alice").await;
    let sse = concat!(
        ": keep-alive comment\n",
        "data: {\"delta\": {\"content\": \"Hel\"}, \"done\": false}\n\n",
        "data: {\"delta\": {\"content\": \"\"}, \"done\": false}\r\n\r\n",
        "data: not json\n\n",
        "data:{\"delta\": {\"content\": \"lo ü\"}, \"done\": false}\n\n",
        "data: {\"delta\": {}, \"done\": true, \"evidence\": {\"conclusions\": [], \"messages\": [], \"tool_calls\": []}}\n\n",
        "data: {\"delta\": {\"content\": \"after done\"}, \"done\": false}\n\n",
    );
    Mock::given(method("POST"))
        .and(path(ws_path("peers/alice/chat")))
        .and(body_json(json!({ "query": "hi", "stream": true })))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .mount(&server)
        .await;

    let alice = honcho.peer("alice").await.unwrap();
    let chunks: Vec<ChatChunk> = alice
        .chat_stream("hi", ChatOptions::default())
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        chunks,
        vec![
            ChatChunk::Content("Hel".into()),
            ChatChunk::Content("lo ü".into()),
            ChatChunk::Done {
                evidence: Some(Evidence::default())
            },
        ]
    );
}

#[tokio::test]
async fn chat_stream_error_status_surfaces_before_streaming() {
    let (server, honcho) = setup().await;
    mount_peer(&server, "alice").await;
    Mock::given(method("POST"))
        .and(path(ws_path("peers/alice/chat")))
        .respond_with(
            ResponseTemplate::new(422).set_body_json(json!({ "detail": [{ "msg": "bad" }] })),
        )
        .mount(&server)
        .await;
    let alice = honcho.peer("alice").await.unwrap();
    let err = alice
        .chat_stream("hi", ChatOptions::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(err, Error::UnprocessableEntity(_)), "{err:?}");
}

#[tokio::test]
async fn session_context_query_and_conversions() {
    let (server, honcho) = setup().await;
    mount_session(&server, "s1").await;
    Mock::given(method("GET"))
        .and(path(ws_path("sessions/s1/context")))
        .and(query_param("summary", "true"))
        .and(query_param("tokens", "10000"))
        .and(query_param("limit_to_session", "false"))
        .and(query_param("peer_target", "alice"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "s1",
            "messages": [
                message_json("m1", "alice", "Can you help with math?"),
                message_json("m2", "tutor", "Sure!"),
            ],
            "summary": { "content": "Alice asked for help.", "message_id": "m0",
                         "summary_type": "short", "created_at": TS, "token_count": 4 },
            "peer_representation": "Alice is a student.",
            "peer_card": ["Name: Alice", "it's \"quoted\"", "it's"],
        })))
        .mount(&server)
        .await;

    let session = honcho.session("s1").await.unwrap();
    let ctx = session
        .context(ContextOptions {
            summary: Some(true),
            tokens: Some(10_000),
            peer_target: Some("alice".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ctx.len(), 3);

    let card = r#"<peer_card>['Name: Alice', 'it\'s "quoted"', "it's"]</peer_card>"#;
    assert_eq!(
        ctx.to_openai("tutor"),
        vec![
            json!({ "role": "system", "content": "<peer_representation>Alice is a student.</peer_representation>" }),
            json!({ "role": "system", "content": card }),
            json!({ "role": "system", "content": "<summary>Alice asked for help.</summary>" }),
            json!({ "role": "user", "name": "alice", "content": "Can you help with math?" }),
            json!({ "role": "assistant", "name": "tutor", "content": "Sure!" }),
        ]
    );
    assert_eq!(
        ctx.to_anthropic("tutor"),
        vec![
            json!({ "role": "user", "content": "<peer_representation>Alice is a student.</peer_representation>" }),
            json!({ "role": "user", "content": card }),
            json!({ "role": "user", "content": "<summary>Alice asked for help.</summary>" }),
            json!({ "role": "user", "content": "alice: Can you help with math?" }),
            json!({ "role": "assistant", "content": "Sure!" }),
        ]
    );
}

#[tokio::test]
async fn upload_file_multipart() {
    let (server, honcho) = setup().await;
    mount_session(&server, "s1").await;
    Mock::given(method("POST"))
        .and(path(ws_path("sessions/s1/messages/upload")))
        .and(header("authorization", "Bearer test-key"))
        .and(body_string_contains(
            "name=\"file\"; filename=\"notes.txt\"",
        ))
        .and(body_string_contains("Content-Type: text/plain"))
        .and(body_string_contains("hello from a file"))
        .and(body_string_contains("name=\"peer_id\"\r\n\r\nalice"))
        .and(body_string_contains(
            "name=\"metadata\"\r\n\r\n{\"src\":\"upload\"}",
        ))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!([message_json(
                "m1",
                "alice",
                "hello from a file"
            )])),
        )
        .expect(1)
        .mount(&server)
        .await;

    let session = honcho.session("s1").await.unwrap();
    let mut meta = Metadata::new();
    meta.insert("src".into(), json!("upload"));
    let msgs = session
        .upload_file(FileUpload {
            filename: "notes.txt".into(),
            content_type: "text/plain".into(),
            bytes: "hello from a file".into(),
            peer_id: "alice".into(),
            metadata: Some(meta),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(msgs[0].content, "hello from a file");

    let reqs: Vec<Request> = server.received_requests().await.unwrap();
    let upload = reqs
        .iter()
        .find(|r| r.url.path().ends_with("/upload"))
        .unwrap();
    let ct = upload
        .headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.starts_with("multipart/form-data; boundary="), "{ct}");
}

#[tokio::test]
async fn conclusions_view_scopes_filters_and_creates() {
    let (server, honcho) = setup().await;
    mount_peer(&server, "alice").await;
    Mock::given(method("POST"))
        .and(path(ws_path("conclusions/list")))
        .and(body_json(json!({ "filters": {
            "session_id": "s1", "observer_id": "alice", "observed_id": "bob" } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [conclusion_json("c1", "alice", "bob")],
            "page": 1, "size": 50, "total": 1, "pages": 1,
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(ws_path("conclusions")))
        .and(body_json(json!({ "conclusions": [
            { "content": "likes math", "observer_id": "alice", "observed_id": "bob", "session_id": "s1" },
        ]})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!([conclusion_json("c2", "alice", "bob")])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ws_path("conclusions/c3")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(conclusion_json("c3", "carol", "bob")),
        )
        .mount(&server)
        .await;

    let alice = honcho.peer("alice").await.unwrap();
    let view = alice.conclusions_of("bob");
    let page = view
        .list(ListOptions {
            filters: Some(json!({ "session_id": "s1", "observer_id": "spoofed" })),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.items[0].id, "c1");

    let created = view
        .create(vec![NewConclusion {
            content: "likes math".into(),
            session_id: Some("s1".into()),
        }])
        .await
        .unwrap();
    assert_eq!(created[0].id, "c2");

    let err = view.get("c3").await.unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
}
