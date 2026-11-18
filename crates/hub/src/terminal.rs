//! A shell on a node, for a client (ADR-0054), relayed as service requests are (ADR-0052):
//! the client opens `Terminal` naming the node and the window size; the hub asks the node
//! over its `Link` with `OpenTerminal`; the node starts the shell and dials `TerminalStream`
//! back, naming it; the hub joins the two. What the client types and its resizes go down
//! the node's stream, the shell's output comes up it. Either side closing ends both: the
//! client's stream ending closes the node's, which ends the shell.

use std::pin::Pin;

use saarthi_proto::control::v1::TerminalUp;
use saarthi_proto::node::v1::{TerminalIn, TerminalOut};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tonic::{Status, Streaming};

use crate::node_service::Hub;

pub type ToClientStream = Pin<Box<dyn Stream<Item = Result<TerminalOut, Status>> + Send>>;
pub type ToNodeStream = Pin<Box<dyn Stream<Item = Result<TerminalIn, Status>> + Send>>;

/// Serves a client's `Terminal`.
pub async fn open(hub: &Hub, client: &str, mut inbound: Streaming<TerminalUp>) -> Result<ToClientStream, Status> {
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| Status::invalid_argument("an empty stream"))?;
    let size = first.r#in.unwrap_or_default();
    if size.cols == 0 || size.rows == 0 {
        return Err(Status::invalid_argument("the first message carries the window size"));
    }
    let (input, input_rx) = mpsc::channel(32);
    let (to_client, from_node) = mpsc::channel(64);
    // What the client sends goes down the node's stream once it is open; until then it
    // waits here. The client going away drops `input`, which ends the node's stream.
    let early = Some(size.clone()).filter(|s| !s.input.is_empty());
    tokio::spawn(async move {
        if let Some(i) = early
            && input.send(Ok(TerminalIn { cols: 0, rows: 0, ..i })).await.is_err()
        {
            return;
        }
        loop {
            match inbound.message().await {
                Ok(Some(m)) => {
                    if let Some(i) = m.r#in
                        && input.send(Ok(i)).await.is_err()
                    {
                        return;
                    }
                }
                Ok(None) | Err(_) => return,
            }
        }
    });
    hub.live
        .open_terminal(&first.node_id, size.cols, size.rows, input_rx, to_client)
        .await?;
    tracing::info!(%client, node_id = %first.node_id, cols = size.cols, rows = size.rows, "a terminal opened");
    Ok(Box::pin(ReceiverStream::new(from_node)))
}

/// Serves a node's `TerminalStream` for the node `node_id`.
pub async fn stream(hub: &Hub, node_id: &str, mut inbound: Streaming<TerminalOut>) -> Result<ToNodeStream, Status> {
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| Status::invalid_argument("an empty stream"))?;
    let Some(pending) = hub.live.take_terminal(&first.id, node_id).await else {
        return Err(Status::not_found("no such terminal is waiting"));
    };
    let to_client = pending.to_client;
    // The node's stream down is the client's input until the shell has ended and its
    // reason has gone to the client; then it closes, so the node lets go of its call.
    let (down, down_rx) = mpsc::channel(32);
    let (stop, mut stopped) = tokio::sync::oneshot::channel::<()>();
    let mut input = pending.input;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                m = input.recv() => match m {
                    Some(m) => if down.send(m).await.is_err() { return },
                    None => return,
                },
                _ = &mut stopped => return,
            }
        }
    });
    tokio::spawn(async move {
        let _stop = stop;
        let mut up = Some(first);
        loop {
            let m = match up.take() {
                Some(m) => m,
                None => match inbound.message().await {
                    Ok(Some(m)) => m,
                    Ok(None) => return,
                    Err(e) => {
                        let _ = to_client.send(Err(e)).await;
                        return;
                    }
                },
            };
            if m.output.is_empty() && m.ended.is_empty() {
                continue;
            }
            let ended = !m.ended.is_empty();
            if to_client.send(Ok(TerminalOut { id: String::new(), ..m })).await.is_err() || ended {
                return;
            }
        }
    });
    Ok(Box::pin(ReceiverStream::new(down_rx)))
}
