//! Photos sent with a prompt from the app (ADR-0071). Each is saved where the session can
//! read it, and its path is added to the prompt's text, so Claude reads it with its own
//! tools; that works for every kind of session, a terminal one included, which only takes
//! typed text. A session on the host gets `~/.local/state/saarthi/uploads/<session>/`; one
//! in a sandboxed profile gets `tmp/uploads/<session>/` in the profile's state directory,
//! which its sandbox can read.

use std::path::{Path, PathBuf};

use saarthi_proto::node::v1::Attachment;

/// The most a prompt's photos may weigh together: under gRPC's 4 MB a message, which both
/// the app's call to the hub and the hub's to the node keep. The app shrinks each photo to a
/// few hundred KB.
pub const MAX_BYTES: usize = 3 << 20;

/// Where the photos of sessions on the host go, one directory per session.
pub fn host_root(home: &Path) -> PathBuf {
    home.join(".local/state/saarthi/uploads")
}

/// Where the photos of a sandboxed profile's sessions go, inside what its sandbox reads.
pub fn profile_root(state_dir: &Path) -> PathBuf {
    state_dir.join("tmp/uploads")
}

/// Where session `id`'s photos go: under `profile_state` for a sandboxed session, else
/// under `home`.
pub fn dir(home: &Path, profile_state: Option<&Path>, id: &str) -> PathBuf {
    match profile_state {
        Some(state) => profile_root(state).join(id),
        None => host_root(home).join(id),
    }
}

/// Whether `path` is a photo the node saved for session `id`: a file, not a link, directly
/// in that session's directory under one of `roots`, that directory not a link either. A
/// read of it needs nobody's leave (the hooks allow it), since the node put it there for
/// that session to read.
pub fn saved_for(path: &Path, id: &str, roots: &[PathBuf]) -> bool {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || !path.is_absolute() {
        return false;
    }
    if path.components().any(|c| !matches!(c, std::path::Component::RootDir | std::path::Component::Normal(_))) {
        return false;
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    let plain = |p: &Path, dir: bool| std::fs::symlink_metadata(p).is_ok_and(|m| !m.file_type().is_symlink() && if dir { m.is_dir() } else { m.is_file() });
    roots.iter().any(|root| {
        let session = root.join(id);
        let same = |a: &Path, b: &Path| matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y);
        plain(&session, true) && plain(path, false) && same(parent, &session) && same(path, &session.join(name))
    })
}

/// Saves `photos` in `dir` and returns `text` with a line naming each saved path. Names are
/// reduced to a safe file name and prefixed with the time, so two photos never collide.
pub fn save(dir: &Path, text: &str, photos: &[Attachment], now_ms: u64) -> Result<String, String> {
    if photos.is_empty() {
        return Ok(text.to_string());
    }
    if photos.iter().map(|p| p.data.len()).sum::<usize>() > MAX_BYTES {
        return Err(format!("the photos are larger than {} MB together", MAX_BYTES >> 20));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
    let mut paths = Vec::new();
    for (i, p) in photos.iter().enumerate() {
        let path = dir.join(format!("{now_ms}-{i}-{}", safe_name(&p.name)));
        std::fs::write(&path, &p.data).map_err(|e| format!("saving {}: {e}", path.display()))?;
        paths.push(path);
    }
    let mut out = text.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    for p in &paths {
        out.push_str(&format!("[Image: {}]\n", p.display()));
    }
    Ok(out.trim_end().to_string())
}

/// A file name of letters, digits, `.`, `-` and `_` only, never empty or hidden.
fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || ".-_".contains(c) { c } else { '_' })
        .take(80)
        .collect();
    let clean = clean.trim_start_matches('.');
    if clean.is_empty() { "photo.jpg".into() } else { clean.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(name: &str, data: &[u8]) -> Attachment {
        Attachment { name: name.into(), data: data.to_vec() }
    }

    #[test]
    fn photos_are_saved_and_named_in_the_text() {
        let tmp = tempfile::tempdir().unwrap();
        let d = dir(tmp.path(), None, "s1");
        let out = save(&d, "what is this?", &[photo("IMG 1.jpg", b"one"), photo("../../etc/x.png", b"two")], 7).unwrap();
        let a = d.join("7-0-IMG_1.jpg");
        let b = d.join("7-1-x.png");
        assert_eq!(out, format!("what is this?\n\n[Image: {}]\n[Image: {}]", a.display(), b.display()));
        assert_eq!(std::fs::read(a).unwrap(), b"one");
        assert_eq!(std::fs::read(b).unwrap(), b"two");
    }

    #[test]
    fn a_photo_alone_is_the_whole_prompt_and_no_photo_leaves_the_text() {
        let tmp = tempfile::tempdir().unwrap();
        let out = save(tmp.path(), "  ", &[photo(".hidden", b"x")], 1).unwrap();
        assert_eq!(out, format!("[Image: {}]", tmp.path().join("1-0-hidden").display()));
        assert_eq!(save(tmp.path(), "just text", &[], 1).unwrap(), "just text");
    }

    #[test]
    fn a_sandboxed_session_gets_its_profiles_tmp() {
        let d = dir(Path::new("/home/u"), Some(Path::new("/srv/box")), "s1");
        assert_eq!(d, Path::new("/srv/box/tmp/uploads/s1"));
        assert_eq!(dir(Path::new("/home/u"), None, "s1"), Path::new("/home/u/.local/state/saarthi/uploads/s1"));
    }

    #[test]
    fn only_a_file_saved_for_the_session_counts_as_its_photo() {
        let tmp = tempfile::tempdir().unwrap();
        let root = host_root(tmp.path());
        let d = dir(tmp.path(), None, "s1");
        save(&d, "x", &[photo("a.jpg", b"1")], 3).unwrap();
        let roots = [root.clone()];
        let saved = d.join("3-0-a.jpg");
        assert!(saved_for(&saved, "s1", &roots));
        // Another session's, a path climbing out, a file elsewhere, a link: not.
        assert!(!saved_for(&saved, "s2", &roots));
        assert!(!saved_for(&d.join("../s1/3-0-a.jpg"), "s1", &roots));
        std::fs::write(tmp.path().join("secret"), b"k").unwrap();
        assert!(!saved_for(&tmp.path().join("secret"), "s1", &roots));
        std::os::unix::fs::symlink(tmp.path().join("secret"), d.join("link.jpg")).unwrap();
        assert!(!saved_for(&d.join("link.jpg"), "s1", &roots));
        std::fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(tmp.path(), root.join("s3")).unwrap();
        assert!(!saved_for(&root.join("s3/secret"), "s3", &roots));
    }

    #[test]
    fn too_much_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let big = vec![0u8; MAX_BYTES + 1];
        assert!(save(tmp.path(), "x", &[photo("a.jpg", &big)], 1).is_err());
    }
}
