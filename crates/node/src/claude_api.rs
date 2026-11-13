//! The Claude API proxy of a sandboxed session (ADR-0050, [[EXP-20261110-g5mh89]]): the
//! sandbox holds only a dummy token. Claude Code inside sends its API requests as plain HTTP
//! over a unix socket (`ANTHROPIC_UNIX_SOCKET`, with `ANTHROPIC_BASE_URL` set to an
//! `http://` URL); this proxy, in the node, puts the real token in their `Authorization`
//! header and sends them on over TLS, streaming the answer back. The sandbox can spend the
//! token through the socket but never read it.
//!
//! Only `/v1/` is forwarded: the inference API Claude Code reaches through its base URL.
//! Its other calls (settings, policy limits, the account) go straight to Anthropic with the
//! dummy and are refused there; a session runs without them.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Where the requests go.
pub const UPSTREAM: &str = "https://api.anthropic.com";

/// What `CLAUDE_CODE_OAUTH_TOKEN` holds inside: never a credential.
pub const DUMMY_TOKEN: &str = "saarthi-proxied";

/// A request body larger than this is refused rather than held in memory.
const MAX_BODY: usize = 64 << 20;

/// One proxy, serving until dropped; the socket file goes with it.
pub struct Proxy {
    path: PathBuf,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
struct Upstream {
    base: String,
    bearer: String,
    client: reqwest::Client,
}

impl Proxy {
    /// Serves on a new socket at `path` (any stale file there is replaced), adding `token`
    /// to every request and sending it to `upstream`. Needs a tokio runtime.
    pub fn start(path: &Path, token: &str, upstream: &str) -> std::io::Result<Proxy> {
        // The usual limit for a unix socket path is 104 bytes, its terminator included.
        if path.as_os_str().len() > 103 {
            return Err(std::io::Error::other(format!(
                "the socket path {} is too long for a unix socket; use a shorter state_dir",
                path.display()
            )));
        }
        let _ = std::fs::remove_file(path);
        let listener = tokio::net::UnixListener::bind(path)?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(std::io::Error::other)?;
        let state = Upstream {
            base: upstream.trim_end_matches('/').to_string(),
            bearer: format!("Bearer {}", token.trim()),
            client,
        };
        let app = axum::Router::new().fallback(forward).with_state(state);
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

async fn forward(State(up): State<Upstream>, req: Request) -> Response {
    let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/").to_string();
    if !path.starts_with("/v1/") {
        tracing::warn!(path = %path, "refused a sandbox's API request outside /v1/");
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
    headers.insert(header::AUTHORIZATION, up.bearer.parse().expect("checked when the token was read"));
    let sent = up
        .client
        .request(method.clone(), format!("{}{path}", up.base))
        .headers(headers)
        .body(body)
        .send()
        .await;
    let resp = match sent {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(path = %path, error = %e, "the Claude API could not be reached");
            return (StatusCode::BAD_GATEWAY, format!("saarthi could not reach the Claude API: {e}")).into_response();
        }
    };
    tracing::debug!(%method, path = %path, status = %resp.status(), "proxied a Claude API request");
    let mut out = Response::builder().status(resp.status());
    copy(resp.headers(), out.headers_mut().expect("a fresh builder"));
    // Streamed as it arrives: a turn's answer is server-sent events.
    out.body(Body::from_stream(resp.bytes_stream()))
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
        let proxy = Proxy::start(&socket, "real-token\n", &fake_api().await).unwrap();
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
        let _proxy = Proxy::start(&socket, "real-token", &fake_api().await).unwrap();
        let answer = ask(&socket, "GET /api/oauth/profile HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").await;
        assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
        assert!(!answer.contains("real-token"));
    }

    #[test]
    fn a_socket_path_too_long_for_unix_is_refused_up_front() {
        let long = PathBuf::from(format!("/tmp/{}/api.sock", "x".repeat(100)));
        let err = Proxy::start(&long, "t", UPSTREAM).err().unwrap();
        assert!(err.to_string().contains("too long"), "{err}");
    }
}
