//! Embedded tsnet for the hub and the node (ADR-0048): one tailnet identity inside this
//! process, through libtailscale's C API declared by hand (ADR-0015). It serves tonic over
//! connections accepted on the tailnet, dials the tailnet for tonic's client, and names
//! the peer behind an address from the identity's own status ([[EXP-20261112-rq8kgc]]).

mod ffi;

use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::{Context as _, Result, bail};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Uri;
use tonic::transport::server::Connected;

/// What an identity is started with.
#[derive(Clone, Debug)]
pub struct Options {
    /// The tailnet's name in saarthi's own records and logs.
    pub name: String,
    /// The node key and the rest of tsnet's state; created if missing.
    pub dir: PathBuf,
    /// The machine name on the tailnet, and so its MagicDNS name.
    pub hostname: String,
    /// The auth key, if any.
    pub authkey: Option<AuthKey>,
    /// The tailnet removes the machine soon after it goes offline: for tests.
    pub ephemeral: bool,
}

/// Where an identity's auth key comes from.
#[derive(Clone)]
pub enum AuthKey {
    /// A file, read on each start while it exists; it is needed only until the first
    /// join of a hub or a node, and again if the tailnet logs the identity out.
    File(PathBuf),
    /// The key itself, as the phone keeps it (ADR-0016).
    Value(String),
}

impl std::fmt::Debug for AuthKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthKey::File(p) => f.debug_tuple("File").field(p).finish(),
            AuthKey::Value(_) => f.write_str("Value(..)"),
        }
    }
}

/// One tailnet identity. Dropping the last handle leaves the tailnet.
pub struct Tailnet {
    name: String,
    sd: ffi::Server,
}

impl std::fmt::Debug for Tailnet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tailnet").field("name", &self.name).finish()
    }
}

/// Where a connection accepted on a tailnet came from; tonic puts it in each request's
/// extensions.
#[derive(Clone, Debug)]
pub struct Remote {
    tailnet: Arc<Tailnet>,
    /// The caller's tailnet IP.
    pub addr: String,
}

impl Remote {
    pub fn tailnet_name(&self) -> &str {
        &self.tailnet.name
    }

    /// Who the tailnet says the caller is.
    pub async fn peer(&self) -> Option<Peer> {
        self.tailnet.whois(&self.addr).await
    }
}

/// Who holds a tailnet address, as the identity's own status names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub host: String,
    /// The owner's login, or the tags for a tagged machine.
    pub login: String,
}

impl std::fmt::Display for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.host, self.login)
    }
}

/// Whether tsnet has already joined from `dir`: it keeps its node key in this file.
fn joined(dir: &Path) -> bool {
    dir.join("tailscaled.state").exists()
}

impl Tailnet {
    /// Joins the tailnet and waits until it is usable. The first join needs the auth key;
    /// later starts use the state in `dir`.
    pub async fn start(opts: Options) -> Result<Arc<Tailnet>> {
        std::fs::create_dir_all(&opts.dir).with_context(|| format!("creating {}", opts.dir.display()))?;
        restrict(&opts.dir)?;
        // tsnet uses the key only when the identity has to log in: the first join, or
        // after the tailnet logged it out. A file may be gone after the first join.
        let authkey = match &opts.authkey {
            Some(AuthKey::Value(key)) => Some(key.trim().to_string()),
            Some(AuthKey::File(file)) => match std::fs::read_to_string(file) {
                Ok(key) => Some(key.trim().to_string()),
                Err(_) if joined(&opts.dir) => None,
                Err(e) => return Err(e).with_context(|| format!("reading {}", file.display())),
            },
            None if joined(&opts.dir) => None,
            None => bail!(
                "tailnet {}: {} holds no state and no auth key was given",
                opts.name,
                opts.dir.display()
            ),
        };
        // tsnet logs a lot; it goes to a file beside its state, not to ours.
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(opts.dir.join("tsnet.log"))
            .context("opening the tsnet log")?;
        let dir = opts.dir.to_str().context("the tailnet state path is not UTF-8")?;
        // Android gives tsnet no default place for its log state; the identities share
        // the directory above their own.
        #[cfg(target_os = "android")]
        if let Some(parent) = opts.dir.parent().and_then(|p| p.to_str()) {
            ffi::set_logs_dir(parent)?;
        }
        let sd = ffi::Server::new(
            dir,
            &opts.hostname,
            authkey.as_deref(),
            opts.ephemeral,
            std::os::fd::IntoRawFd::into_raw_fd(log),
        )
            .with_context(|| format!("tailnet {}", opts.name))?;
        let tailnet = Arc::new(Tailnet { name: opts.name.clone(), sd });
        let up = tailnet.clone();
        tokio::task::spawn_blocking(move || up.sd.up())
            .await?
            .with_context(|| format!("joining tailnet {}", opts.name))?;
        tracing::info!(tailnet = %opts.name, hostname = %opts.hostname, ips = %tailnet.sd.ips().unwrap_or_default(), "on the tailnet");
        Ok(tailnet)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The identity's tailnet addresses, comma-separated.
    pub fn ips(&self) -> String {
        self.sd.ips().unwrap_or_default()
    }

    /// Connections accepted on `port`, for tonic's `serve_with_incoming`. The stream ends
    /// when accepting fails.
    pub fn incoming(self: &Arc<Self>, port: u16) -> Result<ReceiverStream<io::Result<Conn>>> {
        let listener = self.sd.listen(&format!(":{port}"))?;
        let (tx, rx) = mpsc::channel(16);
        let me = self.clone();
        let rt = tokio::runtime::Handle::current();
        // `tailscale_accept` blocks, so it gets a thread of its own.
        std::thread::Builder::new()
            .name(format!("tailnet-accept-{}", self.name))
            .spawn(move || {
                let _rt = rt.enter();
                loop {
                    let conn = match ffi::accept(listener) {
                        Ok((fd, addr)) => stream(fd).map(|io| Conn {
                            io,
                            remote: Remote { tailnet: me.clone(), addr },
                        }),
                        Err(e) => {
                            tracing::warn!(tailnet = %me.name, error = %format!("{e:#}"), "accepting stopped");
                            return;
                        }
                    };
                    if tx.blocking_send(conn).is_err() {
                        return;
                    }
                }
            })?;
        Ok(ReceiverStream::new(rx))
    }

    /// A TCP connection to `host:port` on the tailnet. Right after a start from saved
    /// state the name may not resolve yet; the caller retries as for any failed connect.
    pub async fn dial(self: &Arc<Self>, addr: &str) -> io::Result<UnixStream> {
        let me = self.clone();
        let addr = addr.to_string();
        let fd = tokio::task::spawn_blocking(move || me.sd.dial(&addr))
            .await
            .map_err(io::Error::other)?
            .map_err(|e| io::Error::other(format!("{e:#}")))?;
        stream(fd)
    }

    /// A connector for tonic's `Endpoint::connect_with_connector`: dials the endpoint's
    /// host and port on this tailnet.
    pub fn connector(
        self: &Arc<Self>,
    ) -> impl tower::Service<
        Uri,
        Response = TokioIo<UnixStream>,
        Error = io::Error,
        Future = impl Future<Output = io::Result<TokioIo<UnixStream>>> + Send,
    > + Clone
    + Send
    + 'static {
        let me = self.clone();
        tower::service_fn(move |uri: Uri| {
            let me = me.clone();
            async move {
                let host = uri.host().ok_or_else(|| io::Error::other("no host in the address"))?;
                let port = uri.port_u16().unwrap_or(80);
                Ok(TokioIo::new(me.dial(&format!("{host}:{port}")).await?))
            }
        })
    }

    /// Who holds `addr` on this tailnet, or None when the status does not name it.
    pub async fn whois(self: &Arc<Self>, addr: &str) -> Option<Peer> {
        let me = self.clone();
        let status = tokio::task::spawn_blocking(move || me.sd.status()).await.ok()?.ok()?;
        peer_in(&status, addr)
    }
}

impl Drop for Tailnet {
    fn drop(&mut self) {
        self.sd.close();
    }
}

/// The state holds the node key: only this user may read it.
fn restrict(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("restricting {}", dir.display()))
}

/// libtailscale's connections are stream sockets ([[EXP-20261112-rq8kgc]]).
fn stream(fd: OwnedFd) -> io::Result<UnixStream> {
    let s = std::os::unix::net::UnixStream::from(fd);
    s.set_nonblocking(true)?;
    UnixStream::from_std(s)
}

/// Finds `addr` among the peers of a `tailscale_status_json` document.
fn peer_in(status: &serde_json::Value, addr: &str) -> Option<Peer> {
    let ip = addr
        .parse::<std::net::SocketAddr>()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| addr.trim_matches(['[', ']']).to_string());
    let peer = status["Peer"].as_object()?.values().find(|p| {
        p["TailscaleIPs"]
            .as_array()
            .is_some_and(|ips| ips.iter().any(|i| i.as_str() == Some(ip.as_str())))
    })?;
    let tags: Vec<&str> = peer["Tags"].as_array().into_iter().flatten().filter_map(|t| t.as_str()).collect();
    let login = if tags.is_empty() {
        status["User"][peer["UserID"].to_string()]["LoginName"].as_str().unwrap_or_default().to_string()
    } else {
        tags.join(",")
    };
    Some(Peer {
        host: peer["HostName"].as_str().unwrap_or_default().to_string(),
        login,
    })
}

/// A connection accepted on a tailnet.
pub struct Conn {
    io: UnixStream,
    remote: Remote,
}

impl Connected for Conn {
    type ConnectInfo = Remote;
    fn connect_info(&self) -> Remote {
        self.remote.clone()
    }
}

impl AsyncRead for Conn {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl AsyncWrite for Conn {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status() -> serde_json::Value {
        json!({
            "Peer": {
                "nodekey:a": {"HostName": "rogue", "TailscaleIPs": ["100.1.2.3", "fd7a::1"], "UserID": 7, "Tags": null},
                "nodekey:b": {"HostName": "ci", "TailscaleIPs": ["100.9.9.9"], "UserID": 8, "Tags": ["tag:ci", "tag:x"]}
            },
            "User": {"7": {"LoginName": "me@github"}, "8": {"LoginName": "tagged-devices"}}
        })
    }

    #[test]
    fn a_peer_is_named_by_its_address_with_or_without_a_port() {
        let want = Some(Peer { host: "rogue".into(), login: "me@github".into() });
        assert_eq!(peer_in(&status(), "100.1.2.3"), want);
        assert_eq!(peer_in(&status(), "100.1.2.3:52011"), want);
        assert_eq!(peer_in(&status(), "[fd7a::1]:7700"), want);
        assert_eq!(peer_in(&status(), "fd7a::1"), want);
    }

    #[test]
    fn a_tagged_peer_is_named_by_its_tags_and_a_stranger_not_at_all() {
        assert_eq!(peer_in(&status(), "100.9.9.9").unwrap().login, "tag:ci,tag:x");
        assert_eq!(peer_in(&status(), "100.4.4.4"), None);
        assert_eq!(peer_in(&json!({}), "100.1.2.3"), None);
    }

    #[tokio::test]
    async fn a_first_start_without_a_key_is_refused_before_touching_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let err = Tailnet::start(Options {
            name: "home".into(),
            dir: dir.path().join("home"),
            hostname: "saarthi-test".into(),
            authkey: None,
            ephemeral: true,
        })
        .await
        .unwrap_err();
        assert!(format!("{err:#}").contains("no auth key"), "{err:#}");
    }
}
