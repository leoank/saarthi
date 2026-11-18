//! What a session changed (ADR-0067): yajna-serve's files viewer, its changes view and its
//! read-only git API (yajna's ADR-0038), rooted at a session's directory and served by the
//! node itself as the local service `changes`, on a socket of its own, so the hub relays it
//! like any other (ADR-0052). `/s/<session id>/<rest>` is `/<rest>` of a server over the
//! git worktree the session's directory is in, or the directory itself outside git; the
//! app opens `/s/<id>/files/changes`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use tower::ServiceExt;
use yajna_serve::files::{Root, Watch};

use crate::sessions::Sessions;

/// The service's name, as clients ask for it.
pub const NAME: &str = "changes";

/// Where the node serves it.
pub fn socket(home: &Path) -> PathBuf {
    home.join(".local/state/saarthi/changes.sock")
}

/// Serves the session changes on `socket` for the node's life. Its directory is made for
/// this user only and a stale socket replaced; one another server still answers on is left
/// to it, as an error.
pub async fn serve(socket: PathBuf, sessions: Arc<Sessions>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if tokio::net::UnixStream::connect(&socket).await.is_ok() {
        anyhow::bail!("{}: another server answers there (a second node?)", socket.display());
    }
    let _ = std::fs::remove_file(&socket);
    let listener = tokio::net::UnixListener::bind(&socket).with_context(|| format!("binding {}", socket.display()))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    axum::serve(listener, router(sessions)).await?;
    Ok(())
}

/// The routes: every path is a session's.
pub fn router(sessions: Arc<Sessions>) -> Router {
    Router::new().fallback(move |req: Request| {
        let sessions = sessions.clone();
        async move { dispatch(&sessions, req).await }
    })
}

fn not_found(why: String) -> Response {
    (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": why }))).into_response()
}

async fn dispatch(sessions: &Sessions, mut req: Request<Body>) -> Response {
    let path = req.uri().path().to_string();
    let Some(rest) = path.strip_prefix("/s/") else {
        return not_found("a session's changes are under /s/<session id>/".into());
    };
    let (id, inner) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        // the slash form, which the viewer's relative addresses need
        None => return Redirect::permanent(&format!("{path}/")).into_response(),
    };
    let Some(dir) = sessions.directory(id).await else {
        return not_found(format!("this node knows no session {id}"));
    };
    let top = worktree_top(&dir).await.unwrap_or(dir);
    if !top.is_dir() {
        return not_found(format!("the session's directory {} is gone", top.display()));
    }
    let root = match Root::new(&root_name(&top), top, &[], Watch::Auto) {
        Ok(r) => r,
        Err(e) => return not_found(e),
    };
    let inner = match req.uri().query() {
        Some(q) => format!("{inner}?{q}"),
        None => inner.to_string(),
    };
    match inner.parse() {
        Ok(uri) => *req.uri_mut() = uri,
        Err(_) => return (StatusCode::BAD_REQUEST, "a bad path\n").into_response(),
    }
    let router = yajna_serve::dir_router(vec![root], Arc::new(render), env!("CARGO_PKG_VERSION"));
    match router.oneshot(req).await {
        Ok(r) => r,
        Err(never) => match never {},
    }
}

/// The top of the git worktree `dir` is in, if it is in one.
async fn worktree_top(dir: &Path) -> Option<PathBuf> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .await
        .ok()?;
    out.status.success().then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

/// The root's name in the viewer's paths: the directory's own name, as yajna-serve accepts
/// a root's name.
fn root_name(dir: &Path) -> String {
    let name: String = dir
        .file_name()
        .map(|n| n.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '-' }).collect())
        .unwrap_or_default();
    if name.is_empty() || name.chars().all(|c| c == '.') { "work".into() } else { name }
}

/// Markdown for the viewer's preview, as CommonMark with GitHub's tables, task lists and
/// strikethrough.
fn render(path: &Path) -> Result<yajna_serve::Rendered, String> {
    use pulldown_cmark::{Options, Parser, html};
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = String::new();
    html::push_html(
        &mut out,
        Parser::new_ext(&text, Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH),
    );
    Ok(yajna_serve::Rendered { html: out, id: None, title: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::Outbox;
    use std::time::Duration;

    async fn get(r: &Router, path: &str) -> (StatusCode, String) {
        let res = r.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    #[tokio::test]
    async fn a_session_s_changes_from_its_transcript_s_directory() {
        let dir = tempfile::tempdir().unwrap();
        // a repository with a commit and a change, the session working in a directory of it
        let repo = dir.path().join("my repo");
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["-c", "commit.gpgsign=false", "-c", "user.name=a", "-c", "user.email=a@x"])
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("README.md"), "# one\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        std::fs::write(repo.join("README.md"), "# two\n").unwrap();
        // the node never saw the session: its transcript names the directory
        let claude = dir.path().join(".claude");
        let project = claude.join("projects/-x");
        std::fs::create_dir_all(&project).unwrap();
        let cwd = repo.join("sub");
        std::fs::write(
            project.join("s-1.jsonl"),
            format!("{{\"type\":\"summary\"}}\n{}\n", serde_json::json!({"type": "user", "cwd": cwd})),
        )
        .unwrap();
        let (outbox, _rx) = Outbox::new(10);
        let r = router(Arc::new(Sessions::new(claude, outbox, Duration::from_secs(60), Vec::new())));

        // rooted at the worktree's top, named after it
        let (s, body) = get(&r, "/s/s-1/api/files/roots").await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert!(body.contains("\"my-repo\""), "{body}");
        let (s, body) = get(&r, "/s/s-1/api/git/status?path=my-repo").await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert!(body.contains("README.md"), "{body}");
        let (s, body) = get(&r, "/s/s-1/api/git/diff?path=my-repo/README.md").await;
        assert!(s == StatusCode::OK && body.contains("+# two"), "{body}");
        let (s, body) = get(&r, "/s/s-1/api/files/render?path=my-repo/README.md").await;
        assert!(s == StatusCode::OK && body.contains("<h1>two</h1>"), "{body}");
        let (s, body) = get(&r, "/s/s-1/files/changes").await;
        assert!(s == StatusCode::OK && body.to_lowercase().contains("<!doctype"), "{body}");

        // the bare prefix goes to its slash form; unknown sessions and other paths are refused
        assert_eq!(get(&r, "/s/s-1").await.0, StatusCode::PERMANENT_REDIRECT);
        for missing in ["/s/nope/api/info", "/s/../api/info", "/api/info", "/s/s-1/api/worktrees"] {
            assert_eq!(get(&r, missing).await.0, StatusCode::NOT_FOUND, "{missing}");
        }
    }
}
