//! A shell for a client (ADR-0054, ADR-0078): on `OpenTerminal` the node dials
//! `TerminalStream` back to the hub, naming the terminal, and attaches it to a shell the
//! holder on this machine keeps, a new one or the one the client names. The holder sends the
//! shell's screen as it is, then its output; what the client types goes in, a resize resizes
//! it. The client hanging up detaches: the shell goes on, for the client to attach to again.
//! The shell ending says why. Only with `--terminal`: a shell is the owner's whole account on
//! this machine.

use std::path::PathBuf;
use std::sync::Arc;

use saarthi_proto::node::v1::{OpenTerminal, TerminalIn, TerminalOut};
use saarthi_proto::shells::v1::{AttachIn, Open, Resize, attach_in, attach_out};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::claude_api::HubApi;

/// Serves one `OpenTerminal`. `shell` is `--terminal`: None for no terminal, which the
/// stream back says; empty for the owner's login shell; else that program (tests use one).
/// `holder` is the shell holder's socket.
pub async fn open(api: Arc<HubApi>, open: OpenTerminal, shell: Option<String>, holder: PathBuf) {
    let (up, up_rx) = mpsc::channel::<TerminalOut>(64);
    let _ = up.send(TerminalOut { id: open.id.clone(), ..Default::default() }).await;
    let mut down = match api.terminal_stream(ReceiverStream::new(up_rx)).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(id = %open.id, error = %e, "the hub did not take a terminal stream");
            return;
        }
    };
    let Some(program) = shell else {
        return end(up, down, "this node offers no terminal: it runs without --terminal".into()).await;
    };
    let mut client = match crate::shells::reach(&holder).await {
        Ok(c) => c,
        Err(e) => return end(up, down, format!("the shell holder is not running: {e}")).await,
    };
    let (to_holder, holder_rx) = mpsc::channel::<AttachIn>(64);
    let size = Resize { cols: open.cols, rows: open.rows, ..Default::default() };
    let first = Open { id: open.shell_id.clone(), program, size: Some(size) };
    let _ = to_holder.send(AttachIn { body: Some(attach_in::Body::Open(first)) }).await;
    let mut from_holder = match client.attach(ReceiverStream::new(holder_rx)).await {
        Ok(r) => r.into_inner(),
        Err(e) if e.code() == tonic::Code::NotFound => return end(up, down, e.message().to_string()).await,
        Err(e) => return end(up, down, format!("the shell holder refused: {}", e.message())).await,
    };

    loop {
        tokio::select! {
            m = down.message() => match m {
                Ok(Some(TerminalIn { input, cols, rows, width_px, height_px })) => {
                    if cols > 0 && rows > 0 {
                        let resize = Resize { cols, rows, width_px, height_px };
                        let _ = to_holder.send(AttachIn { body: Some(attach_in::Body::Resize(resize)) }).await;
                    }
                    if !input.is_empty() {
                        let _ = to_holder.send(AttachIn { body: Some(attach_in::Body::Input(input)) }).await;
                    }
                }
                // The client went away: detached, the shell goes on.
                Ok(None) | Err(_) => {
                    tracing::info!(id = %open.id, "a terminal detached by its client");
                    return;
                }
            },
            m = from_holder.message() => match m {
                Ok(Some(m)) => match m.body {
                    Some(attach_out::Body::Attached(a)) => {
                        tracing::info!(id = %open.id, shell = %a.id, started = a.started, "a terminal attached");
                        let _ = up.send(TerminalOut { shell_id: a.id, ..Default::default() }).await;
                    }
                    Some(attach_out::Body::Output(bytes)) => {
                        if up.send(TerminalOut { output: bytes, ..Default::default() }).await.is_err() {
                            return;
                        }
                    }
                    Some(attach_out::Body::Ended(why)) => {
                        tracing::info!(id = %open.id, %why, "a terminal's shell ended");
                        return end(up, down, why).await;
                    }
                    None => {}
                },
                // The holder let go of this client (it fell behind, or the holder went):
                // the stream closes without a reason, and the client attaches again.
                Ok(None) | Err(_) => {
                    tracing::info!(id = %open.id, "the shell holder let a terminal go");
                    return;
                }
            },
        }
    }
}

/// Says why the terminal ended. Dropping the call at once would cancel it before the reason
/// went out: it is held until the hub, having passed the reason on, closes it.
async fn end(up: mpsc::Sender<TerminalOut>, mut down: tonic::Streaming<TerminalIn>, why: String) {
    let _ = up.send(TerminalOut { ended: why, ..Default::default() }).await;
    drop(up);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async { while let Ok(Some(_)) = down.message().await {} }).await;
}
