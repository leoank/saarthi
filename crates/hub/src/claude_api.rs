//! The Claude API relay (ADR-0051): a node sends a session's API request under one of the
//! hub's token names over `ClaudeApi`; the hub adds the token, sends it to Anthropic and
//! streams the answer back, so no token's value ever leaves the hub. Each answer's
//! rate-limit headers become the token's usage windows (ADR-0042).

use std::collections::HashMap;
use std::pin::Pin;

use saarthi_proto::node::v1::{ClaudeApiRequest, ClaudeApiResponse, ClaudeApiStatus, HttpHeader};
use tokio_stream::{Stream, StreamExt};
use tonic::{Status, Streaming};

use crate::db::Windows;
use crate::node_service::{Hub, now_ms};

/// Where the requests go.
pub const UPSTREAM: &str = "https://api.anthropic.com";

/// A request body larger than this is refused rather than held in memory.
const MAX_BODY: usize = 64 << 20;

/// Unchanged windows are recorded again only after this long.
const USAGE_EVERY_MS: i64 = 60_000;

pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<ClaudeApiResponse, Status>> + Send>>;

pub struct ClaudeApi {
    upstream: String,
    client: reqwest::Client,
    /// The windows last recorded per token, and when.
    recorded: std::sync::Mutex<HashMap<String, Windows>>,
}

impl ClaudeApi {
    /// Relays to `upstream`, Anthropic's API but in tests.
    pub fn new(upstream: &str) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("a TLS client");
        Self {
            upstream: upstream.trim_end_matches('/').to_string(),
            client,
            recorded: Default::default(),
        }
    }

    /// Whether `w` for `name` should be recorded now: changed, or not recorded lately.
    fn due(&self, name: &str, w: &Windows) -> bool {
        let mut recorded = self.recorded.lock().expect("not poisoned");
        let fresh = recorded.get(name).is_some_and(|last| {
            w.time_ms - last.time_ms < USAGE_EVERY_MS
                && (last.five_hour_pct, last.five_hour_resets_s, last.seven_day_pct, last.seven_day_resets_s)
                    == (w.five_hour_pct, w.five_hour_resets_s, w.seven_day_pct, w.seven_day_resets_s)
        });
        if !fresh {
            recorded.insert(name.to_string(), w.clone());
        }
        !fresh
    }
}

/// Headers that belong to one hop, or that the hub sets itself.
fn passes(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
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

/// The token's windows from an answer's unified rate-limit headers: utilization from 0 to
/// 1 and resets in Unix seconds, as [`crate::claude_tokens::windows`] reads them from a
/// session's stream. None when the answer carries neither window.
pub fn windows(headers: &reqwest::header::HeaderMap, time_ms: i64) -> Option<Windows> {
    let get = |k: &str| headers.get(format!("anthropic-ratelimit-unified-{k}"))?.to_str().ok().map(str::to_string);
    let pct = |k: &str| get(k)?.parse::<f64>().ok().map(|u| (u * 1000.0).round() / 10.0);
    let resets = |k: &str| get(k)?.parse::<i64>().ok();
    let w = Windows {
        time_ms,
        five_hour_pct: pct("5h-utilization"),
        five_hour_resets_s: resets("5h-reset"),
        seven_day_pct: pct("7d-utilization"),
        seven_day_resets_s: resets("7d-reset"),
    };
    (w.five_hour_pct.is_some() || w.seven_day_pct.is_some()).then_some(w)
}

/// Serves one `ClaudeApi` stream for the node `node`.
pub async fn relay(
    hub: &Hub,
    node: &str,
    mut inbound: Streaming<ClaudeApiRequest>,
) -> Result<ResponseStream, Status> {
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| Status::invalid_argument("an empty request"))?;
    let head = first
        .head
        .ok_or_else(|| Status::invalid_argument("the first message carries the request head"))?;
    let Some(token) = hub.claude_tokens.get(&head.claude_token) else {
        return Err(Status::not_found(format!(
            "the hub holds no Claude token named {}",
            head.claude_token
        )));
    };
    if !head.path.starts_with("/v1/") {
        tracing::warn!(%node, path = %head.path, "refused a Claude API request outside /v1/");
        return Err(Status::permission_denied("the hub relays only the Claude API's /v1/"));
    }
    let method = reqwest::Method::from_bytes(head.method.as_bytes())
        .map_err(|_| Status::invalid_argument("not an HTTP method"))?;
    let mut body = first.body;
    while let Some(m) = inbound.message().await? {
        body.extend_from_slice(&m.body);
        if body.len() > MAX_BODY {
            return Err(Status::resource_exhausted("the request body is too large"));
        }
    }
    let mut headers = reqwest::header::HeaderMap::new();
    for h in &head.headers {
        if !passes(&h.name) {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            reqwest::header::HeaderName::from_bytes(h.name.as_bytes()),
            reqwest::header::HeaderValue::from_bytes(&h.value),
        ) {
            headers.append(n, v);
        }
    }
    let bearer = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| Status::internal("the Claude token is not valid in a header"))?;
    headers.insert(reqwest::header::AUTHORIZATION, bearer);
    let resp = hub
        .claude_api
        .client
        .request(method, format!("{}{}", hub.claude_api.upstream, head.path))
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(%node, error = %e, "the Claude API could not be reached");
            Status::unavailable(format!("the hub could not reach the Claude API: {e}"))
        })?;
    let status = resp.status();
    tracing::debug!(%node, claude_token = %head.claude_token, path = %head.path, %status, "relayed a Claude API request");
    if let Some(w) = windows(resp.headers(), now_ms())
        && hub.claude_api.due(&head.claude_token, &w)
    {
        record_usage(hub, node, &head.claude_token, w);
    }
    let out = ClaudeApiStatus {
        status: status.as_u16() as u32,
        headers: resp
            .headers()
            .iter()
            .filter(|(n, _)| passes(n.as_str()))
            .map(|(n, v)| HttpHeader {
                name: n.as_str().to_string(),
                value: v.as_bytes().to_vec(),
            })
            .collect(),
    };
    let head = tokio_stream::once(Ok(ClaudeApiResponse {
        head: Some(out),
        body: Vec::new(),
    }));
    let node = node.to_string();
    // A piece as hyper reads it, well under the node's message limit.
    let body = resp.bytes_stream().map(move |chunk| match chunk {
        Ok(bytes) => Ok(ClaudeApiResponse {
            head: None,
            body: bytes.to_vec(),
        }),
        Err(e) => {
            tracing::warn!(node = %node, error = %e, "the Claude API's answer broke off");
            Err(Status::unavailable(format!("the Claude API's answer broke off: {e}")))
        }
    });
    Ok(Box::pin(head.chain(body)))
}

/// Records the windows on the token and alerts on them, off the request's path.
fn record_usage(hub: &Hub, node: &str, name: &str, w: Windows) {
    let (hub, node, name) = (hub.clone(), node.to_string(), name.to_string());
    tokio::spawn(async move {
        if let Err(e) = hub.db.record_claude_token_usage(&name, &w).await {
            tracing::warn!(error = %format!("{e:#}"), "recording a token's usage failed");
            return;
        }
        match hub.db.node_id(&node).await {
            Ok(Some(id)) => hub.notifier.token_usage(&name, &id, &w).await,
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "finding the node failed"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_windows_from_the_unified_headers() {
        let mut h = reqwest::header::HeaderMap::new();
        for (k, v) in [
            ("anthropic-ratelimit-unified-5h-utilization", "0.57"),
            ("anthropic-ratelimit-unified-5h-reset", "1794670200"),
            ("anthropic-ratelimit-unified-7d-utilization", "0.13"),
            ("anthropic-ratelimit-unified-7d-reset", "1795172400"),
            ("anthropic-ratelimit-unified-status", "allowed"),
        ] {
            h.insert(k, v.parse().unwrap());
        }
        let w = windows(&h, 9).unwrap();
        assert_eq!(
            (w.time_ms, w.five_hour_pct, w.five_hour_resets_s, w.seven_day_pct, w.seven_day_resets_s),
            (9, Some(57.0), Some(1794670200), Some(13.0), Some(1795172400))
        );
        assert!(windows(&reqwest::header::HeaderMap::new(), 9).is_none());
    }

    #[test]
    fn unchanged_windows_are_recorded_once_a_minute() {
        let api = ClaudeApi::new(UPSTREAM);
        let w = |t: i64, pct: f64| Windows {
            time_ms: t,
            five_hour_pct: Some(pct),
            five_hour_resets_s: None,
            seven_day_pct: None,
            seven_day_resets_s: None,
        };
        assert!(api.due("work", &w(0, 1.0)));
        assert!(!api.due("work", &w(1_000, 1.0)));
        assert!(api.due("work", &w(2_000, 2.0)));
        assert!(api.due("personal", &w(2_000, 2.0)));
        assert!(!api.due("work", &w(30_000, 2.0)));
        assert!(api.due("work", &w(62_000, 2.0)));
    }

    #[test]
    fn the_token_and_hop_headers_never_pass() {
        for h in ["Authorization", "x-api-key", "Host", "Content-Length", "Connection"] {
            assert!(!passes(h), "{h}");
        }
        assert!(passes("anthropic-beta"));
    }
}
