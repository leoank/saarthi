//! Pre-shared tokens for nodes and clients, until the credential store holds them: a file of
//! `<name> <token>` lines, blank lines and `#` comments ignored. The name a token maps to is
//! the node's or client's identity at the hub.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};

#[derive(Debug, Default)]
pub struct Tokens(HashMap<String, String>);

impl Tokens {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut map = HashMap::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(name), Some(token), None) = (parts.next(), parts.next(), parts.next()) else {
                bail!("line {}: expected `<name> <token>`", n + 1);
            };
            if map.insert(token.to_string(), name.to_string()).is_some() {
                bail!("line {}: token used twice", n + 1);
            }
        }
        Ok(Self(map))
    }

    /// Every name in the file.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.values().map(String::as_str)
    }

    /// The name for an `authorization` header value of `Bearer <token>`.
    pub fn name_for(&self, authorization: &str) -> Option<&str> {
        let token = authorization.strip_prefix("Bearer ")?;
        self.0.get(token).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_and_tokens() {
        let t = Tokens::parse("# nodes\nmac  t-mac\n\nbox t-box\n").unwrap();
        assert_eq!(t.name_for("Bearer t-mac"), Some("mac"));
        assert_eq!(t.name_for("Bearer nope"), None);
        assert_eq!(t.name_for("t-mac"), None);
        assert!(Tokens::parse("mac").is_err());
        assert!(Tokens::parse("a t\nb t").is_err());
    }
}
