//! A thin safe layer over libtailscale's C API (`tailscale.h`), declared by hand.

use std::ffi::{CStr, CString, c_char, c_int};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};

use anyhow::{Result, bail};

#[allow(non_camel_case_types)]
type tailscale = c_int;

unsafe extern "C" {
    fn tailscale_new() -> tailscale;
    fn tailscale_up(sd: tailscale) -> c_int;
    fn tailscale_close(sd: tailscale) -> c_int;
    fn tailscale_set_dir(sd: tailscale, dir: *const c_char) -> c_int;
    fn tailscale_set_hostname(sd: tailscale, hostname: *const c_char) -> c_int;
    fn tailscale_set_authkey(sd: tailscale, authkey: *const c_char) -> c_int;
    fn tailscale_set_logfd(sd: tailscale, fd: c_int) -> c_int;
    fn tailscale_getips(sd: tailscale, buf: *mut c_char, buflen: usize) -> c_int;
    fn tailscale_dial(sd: tailscale, network: *const c_char, addr: *const c_char, out: *mut c_int) -> c_int;
    fn tailscale_listen(sd: tailscale, network: *const c_char, addr: *const c_char, out: *mut c_int) -> c_int;
    fn tailscale_getremoteaddr(l: c_int, conn: c_int, buf: *mut c_char, buflen: usize) -> c_int;
    fn tailscale_accept(l: c_int, out: *mut c_int) -> c_int;
    fn tailscale_status_json(sd: tailscale, out: *mut *mut c_char) -> c_int;
    fn tailscale_errmsg(sd: tailscale, buf: *mut c_char, buflen: usize) -> c_int;
}

/// One tailnet identity: a tsnet server inside this process.
#[derive(Clone, Copy, Debug)]
pub struct Server(tailscale);

fn c(s: &str) -> CString {
    CString::new(s).expect("no NUL in the argument")
}

impl Server {
    /// Configures a server; `authkey` only matters the first time a state dir is used.
    pub fn new(dir: &str, hostname: &str, authkey: Option<&str>, logfd: RawFd) -> Result<Self> {
        let s = Server(unsafe { tailscale_new() });
        s.check(unsafe { tailscale_set_dir(s.0, c(dir).as_ptr()) }, "set_dir")?;
        s.check(unsafe { tailscale_set_hostname(s.0, c(hostname).as_ptr()) }, "set_hostname")?;
        if let Some(k) = authkey {
            s.check(unsafe { tailscale_set_authkey(s.0, c(k).as_ptr()) }, "set_authkey")?;
        }
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
            bail!("getips: {rc}");
        }
        Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
    }

    /// Returns the listener descriptor; accept on it with [`accept`].
    pub fn listen(self, addr: &str) -> Result<RawFd> {
        let mut l = -1;
        self.check(
            unsafe { tailscale_listen(self.0, c("tcp").as_ptr(), c(addr).as_ptr(), &mut l) },
            "listen",
        )?;
        Ok(l)
    }

    /// A TCP connection to `addr` (`host:port`, a MagicDNS name or an IP). Blocks.
    pub fn dial(self, addr: &str) -> Result<OwnedFd> {
        let mut conn = -1;
        self.check(
            unsafe { tailscale_dial(self.0, c("tcp").as_ptr(), c(addr).as_ptr(), &mut conn) },
            "dial",
        )?;
        Ok(unsafe { OwnedFd::from_raw_fd(conn) })
    }

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
        bail!("tailscale_{what}: {msg} (rc {rc})")
    }
}

/// Accepts one connection on a listener: the connection and the dialler's address. Blocks.
pub fn accept(listener: RawFd) -> Result<(OwnedFd, String)> {
    let mut conn = -1;
    let rc = unsafe { tailscale_accept(listener, &mut conn) };
    if rc != 0 {
        bail!("tailscale_accept: rc {rc}");
    }
    let mut buf = [0 as c_char; 128];
    let rc = unsafe { tailscale_getremoteaddr(listener, conn, buf.as_mut_ptr(), buf.len()) };
    let remote = if rc == 0 {
        unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
    } else {
        format!("(getremoteaddr rc {rc})")
    };
    Ok((unsafe { OwnedFd::from_raw_fd(conn) }, remote))
}

/// Whether the descriptor is marked close-on-exec.
pub fn cloexec(fd: RawFd) -> bool {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    flags >= 0 && flags & libc::FD_CLOEXEC != 0
}
