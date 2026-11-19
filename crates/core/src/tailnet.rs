//! The app's own identities on hubs' tailnets (ADR-0016): one per hub, each with a state
//! directory of its own, started on the first connect through it and kept for the life of
//! the process, so a reconnect does not join again.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use saarthi_tailnet::{AuthKey, Options, Tailnet};
use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint};

use crate::CoreError;

/// What an identity on a hub's tailnet is started with.
#[derive(Clone, uniffi::Record)]
pub struct TailnetLogin {
    /// The identity's state, in the app's private storage; one directory per hub.
    pub dir: String,
    /// The device's name on the tailnet.
    pub hostname: String,
    /// The auth key, used by tsnet only when the identity has to log in.
    pub authkey: Option<String>,
}

impl std::fmt::Debug for TailnetLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TailnetLogin")
            .field("dir", &self.dir)
            .field("hostname", &self.hostname)
            .field("authkey", &self.authkey.as_ref().map(|_| ".."))
            .finish()
    }
}

/// How long the app waits for a join before saying why it failed.
const JOIN_LIMIT: Duration = Duration::from_secs(45);

/// The identities, by state directory: each started once and kept. Each has its own lock,
/// so two joins of one identity never overlap and a slow join holds up no other hub's.
type Slot = Arc<Mutex<Option<Arc<Tailnet>>>>;
static RUNNING: LazyLock<std::sync::Mutex<HashMap<String, Slot>>> = LazyLock::new(Default::default);

/// Starts the identity for `login` if it is not running yet, and gives its tailnet
/// addresses: the app shows the join as a step of its start.
#[uniffi::export(async_runtime = "tokio")]
pub async fn join_tailnet(login: TailnetLogin) -> Result<String, CoreError> {
    Ok(identity(login).await?.ips())
}

/// The identity for `login`, started if it is not running yet.
pub(crate) async fn identity(login: TailnetLogin) -> Result<Arc<Tailnet>, CoreError> {
    let slot = RUNNING.lock().expect("the identities' lock").entry(login.dir.clone()).or_default().clone();
    let mut slot = slot.lock().await;
    if let Some(t) = slot.as_ref() {
        return Ok(t.clone());
    }
    let t = Tailnet::start(Options {
        name: login.hostname.clone(),
        dir: login.dir.clone().into(),
        hostname: login.hostname,
        authkey: login.authkey.filter(|k| !k.trim().is_empty()).map(AuthKey::Value),
        ephemeral: false,
        // The app's start shows the join as a step: it must end, with a reason. A retry
        // after a given-up join needs a new identity (`TailnetLogin::dir`).
        give_up_after: Some(JOIN_LIMIT),
    })
    .await
    .map_err(|e| CoreError::Connection {
        reason: format!("tailnet: {e:#}"),
    })?;
    *slot = Some(t.clone());
    Ok(t)
}

/// A channel to the endpoint through `tailnet`. Right after a start from saved state the
/// hub's name may not resolve until the netmap arrives ([[EXP-20261112-rq8kgc]]), so a
/// failed dial is tried again a few times before giving up.
pub(crate) async fn connect(endpoint: &Endpoint, tailnet: &Arc<Tailnet>) -> Result<Channel, tonic::transport::Error> {
    let mut tries = 0;
    loop {
        match endpoint.connect_with_connector(tailnet.connector()).await {
            Ok(c) => return Ok(c),
            Err(_) if tries < 4 => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => return Err(e),
        }
    }
}
