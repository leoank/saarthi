//! Questions a spawned session asks the person (ADR-0061). A `claude -p` session has no
//! `AskUserQuestion` ([[EXP-20261016-rq1404]], [[EXP-20261117-de4f82]]), so the node gives
//! each session it spawns a tool of its own, `ask_user_question`, from a small MCP server on
//! its hook port, as learnharness does for its sessions: the call is held as a pending
//! question, which clients show and answer as they do `AskUserQuestion`'s, and the answers
//! come back as the tool's result, the session's turn going on in the same process. The
//! answer goes out as an event stream with keepalives, so no HTTP client gives up on a
//! person who takes their time. Each session reaches it with a bearer token of its own.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::pending::{Pending, QUESTION_TOOL};

/// The tool's name as the session sees it: server `saarthi`, tool `ask_user_question`.
pub const TOOL: &str = "mcp__saarthi__ask_user_question";
const NAME: &str = "ask_user_question";
/// How long a question may wait for the person, and Claude Code's MCP tool timeouts with it.
pub const HOLD: Duration = Duration::from_secs(24 * 3600);
const KEEPALIVE: Duration = Duration::from_secs(20);

/// What the session is told, appended to its system prompt.
pub const PROMPT: &str = "The person running this session is on another device and answers \
through saarthi. AskUserQuestion is not available in this session: whenever you would ask \
with AskUserQuestion, call the mcp__saarthi__ask_user_question tool instead (load it with \
ToolSearch first if it is deferred), with the same input: one to four questions, each with \
a short header, two to four options with a label and a description, and multiSelect. It \
returns the person's answers when they have answered, which can take a while; do not ask \
in plain text what this tool can ask.";

/// The Claude Code environment for a session with the tool: its MCP calls may wait as long
/// as a question does.
pub fn env() -> [(&'static str, String); 2] {
    let ms = HOLD.as_millis().to_string();
    [("MCP_TOOL_TIMEOUT", ms.clone()), ("CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT", ms)]
}

/// The bearer tokens of the sessions that may ask, by token.
#[derive(Default)]
pub struct Questions {
    tokens: Mutex<HashMap<String, String>>,
    url: std::sync::OnceLock<String>,
}

impl Questions {
    /// Where sessions reach the tool: `http://<hook address>/mcp`.
    pub fn set_url(&self, url: String) {
        let _ = self.url.set(url);
    }

    /// A new token for session `id`, and the `--mcp-config` that hands it to claude; none
    /// while the node has no URL (no hub link to answer through).
    pub async fn register(&self, id: &str) -> Option<String> {
        let url = self.url.get()?;
        let token = uuid::Uuid::new_v4().simple().to_string();
        self.tokens.lock().await.insert(token.clone(), id.to_string());
        let server = json!({"type": "http", "url": url, "headers": {"Authorization": format!("Bearer {token}")}});
        Some(json!({"mcpServers": {"saarthi": server}}).to_string())
    }

    /// Session `id` is gone: its token stops working.
    pub async fn forget(&self, id: &str) {
        self.tokens.lock().await.retain(|_, s| s != id);
    }

    async fn session(&self, headers: &HeaderMap) -> Option<String> {
        let token = headers
            .get(header::AUTHORIZATION)?
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")?;
        self.tokens.lock().await.get(token).cloned()
    }
}

#[derive(Clone)]
struct Mcp {
    questions: Arc<Questions>,
    pending: Arc<Pending>,
}

/// `/mcp`: the MCP endpoint (streamable HTTP; no server-initiated stream, so GET is refused).
pub fn router(questions: Arc<Questions>, pending: Arc<Pending>) -> Router {
    Router::new()
        .route("/mcp", post(mcp).get(|| async { StatusCode::METHOD_NOT_ALLOWED }))
        .with_state(Mcp { questions, pending })
}

/// The tool, with `AskUserQuestion`'s input so clients show it as they show that one.
fn tool() -> Value {
    json!({
        "name": NAME,
        "description": "Ask the person running this session one to four questions and wait for their answers. Use it wherever you would use AskUserQuestion: to settle a choice, clarify what they want, or interview them. Each question offers two to four options; the person may also answer in their own words.",
        "inputSchema": {
            "type": "object",
            "required": ["questions"],
            "properties": {"questions": {
                "type": "array", "minItems": 1, "maxItems": 4,
                "items": {
                    "type": "object",
                    "required": ["question", "header", "options", "multiSelect"],
                    "properties": {
                        "question": {"type": "string", "description": "The question, ending with a question mark."},
                        "header": {"type": "string", "description": "A short label for it, at most 12 characters."},
                        "multiSelect": {"type": "boolean", "description": "Whether more than one option may be chosen."},
                        "options": {
                            "type": "array", "minItems": 2, "maxItems": 4,
                            "items": {
                                "type": "object",
                                "required": ["label", "description"],
                                "properties": {
                                    "label": {"type": "string", "description": "The option, in one to five words."},
                                    "description": {"type": "string", "description": "What choosing it means."}
                                }
                            }
                        }
                    }
                }
            }}
        }
    })
}

fn rpc(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn json_response(v: Value) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], v.to_string()).into_response()
}

fn text(t: String, error: bool) -> Value {
    json!({"content": [{"type": "text", "text": t}], "isError": error})
}

async fn mcp(State(m): State<Mcp>, headers: HeaderMap, body: String) -> Response {
    let Some(session) = m.questions.session(&headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(msg) = serde_json::from_str::<Value>(&body) else {
        return json_response(rpc_error(&Value::Null, -32700, "not JSON"));
    };
    let id = msg["id"].clone();
    let method = msg["method"].as_str().unwrap_or_default();
    if id.is_null() {
        // A notification (`notifications/initialized`) or a response: nothing to answer.
        return StatusCode::ACCEPTED.into_response();
    }
    match method {
        "initialize" => {
            let version = msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18");
            json_response(rpc(&id, json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "saarthi", "version": env!("CARGO_PKG_VERSION")},
            })))
        }
        "ping" => json_response(rpc(&id, json!({}))),
        "tools/list" => json_response(rpc(&id, json!({"tools": [tool()]}))),
        "tools/call" if msg["params"]["name"] == NAME => {
            let input = msg["params"]["arguments"].clone();
            if input["questions"].as_array().is_none_or(|q| q.is_empty()) {
                return json_response(rpc(&id, text("No questions to ask: pass one to four in `questions`.".into(), true)));
            }
            ask(m.pending, session, id, input)
        }
        "tools/call" => json_response(rpc(&id, text(format!("No such tool: {}", msg["params"]["name"]), true))),
        _ => json_response(rpc_error(&id, -32601, "method not found")),
    }
}

/// Holds the question until the person answers, sending keepalives meanwhile; the session
/// going away (the client dropping the stream) releases it.
fn ask(pending: Arc<Pending>, session: String, id: Value, input: Value) -> Response {
    let (tx, rx) = mpsc::channel::<Result<String, Infallible>>(4);
    tokio::spawn(async move {
        let hook = json!({"session_id": session, "tool_name": QUESTION_TOOL, "tool_input": input});
        let hold = pending.hold(&hook, HOLD);
        tokio::pin!(hold);
        let mut tick = tokio::time::interval(KEEPALIVE);
        tick.tick().await;
        loop {
            tokio::select! {
                released = &mut hold => {
                    let reply = rpc(&id, outcome(&released));
                    let _ = tx.send(Ok(format!("event: message\ndata: {reply}\n\n"))).await;
                    return;
                }
                _ = tick.tick() => {
                    if tx.send(Ok(": waiting for the person\n\n".into())).await.is_err() {
                        return;
                    }
                }
                _ = tx.closed() => return,
            }
        }
    });
    (
        [(header::CONTENT_TYPE, "text/event-stream"), (header::CACHE_CONTROL, "no-cache")],
        Body::from_stream(ReceiverStream::new(rx)),
    )
        .into_response()
}

/// The tool's result from how the hold ended: the hook-shaped response `Pending` gives.
fn outcome(released: &Value) -> Value {
    let decision = &released["hookSpecificOutput"]["decision"];
    match decision["behavior"].as_str() {
        Some("allow") => {
            let answers = &decision["updatedInput"]["answers"];
            let mut out = String::from("The person answered:\n");
            for (question, answer) in answers.as_object().into_iter().flatten() {
                let answer = match answer {
                    Value::Array(a) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; "),
                    other => other.as_str().unwrap_or_default().to_string(),
                };
                out.push_str(&format!("- {question}\n  {answer}\n"));
            }
            text(out, false)
        }
        Some("deny") => text(
            format!(
                "The person declined to answer{}",
                decision["message"].as_str().filter(|m| !m.is_empty()).map(|m| format!(": {m}")).unwrap_or_default()
            ),
            false,
        ),
        _ => text("No answer came: the question was released without one.".into(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_become_the_tools_result() {
        let released = json!({"hookSpecificOutput": {"decision": {"behavior": "allow", "updatedInput": {
            "questions": [], "answers": {"Which tea?": "Sencha", "Measures?": ["taste", "colour"]}}}}});
        let r = outcome(&released);
        let t = r["content"][0]["text"].as_str().unwrap();
        assert!(t.contains("- Which tea?\n  Sencha") && t.contains("- Measures?\n  taste; colour"), "{t}");
        assert_eq!(r["isError"], false);
        let denied = outcome(&json!({"hookSpecificOutput": {"decision": {"behavior": "deny", "message": "later"}}}));
        assert_eq!(denied["content"][0]["text"], "The person declined to answer: later");
        assert_eq!(outcome(&json!({}))["isError"], true);
    }
}
