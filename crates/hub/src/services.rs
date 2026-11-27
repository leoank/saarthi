//! The service relay (ADR-0052, ADR-0057): a client's HTTP request to one of a node's local
//! services. The client sends the head and the body over `ServiceRequest`; the hub asks the
//! node over its `Link` with `OpenService`, carrying the head; the node starts on the
//! request and dials `ServiceStream` back, naming it; the hub joins the two streams. The
//! request's body goes down the node's stream, the answer comes up it and out to the client.

use std::pin::Pin;

use saarthi_proto::control::v1::{ServiceRequestPart, ServiceResponsePart};
use saarthi_proto::node::v1::{ServiceDown, ServiceUp};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tonic::{Status, Streaming};

use crate::node_service::Hub;

/// Headers that belong to one hop, or that the node sets itself.
pub fn passes(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "connection"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

pub type ToClientStream = Pin<Box<dyn Stream<Item = Result<ServiceResponsePart, Status>> + Send>>;
pub type ToNodeStream = Pin<Box<dyn Stream<Item = Result<ServiceDown, Status>> + Send>>;

/// Serves a client's `ServiceRequest`.
pub async fn request(
    hub: &Hub,
    client: &str,
    mut inbound: Streaming<ServiceRequestPart>,
) -> Result<ToClientStream, Status> {
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| Status::invalid_argument("an empty request"))?;
    let mut head = first
        .head
        .ok_or_else(|| Status::invalid_argument("the first message carries the request head"))?;
    hub.authz.check(client, "ServiceRequest", crate::authz::Res::Node(&first.node_id), serde_json::json!({}))?;
    if !head.path.starts_with('/') {
        return Err(Status::invalid_argument("the path must start with /"));
    }
    head.headers.retain(|h| passes(&h.name));
    let (body_tx, body_rx) = mpsc::channel(8);
    let (to_client, from_node) = mpsc::channel(16);
    tracing::debug!(%client, node_id = %first.node_id, service = %head.service,
        method = %head.method, path = %head.path, "relaying a service request");
    if head.has_body {
        // The body goes down the node's stream once it is open; until then it waits here.
        let mut rest = Some(first.body).filter(|b| !b.is_empty());
        tokio::spawn(async move {
            if let Some(b) = rest.take()
                && body_tx.send(Ok(ServiceDown { body: b })).await.is_err()
            {
                return;
            }
            loop {
                match inbound.message().await {
                    Ok(Some(m)) => {
                        if body_tx.send(Ok(ServiceDown { body: m.body })).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => return,
                    Err(e) => {
                        let _ = body_tx.send(Err(e)).await;
                        return;
                    }
                }
            }
        });
    } else {
        drop(body_tx);
    }
    hub.live
        .open_service(&first.node_id, head, body_rx, to_client)
        .await?;
    Ok(Box::pin(ReceiverStream::new(from_node)))
}

/// Serves a node's `ServiceStream` for the node `node_id`.
pub async fn stream(
    hub: &Hub,
    node_id: &str,
    mut inbound: Streaming<ServiceUp>,
) -> Result<ToNodeStream, Status> {
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| Status::invalid_argument("an empty stream"))?;
    let Some(pending) = hub.live.take_service(&first.id, node_id).await else {
        return Err(Status::not_found("no such service request is waiting"));
    };
    let to_client = pending.to_client;
    tokio::spawn(async move {
        let mut up = Some(Ok(first));
        loop {
            let m = match up.take() {
                Some(m) => m,
                None => match inbound.message().await {
                    Ok(Some(m)) => Ok(m),
                    Ok(None) => return,
                    Err(e) => Err(e),
                },
            };
            let part = match m {
                Ok(m) if !m.error.is_empty() => Err(Status::unavailable(m.error)),
                Ok(m) if m.head.is_none() && m.body.is_empty() => continue,
                Ok(m) => Ok(ServiceResponsePart {
                    head: m.head,
                    body: m.body,
                }),
                Err(e) => Err(e),
            };
            let end = part.is_err();
            if to_client.send(part).await.is_err() || end {
                return;
            }
        }
    });
    Ok(Box::pin(ReceiverStream::new(pending.body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_headers_and_host_never_pass() {
        for h in ["Host", "Connection", "Transfer-Encoding", "upgrade"] {
            assert!(!passes(h), "{h}");
        }
        assert!(passes("range"));
        assert!(passes("accept"));
    }
}
