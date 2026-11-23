//! A shell's screen: libghostty-vt through `vt.c`, which keeps what a client that attaches
//! is sent (ADR-0078, EXP-20261123-j9wyqh).

use std::ptr::NonNull;

#[repr(C)]
struct ShTerm {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn sh_new(cols: u16, rows: u16, lines: usize) -> *mut ShTerm;
    fn sh_free(s: *mut ShTerm);
    fn sh_write(s: *mut ShTerm, data: *const u8, len: usize);
    fn sh_resize(s: *mut ShTerm, cols: u16, rows: u16, cell_w: u32, cell_h: u32);
    fn sh_snapshot(s: *mut ShTerm, out: *mut *mut u8, len: *mut usize) -> i32;
    fn sh_title(s: *mut ShTerm, buf: *mut u8, cap: usize) -> usize;
    fn sh_buf_free(p: *mut u8);
}

/// A terminal fed a shell's output. Not thread-safe: the holder keeps it behind a lock.
pub struct Screen(NonNull<ShTerm>);

// The terminal is owned by this value alone and only touched through `&mut self`.
unsafe impl Send for Screen {}

impl Screen {
    /// A screen of this size keeping about `lines` of scrollback; None if libghostty-vt could
    /// not make one.
    pub fn new(cols: u16, rows: u16, lines: usize) -> Option<Self> {
        NonNull::new(unsafe { sh_new(cols, rows, lines) }).map(Self)
    }

    pub fn write(&mut self, data: &[u8]) {
        unsafe { sh_write(self.0.as_ptr(), data.as_ptr(), data.len()) }
    }

    pub fn resize(&mut self, cols: u16, rows: u16, width_px: u32, height_px: u32) {
        // libghostty-vt takes a cell's size; the client gives its window's.
        let (w, h) = if cols > 0 && rows > 0 { (width_px / u32::from(cols), height_px / u32::from(rows)) } else { (0, 0) };
        unsafe { sh_resize(self.0.as_ptr(), cols, rows, w, h) }
    }

    /// What a client attaching now is sent: the screen as it is, with its scrollback, as VT
    /// for a fresh terminal of the same size.
    pub fn snapshot(&mut self) -> Vec<u8> {
        let mut out: *mut u8 = std::ptr::null_mut();
        let mut len = 0usize;
        if unsafe { sh_snapshot(self.0.as_ptr(), &mut out, &mut len) } != 0 || out.is_null() {
            return Vec::new();
        }
        let v = unsafe { std::slice::from_raw_parts(out, len) }.to_vec();
        unsafe { sh_buf_free(out) };
        v
    }

    /// The window title the shell or its program last set.
    pub fn title(&mut self) -> String {
        let mut buf = [0u8; 256];
        let n = unsafe { sh_title(self.0.as_ptr(), buf.as_mut_ptr(), buf.len()) };
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        unsafe { sh_free(self.0.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `bytes` to a fresh screen and returns it.
    fn fed(bytes: &[u8]) -> Screen {
        let mut s = Screen::new(80, 24, 10_000).unwrap();
        s.write(bytes);
        s
    }

    #[test]
    fn a_snapshot_restores_the_screen_in_a_fresh_one() {
        let mut a = fed(b"\x1b[1;32mhello\x1b[0m world\r\nsecond line");
        let mut b = fed(&a.snapshot());
        assert_eq!(a.snapshot(), b.snapshot());
    }

    #[test]
    fn a_full_screen_program_keeps_the_shell_behind_it() {
        let mut a = fed(b"$ ls\r\nfile\r\n$ top");
        // The sequence entering the alternate screen split across two writes.
        a.write(b"\r\n\x1b[?10");
        a.write(b"49h\x1b[2J\x1b[Htop - 12:00");
        let mut b = fed(&a.snapshot());
        a.write(b"\x1b[?1049l");
        b.write(b"\x1b[?1049l");
        assert_eq!(a.snapshot(), b.snapshot(), "the primary screen came back the same");
        assert!(String::from_utf8_lossy(&b.snapshot()).contains("file"));
    }

    #[test]
    fn the_title_is_what_the_shell_set() {
        let mut s = fed(b"\x1b]2;vim notes.md\x07");
        assert_eq!(s.title(), "vim notes.md");
    }
}
