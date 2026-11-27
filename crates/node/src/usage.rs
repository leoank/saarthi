//! The account's rate-limit utilisation from Anthropic's usage endpoint, with the OAuth
//! access token read from Claude Code's credential store, read only (ADR-0030,
//! EXP-20261016-rq1402, EXP-20261016-rq1408). On macOS the store is the keychain item
//! `Claude Code-credentials` (suffixed for a non-default config dir), read through the
//! `security` tool; on Linux it is `.credentials.json` in the config dir. The token is
//! never logged, written or sent anywhere but the endpoint. Off unless `--claude-usage`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use saarthi_proto::event::v1::{HealthSample, Metric};
use serde_json::Value;

use crate::health::usage_windows;
use crate::outbox::now_ms;

pub const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";

/// The longest the poller waits after being refused for calling too often, unless the
/// endpoint asks for longer.
pub const MAX_WAIT: Duration = Duration::from_secs(30 * 60);

/// The endpoint refused for calling too often (429), with the wait it asked for. It did so
/// for half an hour after about 30 calls in 30 minutes (EXP-20261025-25f6vm).
#[derive(Debug)]
pub struct TooMany(pub Option<Duration>);

impl std::fmt::Display for TooMany {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the usage endpoint answered 429, too many requests")
    }
}

impl std::error::Error for TooMany {}

/// The wait before the next poll after a 429: what the endpoint asked for, else twice the
/// last wait up to `MAX_WAIT`; never less than the interval.
pub fn backoff(every: Duration, last: Duration, retry_after: Option<Duration>) -> Duration {
    match retry_after {
        Some(after) => after.max(every),
        None => (last * 2).min(MAX_WAIT).max(every),
    }
}

/// Where the access token is read from.
#[derive(Debug, Clone)]
pub enum Store {
    /// A macOS keychain generic password by service name.
    Keychain { service: String },
    /// Claude Code's `.credentials.json`.
    File { path: PathBuf },
}

impl Store {
    /// The store Claude Code uses for a config dir: the keychain on macOS, the file elsewhere.
    pub fn for_dir(claude_dir: &Path, default_dir: bool) -> Self {
        if cfg!(target_os = "macos") {
            let service = if default_dir {
                "Claude Code-credentials".to_string()
            } else {
                use sha2::Digest;
                let hash = sha2::Sha256::digest(claude_dir.to_string_lossy().as_bytes());
                format!("Claude Code-credentials-{}", &hex::encode(hash)[..8])
            };
            Store::Keychain { service }
        } else {
            Store::File {
                path: claude_dir.join(".credentials.json"),
            }
        }
    }

    /// The credential JSON as Claude Code stores it. A keychain read that prompts would
    /// block, so it is given ten seconds.
    pub async fn read(&self) -> Result<String> {
        match self {
            Store::Keychain { service } => {
                let out = tokio::time::timeout(
                    Duration::from_secs(10),
                    tokio::process::Command::new("/usr/bin/security")
                        .args(["find-generic-password", "-s", service, "-w"])
                        .output(),
                )
                .await
                .context("the keychain read timed out, a prompt may be waiting")?
                .context("running security")?;
                if !out.status.success() {
                    return Err(anyhow!("no keychain item {service} readable by this process"));
                }
                Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
            }
            Store::File { path } => tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("reading {}", path.display())),
        }
    }
}

/// The access token inside the credential JSON, or why there is none.
pub fn access_token(credential_json: &str) -> Result<String> {
    let v: Value = serde_json::from_str(credential_json).context("the credential is not JSON")?;
    v["claudeAiOauth"]["accessToken"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("the credential has no access token (logged out?)"))
}

/// One poll: read the token, call the endpoint, return a health sample with the windows
/// and `usage.age_s` 0 (the value is live, unlike the cached one the sampler reports).
pub async fn poll(client: &reqwest::Client, store: &Store, url: &str) -> Result<HealthSample> {
    let token = access_token(&store.read().await?)?;
    let response = client
        .get(url)
        .bearer_auth(&token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .context("calling the usage endpoint")?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()?.trim().parse().ok())
            .map(Duration::from_secs);
        return Err(TooMany(after).into());
    }
    let response: Value = response
        .error_for_status()
        .context("the usage endpoint refused")?
        .json()
        .await
        .context("the usage endpoint's body is not JSON")?;
    let mut metrics: Vec<Metric> = usage_windows(&response)
        .into_iter()
        .map(|(name, value)| Metric {
            name: name.into(),
            value,
        })
        .collect();
    if metrics.is_empty() {
        return Err(anyhow!("the usage endpoint's response has no utilisation"));
    }
    metrics.push(Metric {
        name: "usage.age_s".into(),
        value: 0.0,
    });
    Ok(HealthSample {
        time_ms: now_ms(),
        metrics,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_comes_from_the_oauth_block_and_never_from_elsewhere() {
        let cred = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-abc","refreshToken":"r","expiresAt":1}}"#;
        assert_eq!(access_token(cred).unwrap(), "sk-ant-oat01-abc");
        assert!(access_token(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_err());
        assert!(access_token("not json").is_err());
    }

    #[test]
    fn a_refusal_backs_off_to_what_was_asked_or_doubles_up_to_half_an_hour() {
        let (every, min) = (Duration::from_secs(300), |m: u64| Duration::from_secs(m * 60));
        assert_eq!(backoff(every, every, None), min(10));
        assert_eq!(backoff(every, min(20), None), min(30));
        assert_eq!(backoff(every, min(30), None), min(30));
        assert_eq!(backoff(every, every, Some(min(45))), min(45));
        assert_eq!(backoff(every, every, Some(Duration::from_secs(10))), every);
    }

    #[tokio::test]
    async fn a_429_is_told_apart_with_its_retry_after() {
        let app = axum::Router::new().route(
            "/usage",
            axum::routing::get(|| async { (axum::http::StatusCode::TOO_MANY_REQUESTS, [("retry-after", "120")], "") }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/usage", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".credentials.json");
        std::fs::write(&path, r#"{"claudeAiOauth":{"accessToken":"t"}}"#).unwrap();
        let err = poll(&reqwest::Client::new(), &Store::File { path }, &url).await.unwrap_err();
        let too_many = err.downcast_ref::<TooMany>().expect("a 429");
        assert_eq!(too_many.0, Some(Duration::from_secs(120)));
    }

    #[test]
    fn the_store_follows_the_platform_and_the_config_dir() {
        let default = Store::for_dir(Path::new("/Users/x/.claude"), true);
        let other = Store::for_dir(Path::new("/tmp/other"), false);
        if cfg!(target_os = "macos") {
            assert!(matches!(&default, Store::Keychain { service } if service == "Claude Code-credentials"));
            assert!(matches!(&other, Store::Keychain { service } if service.starts_with("Claude Code-credentials-") && service.len() == 32));
        } else {
            assert!(matches!(&default, Store::File { path } if path.ends_with(".credentials.json")));
        }
    }
}
