//! The core against a real hub and node on loopback, as the app uses it: connect, list,
//! watch a held request arrive, answer it, read a session's conversation, and see a
//! refused command's reason.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use saarthi_core::{Change, CoreError, Hub, Item, Role, SessionListener, WatchListener};
use saarthi_hub::db::Db;
use saarthi_hub::live::Live;
use saarthi_hub::tokens::Tokens;
use saarthi_node::outbox::Outbox;
use saarthi_node::pending::Pending;
use saarthi_node::sessions::Sessions;
use saarthi_node::wrapper::Wrappers;
use saarthi_proto::event::v1::EventSource;

#[derive(Default)]
struct Changes(Mutex<Vec<Change>>);
impl WatchListener for Changes {
    fn on_change(&self, change: Change) {
        self.0.lock().unwrap().push(change);
    }
    fn on_error(&self, message: String) {
        panic!("watch ended: {message}");
    }
}

#[derive(Default)]
struct Conversation(Mutex<Vec<Item>>);
impl SessionListener for Conversation {
    fn on_usage(&self, _totals: Vec<saarthi_core::ModelTotals>) {}
    fn on_items(&self, items: Vec<Item>, _last_seq: u64) {
        self.0.lock().unwrap().extend(items);
    }
    fn on_error(&self, message: String) {
        panic!("stream ended: {message}");
    }
}

async fn until<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    for _ in 0..500 {
        if let Some(v) = f() {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("never {what}");
}

/// A POST to the node's hook endpoint, as Claude Code makes it; returns the response body.
async fn post_hook(addr: std::net::SocketAddr, body: String) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "POST /hook HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut reply = String::new();
    s.read_to_string(&mut reply).await.unwrap();
    reply
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_core_lists_watches_answers_and_streams() {
    let dir = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
    let hub = saarthi_hub::node_service::Hub {
        notifier: Arc::new(saarthi_hub::notify::Notifier::new(None, db.clone(), Vec::new())),
        db,
        node_tokens: Arc::new(Tokens::parse("mac t-node").unwrap()),
        client_tokens: Arc::new(Tokens::parse("phone t-phone").unwrap()),
        claude_tokens: Default::default(),
        live: Live::new(),
    };
    tokio::spawn(saarthi_hub::serve(listener, hub));

    let (outbox, rx) = Outbox::new(256);
    let sessions = Arc::new(Sessions::new(
        dir.path().into(),
        outbox.clone(),
        Duration::from_millis(20),
        Vec::new(),
    ));
    let pending = Pending::new(outbox.clone());
    let wrappers = Wrappers::new(sessions.clone(), pending.clone(), outbox.clone());
    let cfg = saarthi_node::link::Config {
        hub: format!("http://{addr}"),
        token: "t-node".into(),
        name: "mac".into(),
        buffer_cap: 1000,
    };
    let spawner = saarthi_node::spawner::Spawner::new(
        "claude".into(),
        "/bin/sh".into(),
        &[],
        None,
        Default::default(),
        sessions.clone(),
    );
    let parts = saarthi_node::link::Parts {
        sessions: sessions.clone(),
        pending: pending.clone(),
        wrappers,
        spawner,
        keep_awake: Default::default(),
    };
    tokio::spawn(saarthi_node::link::run(cfg, rx, parts));
    let hooks_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hooks = hooks_listener.local_addr().unwrap();
    let hold = saarthi_node::hooks::Hold {
        pending: pending.clone(),
        duration: Duration::from_secs(30),
        wrapped_duration: Duration::from_secs(30),
    };
    let app = saarthi_node::hooks::router(outbox.clone(), sessions.clone(), Some(hold));
    tokio::spawn(async move { axum::serve(hooks_listener, app).await });

    // A wrong token fails at connect.
    let wrong = Hub::connect(format!("http://{addr}"), "nope".into()).await;
    assert!(matches!(wrong, Err(CoreError::Hub { .. })));
    let core = Hub::connect(format!("http://{addr}"), "t-phone".into())
        .await
        .unwrap();

    // List first, then watch: the watch carries changes from when it starts.
    let mut nodes = Vec::new();
    for _ in 0..500 {
        nodes = core.nodes().await.unwrap();
        if nodes.first().is_some_and(|n| n.connected) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let node_id = nodes[0].id.clone();
    let changes = Arc::new(Changes::default());
    let _watch = core.watch(changes.clone()).await.unwrap();
    assert_eq!(core.nodes().await.unwrap()[0].name, "mac");

    // A question arrives through the watch, with its questions parsed, and is answered.
    let question = serde_json::json!({"hook_event_name": "PermissionRequest", "session_id": "s1",
        "cwd": "/w", "tool_name": "AskUserQuestion", "tool_input": {"questions": [
            {"question": "Which db?", "header": "DB", "multiSelect": false,
             "options": [{"label": "sqlite"}, {"label": "postgres"}]}]}});
    let hook = tokio::spawn(post_hook(hooks, question.to_string()));
    let held = until("the question", || {
        changes.0.lock().unwrap().iter().find_map(|c| match c {
            Change::PendingOpened { pending } => Some(pending.clone()),
            _ => None,
        })
    })
    .await;
    assert_eq!(held.summary, "Which db?");
    assert_eq!(held.questions[0].options, ["sqlite", "postgres"]);
    assert_eq!(core.pending().await.unwrap(), std::slice::from_ref(&held));
    let answers = [("Which db?".to_string(), vec!["sqlite".to_string()])].into();
    core.answer(node_id.clone(), held.id.clone(), answers)
        .await
        .unwrap();
    let reply: serde_json::Value = serde_json::from_str(&hook.await.unwrap()).unwrap();
    assert_eq!(
        reply["hookSpecificOutput"]["decision"]["updatedInput"]["answers"]["Which db?"],
        "sqlite"
    );
    until("the request closed", || {
        changes.0.lock().unwrap().iter().find_map(|c| match c {
            Change::PendingClosed { reason, .. } => Some(reason.clone()),
            _ => None,
        })
    })
    .await;

    // The conversation: a transcript line stored before the stream, and one after.
    let line = |text: &str| {
        serde_json::json!({"type": "user", "uuid": text, "message": {"role": "user", "content": text}})
            .to_string()
    };
    outbox
        .line(
            "s1",
            EventSource::Transcript,
            "user",
            line("first"),
            "first".into(),
        )
        .await;
    let convo = Arc::new(Conversation::default());
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _stream = core
        .stream_session(node_id.clone(), "s1".into(), 0, Vec::new(), convo.clone())
        .await
        .unwrap();
    outbox
        .line(
            "s1",
            EventSource::Transcript,
            "user",
            line("second"),
            "second".into(),
        )
        .await;
    let texts = until("both lines", || {
        let items = convo.0.lock().unwrap();
        let users: Vec<String> = items
            .iter()
            .filter(|i| i.role == Role::User)
            .map(|i| i.text.clone())
            .collect();
        (users.len() == 2).then_some(users)
    })
    .await;
    assert_eq!(texts, ["first", "second"]);

    // A command the node refuses comes back with its reason.
    let refused = core.interrupt(node_id, "s1".into(), None).await;
    match refused {
        Err(CoreError::Refused { reason }) => assert!(reason.contains("wrapper"), "{reason}"),
        other => panic!("{other:?}"),
    }
}
