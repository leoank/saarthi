//! The Claude API proxy of a session that must not hold its token (ADR-0050, ADR-0051,
//! [[EXP-20261110-g5mh89]], [[EXP-20261114-d82rcz]]): claude holds only a dummy and sends
//! its API requests as plain HTTP over a unix socket (`ANTHROPIC_UNIX_SOCKET`, with
//! `ANTHROPIC_BASE_URL` set to an `http://` URL). The proxy sends each one on:
//!
//! - under one of the hub's tokens, to the hub over the node's connection (`ClaudeApi`),
//!   which adds the token, so its value never reaches the node;
//! - under a sandboxed profile's own `token_file`, straight to Anthropic over TLS with the
//!   token added here.
//!
//! The answer streams back as it arrives. Only `/v1/` is forwarded: the inference API Claude
//! Code reaches through its base URL. Its other calls (settings, policy limits, the account)
//! go straight to Anthropic with the dummy and are refused there; a session runs without
//! them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};
use saarthi_proto::node::v1::node_service_client::NodeServiceClient;
use saarthi_proto::node::v1::{ClaudeApiHead, ClaudeApiRequest, HttpHeader};
use tokio_stream::StreamExt;
use tonic::transport::Channel;

/// Where a node-held token's requests go.
pub const UPSTREAM: &str = "https://api.anthropic.com";

/// What `CLAUDE_CODE_OAUTH_TOKEN` holds in a proxied session: never a credential.
pub const DUMMY_TOKEN: &str = "saarthi-proxied";

/// What `ANTHROPIC_BASE_URL` holds: plain HTTP over the socket, the TLS done outside.
pub const BASE_URL: &str = "http://api.anthropic.com";

/// A request body larger than this is refused rather than held in memory.
const MAX_BODY: usize = 64 << 20;

/// A request body goes to the hub in pieces of at most this.
const PIECE: usize = 1 << 20;

/// The environment a proxied claude gets: the dummy token and the socket.
pub fn env(socket: &Path) -> [(&'static str, String); 3] {
    [
        ("CLAUDE_CODE_OAUTH_TOKEN", DUMMY_TOKEN.to_string()),
        ("ANTHROPIC_UNIX_SOCKET", socket.to_string_lossy().into_owned()),
        ("ANTHROPIC_BASE_URL", BASE_URL.to_string()),
    ]
}

/// Where the proxies of sessions on the host and of terminal sessions listen: a directory
/// only this user can enter, beside the wrappers' socket directory, and denied to every
/// sandbox.
pub fn host_socket_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    // SAFETY: getuid only reads.
    base.join(format!("saarthi-{}-api", unsafe { libc::getuid() }))
}

/// Makes [`host_socket_dir`] if missing, for this user only; the node does at start, so a
/// sandbox can be denied it.
pub fn make_host_socket_dir() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dir = host_socket_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}

/// A new socket path in [`host_socket_dir`], which is made if missing.
pub fn host_socket() -> std::io::Result<PathBuf> {
    Ok(make_host_socket_dir()?.join(socket_name()))
}

/// `claude-api-<8 random hex>.sock`: one per session, since sessions may run under
/// different tokens.
pub fn socket_name() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("claude-api-{}.sock", &id[..8])
}

/// The hub, for sessions under its tokens: its address and this node's token, with one
/// connection kept for every session's requests and made again after a failure.
pub struct HubApi {
    hub: String,
    token: String,
    channel: tokio::sync::Mutex<Option<Channel>>,
}

impl HubApi {
    pub fn new(hub: String, token: String) -> Arc<Self> {
        Arc::new(Self {
            hub,
            token,
            channel: Default::default(),
        })
    }

    async fn client(&self) -> Result<NodeServiceClient<Channel>, String> {
        let mut kept = self.channel.lock().await;
        if kept.is_none() {
            *kept = Some(crate::link::dial(&self.hub).await.map_err(|e| format!("{e:#}"))?);
        }
        let channel = kept.clone().expect("just made");
        Ok(NodeServiceClient::new(channel).max_decoding_message_size(16 << 20))
    }

    async fn forget(&self) {
        *self.channel.lock().await = None;
    }
}

/// Where a proxy sends its requests.
pub enum Upstream {
    /// To the hub, under the token it holds by this name (ADR-0051).
    Hub { api: Arc<HubApi>, name: String },
    /// To `base` with a token the node holds, a sandboxed profile's `token_file` (ADR-0050).
    Direct { base: String, token: String },
}

/// One proxy, serving until dropped; the socket file goes with it.
pub struct Proxy {
    path: PathBuf,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
enum Route {
    Hub {
        api: Arc<HubApi>,
        name: String,
    },
    Direct {
        base: String,
        bearer: String,
        client: reqwest::Client,
    },
}

impl Proxy {
    /// Serves on a new socket at `path` (any stale file there is replaced), sending every
    /// request to `upstream`. Needs a tokio runtime.
    pub fn start(path: &Path, upstream: Upstream) -> std::io::Result<Proxy> {
        // The usual limit for a unix socket path is 104 bytes, its terminator included.
        if path.as_os_str().len() > 103 {
            return Err(std::io::Error::other(format!(
                "the socket path {} is too long for a unix socket; use a shorter state_dir",
                path.display()
            )));
        }
        let route = match upstream {
            Upstream::Hub { api, name } => Route::Hub { api, name },
            Upstream::Direct { base, token } => Route::Direct {
                base: base.trim_end_matches('/').to_string(),
                bearer: format!("Bearer {}", token.trim()),
                client: reqwest::Client::builder()
                    .connect_timeout(std::time::Duration::from_secs(10))
                    .build()
                    .map_err(std::io::Error::other)?,
            },
        };
        let _ = std::fs::remove_file(path);
        let listener = tokio::net::UnixListener::bind(path)?;
        let app = axum::Router::new().fallback(forward).with_state(route);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Proxy { path: path.to_path_buf(), task })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Headers that belong to one hop, or that the proxy sets itself.
fn passes(name: &HeaderName) -> bool {
    !matches!(
        name.as_str(),
        "host"
            | "authorization"
            | "x-api-key"
            | "connection"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
    )
}

fn copy(from: &HeaderMap, to: &mut HeaderMap) {
    for (name, value) in from {
        if passes(name) {
            to.append(name.clone(), value.clone());
        }
    }
}

async fn forward(State(route): State<Route>, req: Request) -> Response {
    let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/").to_string();
    if !path.starts_with("/v1/") {
        tracing::warn!(path = %path, "refused a session's API request outside /v1/");
        return (StatusCode::FORBIDDEN, "saarthi forwards only the Claude API's /v1/ to Anthropic").into_response();
    }
    let method = req.method().clone();
    let (parts, body) = req.into_parts();
    let body = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::PAYLOAD_TOO_LARGE, format!("the request body: {e}")).into_response(),
    };
    let mut headers = HeaderMap::new();
    copy(&parts.headers, &mut headers);
    match route {
        Route::Hub { api, name } => via_hub(&api, &name, method, path, headers, body).await,
        Route::Direct { base, bearer, client } => {
            headers.insert(header::AUTHORIZATION, bearer.parse().expect("checked when the token was read"));
            let sent = client.request(method.clone(), format!("{base}{path}")).headers(headers).body(body).send().await;
            let resp = match sent {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "the Claude API could not be reached");
                    return (StatusCode::BAD_GATEWAY, format!("saarthi could not reach the Claude API: {e}"))
                        .into_response();
                }
            };
            tracing::debug!(%method, path = %path, status = %resp.status(), "proxied a Claude API request");
            let mut out = Response::builder().status(resp.status());
            copy(resp.headers(), out.headers_mut().expect("a fresh builder"));
            // Streamed as it arrives: a turn's answer is server-sent events.
            out.body(Body::from_stream(resp.bytes_stream()))
                .unwrap_or_else(|e| (StatusCode::BAD_GATEWAY, e.to_string()).into_response())
        }
    }
}

/// One request through the hub's `ClaudeApi`: the head and the body in pieces, then the
/// answer's head and its body as it streams back. A hub that cannot be reached is a 502,
/// which claude retries; one that refuses the token or the request a 403.
async fn via_hub(
    api: &HubApi,
    name: &str,
    method: axum::http::Method,
    path: String,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let mut head = Some(ClaudeApiHead {
        claude_token: name.to_string(),
        method: method.to_string(),
        path: path.clone(),
        headers: headers
            .iter()
            .map(|(n, v)| HttpHeader {
                name: n.as_str().to_string(),
                value: v.as_bytes().to_vec(),
            })
            .collect(),
    });
    let mut pieces: Vec<ClaudeApiRequest> = body
        .chunks(PIECE)
        .map(|c| ClaudeApiRequest {
            head: head.take(),
            body: c.to_vec(),
        })
        .collect();
    if let Some(head) = head {
        pieces.push(ClaudeApiRequest {
            head: Some(head),
            body: Vec::new(),
        });
    }
    let bad_gateway = |why: String| {
        tracing::warn!(path = %path, error = %why, "the hub did not relay a Claude API request");
        (StatusCode::BAD_GATEWAY, format!("saarthi: the hub did not relay the request: {why}")).into_response()
    };
    let mut client = match api.client().await {
        Ok(c) => c,
        Err(e) => return bad_gateway(e),
    };
    let mut request = tonic::Request::new(tokio_stream::iter(pieces));
    match format!("Bearer {}", api.token).parse() {
        Ok(v) => request.metadata_mut().insert("authorization", v),
        Err(_) => return bad_gateway("the node token is not valid in a header".into()),
    };
    let mut stream = match client.claude_api(request).await {
        Ok(r) => r.into_inner(),
        Err(status) => {
            use tonic::Code;
            if matches!(
                status.code(),
                Code::NotFound | Code::PermissionDenied | Code::Unauthenticated | Code::InvalidArgument
            ) {
                tracing::warn!(path = %path, error = %status.message(), "the hub refused a Claude API request");
                return (StatusCode::FORBIDDEN, format!("saarthi: {}", status.message())).into_response();
            }
            api.forget().await;
            return bad_gateway(status.message().to_string());
        }
    };
    let first = match stream.message().await {
        Ok(Some(m)) => m,
        Ok(None) => return bad_gateway("the hub sent no answer".into()),
        Err(status) => {
            api.forget().await;
            return bad_gateway(status.message().to_string());
        }
    };
    let Some(status) = first.head else {
        return bad_gateway("the hub's answer had no head".into());
    };
    tracing::debug!(%method, path = %path, status = status.status, "relayed a Claude API request through the hub");
    let mut out = Response::builder().status(status.status as u16);
    let hs = out.headers_mut().expect("a fresh builder");
    for h in status.headers {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(h.name.as_bytes()),
            axum::http::HeaderValue::from_bytes(&h.value),
        ) && passes(&n)
        {
            hs.append(n, v);
        }
    }
    // A hub that goes away mid-answer breaks the stream, and claude retries the request.
    let body = stream.map(|m| m.map(|m| axum::body::Bytes::from(m.body)));
    out.body(Body::from_stream(body))
        .unwrap_or_else(|e| (StatusCode::BAD_GATEWAY, e.to_string()).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A stand-in for the API on plain HTTP: answers with the credential and body it got.
    async fn fake_api() -> String {
        let app = axum::Router::new().fallback(|req: Request| async move {
            let auth = req.headers().get("authorization").map(|v| v.to_str().unwrap().to_string());
            let key = req.headers().get("x-api-key").is_some();
            let beta = req.headers().get("anthropic-beta").map(|v| v.to_str().unwrap().to_string());
            let path = req.uri().to_string();
            let body = axum::body::to_bytes(req.into_body(), 1 << 20).await.unwrap();
            axum::Json(serde_json::json!({
                "auth": auth, "x_api_key": key, "beta": beta, "path": path,
                "body": String::from_utf8_lossy(&body),
            }))
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    fn direct(token: &str, base: &str) -> Upstream {
        Upstream::Direct {
            base: base.to_string(),
            token: token.to_string(),
        }
    }

    async fn ask(socket: &Path, request: &str) -> String {
        let mut s = tokio::net::UnixStream::connect(socket).await.unwrap();
        s.write_all(request.as_bytes()).await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }

    #[tokio::test]
    async fn the_real_token_replaces_the_dummy_on_the_way_out() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("api.sock");
        let proxy = Proxy::start(&socket, direct("real-token\n", &fake_api().await)).unwrap();
        let answer = ask(
            &socket,
            "POST /v1/messages?beta=true HTTP/1.1\r\nHost: api.anthropic.com\r\n\
             Authorization: Bearer saarthi-proxied\r\nx-api-key: nope\r\nanthropic-beta: oauth-2025-04-20\r\n\
             Content-Length: 5\r\nConnection: close\r\n\r\nhello",
        )
        .await;
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        // Streamed back, so chunked: the JSON is the one object in the body.
        let json: serde_json::Value =
            serde_json::from_str(&answer[answer.find('{').unwrap()..=answer.rfind('}').unwrap()]).unwrap();
        assert_eq!(json["auth"], "Bearer real-token");
        assert_eq!(json["x_api_key"], false);
        assert_eq!(json["beta"], "oauth-2025-04-20");
        assert_eq!(json["path"], "/v1/messages?beta=true");
        assert_eq!(json["body"], "hello");
        drop(proxy);
        assert!(!socket.exists(), "the socket goes with the proxy");
    }

    #[tokio::test]
    async fn nothing_outside_v1_is_forwarded() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("api.sock");
        let _proxy = Proxy::start(&socket, direct("real-token", &fake_api().await)).unwrap();
        let answer = ask(&socket, "GET /api/oauth/profile HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await;
        assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
        assert!(!answer.contains("real-token"));
    }

    #[test]
    fn a_socket_path_too_long_for_unix_is_refused_up_front() {
        let long = PathBuf::from(format!("/tmp/{}/api.sock", "x".repeat(100)));
        let err = Proxy::start(&long, direct("t", UPSTREAM)).err().unwrap();
        assert!(err.to_string().contains("too long"), "{err}");
    }
}
