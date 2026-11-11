//! Named Claude tokens (ADR-0039): `claude setup-token` tokens the hub reads at start from
//! `--claude-tokens`, a file of `<name> <token>` lines like the peer token files, and hands
//! to a node with a spawn or resume. Clients see names and dates, never values; the record
//! keeps a fingerprint of each so a renewed token under the same name is recognised.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// A year, the life of a `setup-token` token, from when the hub first reads it.
pub const LIFETIME_MS: i64 = 365 * 24 * 3600 * 1000;

/// How long before expiry the reminder goes out.
pub const REMIND_MS: i64 = 30 * 24 * 3600 * 1000;

#[derive(Default)]
pub struct ClaudeTokens(BTreeMap<String, String>);

// Never print a value, even in a debug log.
impl std::fmt::Debug for ClaudeTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.keys()).finish()
    }
}

impl ClaudeTokens {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut map = BTreeMap::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(name), Some(token), None) = (parts.next(), parts.next(), parts.next()) else {
                bail!("line {}: expected `<name> <token>`", n + 1);
            };
            if !valid_name(name) {
                bail!("line {}: a name is letters, digits, - and _ only", n + 1);
            }
            if map.values().any(|t| t == token) {
                bail!("line {}: the same token under two names", n + 1);
            }
            if map.insert(name.to_string(), token.to_string()).is_some() {
                bail!("line {}: {name} named twice", n + 1);
            }
        }
        Ok(Self(map))
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// Every name with its token's fingerprint.
    pub fn fingerprints(&self) -> impl Iterator<Item = (&str, String)> {
        self.0.iter().map(|(n, t)| (n.as_str(), fingerprint(t)))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The first 16 hex digits of the token's SHA-256: enough to tell a renewed token from the
/// old one, nothing to recover it from.
pub fn fingerprint(token: &str) -> String {
    let hash = Sha256::digest(token.as_bytes());
    hash.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// The windows of a `rate_limit_event`'s `rate_limit_info`, as the node forwards it:
/// `unifiedWindows.{five_hour,seven_day}` with `utilization` from 0 to 1 and `resetsAt` in
/// Unix seconds (EXP-20261016-rq1402). None when it carries neither window.
pub fn windows(payload_json: &str, time_ms: i64) -> Option<crate::db::Windows> {
    let v: serde_json::Value = serde_json::from_str(payload_json).ok()?;
    let w = &v["unifiedWindows"];
    let pct = |k: &str| {
        w[k]["utilization"]
            .as_f64()
            .map(|u| (u * 1000.0).round() / 10.0)
    };
    let resets = |k: &str| w[k]["resetsAt"].as_i64();
    let out = crate::db::Windows {
        time_ms,
        five_hour_pct: pct("five_hour"),
        five_hour_resets_s: resets("five_hour"),
        seven_day_pct: pct("seven_day"),
        seven_day_resets_s: resets("seven_day"),
    };
    (out.five_hour_pct.is_some() || out.seven_day_pct.is_some()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_and_refuses_what_would_be_ambiguous() {
        let t = ClaudeTokens::parse("# accounts\nwork  sk-a\n\npersonal sk-b\n").unwrap();
        assert_eq!(t.get("work"), Some("sk-a"));
        assert_eq!(t.names().collect::<Vec<_>>(), ["personal", "work"]);
        assert_eq!(fingerprint("sk-a").len(), 16);
        assert_ne!(fingerprint("sk-a"), fingerprint("sk-b"));
        assert!(!format!("{t:?}").contains("sk-"));
        for bad in ["work", "a b c", "a t\na u", "a t\nb t", "wo/rk t"] {
            assert!(ClaudeTokens::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_the_windows_of_a_rate_limit_event() {
        let info = r#"{"status": "allowed", "resetsAt": 1792255200, "rateLimitType": "five_hour",
            "unifiedWindows": {"five_hour": {"utilization": 0.39, "resetsAt": 1792255200},
                               "seven_day": {"utilization": 0.17, "resetsAt": 1792753200}}}"#;
        let w = windows(info, 5).unwrap();
        assert_eq!(
            (
                w.time_ms,
                w.five_hour_pct,
                w.five_hour_resets_s,
                w.seven_day_pct,
                w.seven_day_resets_s
            ),
            (
                5,
                Some(39.0),
                Some(1792255200),
                Some(17.0),
                Some(1792753200)
            )
        );
        assert!(windows(r#"{"status": "allowed"}"#, 5).is_none());
        assert!(windows("not json", 5).is_none());
    }
}
