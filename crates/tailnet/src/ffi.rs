//! libtailscale's C API (`tailscale.h` at `ac7ed43`), declared by hand, and a thin safe layer.
//! Every call that can block (`up`, `dial`, `accept`) is made off the async runtime.

use std::ffi::{CStr, CString, c_char, c_int};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};

use anyhow::{Result, bail};

unsafe extern "C" {
    fn tailscale_new() -> c_int;
    fn tailscale_up(sd: c_int) -> c_int;
    fn tailscale_close(sd: c_int) -> c_int;
    fn tailscale_set_dir(sd: c_int, dir: *const c_char) -> c_int;
    fn tailscale_set_hostname(sd: c_int, hostname: *const c_char) -> c_int;
    fn tailscale_set_authkey(sd: c_int, authkey: *const c_char) -> c_int;
    fn tailscale_set_logfd(sd: c_int, fd: c_int) -> c_int;
    fn tailscale_set_ephemeral(sd: c_int, ephemeral: c_int) -> c_int;
    fn tailscale_getips(sd: c_int, buf: *mut c_char, buflen: usize) -> c_int;
    fn tailscale_dial(sd: c_int, network: *const c_char, addr: *const c_char, out: *mut c_int) -> c_int;
    fn tailscale_listen(sd: c_int, network: *const c_char, addr: *const c_char, out: *mut c_int) -> c_int;
    fn tailscale_getremoteaddr(l: c_int, conn: c_int, buf: *mut c_char, buflen: usize) -> c_int;
    fn tailscale_accept(l: c_int, out: *mut c_int) -> c_int;
    fn tailscale_status_json(sd: c_int, out: *mut *mut c_char) -> c_int;
    fn tailscale_errmsg(sd: c_int, buf: *mut c_char, buflen: usize) -> c_int;
}

fn c(s: &str) -> Result<CString> {
    Ok(CString::new(s)?)
}

/// A tsnet server handle.
#[derive(Clone, Copy, Debug)]
pub struct Server(c_int);

impl Server {
    /// Configures a server; nothing touches the network until [`Server::up`].
    pub fn new(dir: &str, hostname: &str, authkey: Option<&str>, ephemeral: bool, logfd: RawFd) -> Result<Self> {
        let s = Server(unsafe { tailscale_new() });
        s.check(unsafe { tailscale_set_dir(s.0, c(dir)?.as_ptr()) }, "set_dir")?;
        s.check(unsafe { tailscale_set_hostname(s.0, c(hostname)?.as_ptr()) }, "set_hostname")?;
        if let Some(k) = authkey {
            s.check(unsafe { tailscale_set_authkey(s.0, c(k)?.as_ptr()) }, "set_authkey")?;
        }
        s.check(unsafe { tailscale_set_ephemeral(s.0, ephemeral as c_int) }, "set_ephemeral")?;
        s.check(unsafe { tailscale_set_logfd(s.0, logfd) }, "set_logfd")?;
        Ok(s)
    }

    /// Joins the tailnet and waits until it is usable. Blocks.
    pub fn up(self) -> Result<()> {
        self.check(unsafe { tailscale_up(self.0) }, "up")
    }

    pub fn ips(self) -> Result<String> {
        let mut buf = [0 as c_char; 256];
        let rc = unsafe { tailscale_getips(self.0, buf.as_mut_ptr(), buf.len()) };
        if rc != 0 {
            bail!("tailscale_getips: {rc}");
        }
        Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
    }

    /// A TCP listener on the tailnet; accept on it with [`accept`].
    pub fn listen(self, addr: &str) -> Result<RawFd> {
        let mut l = -1;
        self.check(
            unsafe { tailscale_listen(self.0, c("tcp")?.as_ptr(), c(addr)?.as_ptr(), &mut l) },
            "listen",
        )?;
        Ok(l)
    }

    /// A TCP connection to `host:port` on the tailnet. Blocks.
    pub fn dial(self, addr: &str) -> Result<OwnedFd> {
        let mut conn = -1;
        self.check(
            unsafe { tailscale_dial(self.0, c("tcp")?.as_ptr(), c(addr)?.as_ptr(), &mut conn) },
            "dial",
        )?;
        Ok(unsafe { OwnedFd::from_raw_fd(conn) })
    }

    /// The backend status, the JSON of tailscale's `ipnstate.Status`.
    pub fn status(self) -> Result<serde_json::Value> {
        let mut out: *mut c_char = std::ptr::null_mut();
        self.check(unsafe { tailscale_status_json(self.0, &mut out) }, "status_json")?;
        let json = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
        unsafe { libc::free(out.cast()) };
        Ok(serde_json::from_str(&json)?)
    }

    pub fn close(self) {
        unsafe { tailscale_close(self.0) };
    }

    fn check(self, rc: c_int, what: &str) -> Result<()> {
        if rc == 0 {
            return Ok(());
        }
        let mut buf = [0 as c_char; 1024];
        unsafe { tailscale_errmsg(self.0, buf.as_mut_ptr(), buf.len()) };
        let msg = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned();
        bail!("tailscale_{what}: {msg}")
    }
}

/// Accepts one connection: the connection and the caller's tailnet address. Blocks.
pub fn accept(listener: RawFd) -> Result<(OwnedFd, String)> {
    let mut conn = -1;
    let rc = unsafe { tailscale_accept(listener, &mut conn) };
    if rc != 0 {
        bail!("tailscale_accept: {}", std::io::Error::last_os_error());
    }
    let conn = unsafe { OwnedFd::from_raw_fd(conn) };
    let mut buf = [0 as c_char; 128];
    let rc = unsafe { tailscale_getremoteaddr(listener, std::os::fd::AsRawFd::as_raw_fd(&conn), buf.as_mut_ptr(), buf.len()) };
    let addr = if rc == 0 {
        unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
    } else {
        String::new()
    };
    Ok((conn, addr))
}
