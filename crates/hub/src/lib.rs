//! The saarthi hub: records every node's sessions, events and health, and passes a person's
//! decisions and prompts from a client to the node that holds the session.

pub mod claude_tokens;
pub mod control_service;
pub mod db;
pub mod live;
pub mod node_service;
pub mod notify;
pub mod tokens;

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use saarthi_proto::control::v1::control_service_server::ControlServiceServer;
use saarthi_proto::node::v1::node_service_server::NodeServiceServer;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

use crate::node_service::{Hub, now_ms};

/// Serves both services on a listener until the task is dropped. The listener is the
/// transport boundary: the embedded tsnet listener takes its place in phase 3.
pub async fn serve(listener: TcpListener, hub: Hub) -> Result<()> {
    // Pings from the hub's side too: a node or phone that vanished is found within half a
    // minute instead of when the OS gives up on the connection (phase 1.5 item 15).
    tonic::transport::Server::builder()
        .http2_keepalive_interval(Some(std::time::Duration::from_secs(15)))
        .http2_keepalive_timeout(Some(std::time::Duration::from_secs(10)))
        .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
        .add_service(NodeServiceServer::new(hub.clone()))
        .add_service(ControlServiceServer::new(hub))
        .serve_with_incoming(TcpListenerStream::new(listener))
        .await?;
    Ok(())
}

/// Sends the reminder for every Claude token within a month of expiring that has not had
/// one for its current expiry date (ADR-0039). Run at start and then hourly.
pub async fn remind_expiring(hub: &Hub) -> Result<()> {
    let now = now_ms();
    let names: Vec<&str> = hub.claude_tokens.names().collect();
    for t in hub.db.claude_tokens(&names).await? {
        if hub.db.claim_expiry_reminder(&t.name, now).await? {
            tracing::info!(token = %t.name, expires_ms = t.expires_ms, "Claude token expiry reminder");
            hub.notifier.token_expiry(&t.name, t.expires_ms, now);
        }
    }
    Ok(())
}

/// How long the hub waits for a node to run a usage probe.
const PROBE_TIMEOUT: Duration = Duration::from_secs(120);

/// Usage probes (ADR-0043): keeps every Claude token's account windows fresh by having
/// the node named `node` run one throwaway Haiku turn under each, about every `every`
/// (each wait drawn between 85% and 115% of it), skipping an account a real session has
/// reported on within 85% of `every`. The first probes after a start come within
/// `first_within`, spread out. Runs until the hub stops; a node offline or a probe failing
/// only waits for the next round.
pub async fn run_usage_probes(hub: Hub, node: String, every: Duration, first_within: Duration) {
    let mut next: HashMap<String, i64> = HashMap::new();
    let tick = (every / 8).clamp(Duration::from_millis(100), Duration::from_secs(30));
    let mut ticks = tokio::time::interval(tick);
    loop {
        ticks.tick().await;
        if let Err(e) = probe_due(&hub, &node, every, first_within, &mut next).await {
            tracing::warn!(error = %format!("{e:#}"), "usage probes failed");
        }
    }
}

/// A random time between 85% and 115% of `every`, in ms.
fn spread(every: Duration) -> i64 {
    let base = every.as_millis() as i64;
    let r = (uuid::Uuid::new_v4().as_u128() % 301) as i64; // 0..=300
    base * (850 + r) / 1000
}

async fn probe_due(
    hub: &Hub,
    node: &str,
    every: Duration,
    first_within: Duration,
    next: &mut HashMap<String, i64>,
) -> Result<()> {
    use saarthi_proto::node::v1::{ClaudeToken, Probe, command};
    let now = now_ms();
    let Some(node_id) = hub
        .db
        .nodes()
        .await?
        .into_iter()
        .find(|n| n.1 == node)
        .map(|n| n.0)
    else {
        return Ok(());
    };
    if hub.live.spawning(&node_id).await.is_none() {
        return Ok(());
    }
    let names: Vec<&str> = hub.claude_tokens.names().collect();
    let fresh = every.as_millis() as i64 * 85 / 100;
    for t in hub.db.claude_tokens(&names).await? {
        let first = first_within.as_millis().max(1);
        let at = *next
            .entry(t.name.clone())
            .or_insert_with(|| now + (uuid::Uuid::new_v4().as_u128() % first) as i64);
        if now < at {
            continue;
        }
        if t.usage_ms > 0 && now - t.usage_ms < fresh {
            // A session under the token reported its windows lately.
            next.insert(t.name.clone(), t.usage_ms + spread(every));
            continue;
        }
        next.insert(t.name.clone(), now + spread(every));
        let Some(token) = hub.claude_tokens.get(&t.name) else {
            continue;
        };
        let action = command::Action::Probe(Probe {
            claude_token: Some(ClaudeToken {
                name: t.name.clone(),
                token: token.to_string(),
            }),
        });
        let result = match hub
            .live
            .command_within(&node_id, "", action, PROBE_TIMEOUT)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(token = %t.name, error = %e.message(), "usage probe not run");
                continue;
            }
        };
        if !result.ok {
            tracing::warn!(token = %t.name, error = %result.error, "usage probe failed");
            continue;
        }
        let Some(w) = claude_tokens::windows(&result.probe_json, now_ms()) else {
            tracing::warn!(token = %t.name, "usage probe reported no windows");
            continue;
        };
        tracing::info!(token = %t.name, five_hour = ?w.five_hour_pct, seven_day = ?w.seven_day_pct, "usage probe");
        hub.db.record_claude_token_usage(&t.name, &w).await?;
        hub.notifier.token_usage(&t.name, &node_id, &w).await;
    }
    Ok(())
}
