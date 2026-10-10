//! User configuration: `<home>/config.toml` (`$ANNOTATOR_HOME` or `~/.annotator`).
//!
//! ```toml
//! author = "alice"
//! ```

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    pub author: Option<String>,
}

/// Load `<home>/config.toml`; a missing or invalid file yields the defaults.
pub fn load(home: &Path) -> Config {
    let Ok(text) = std::fs::read_to_string(home.join("config.toml")) else {
        return Config::default();
    };
    toml::from_str(&text).unwrap_or_else(|e| {
        tracing::warn!("ignoring invalid config.toml: {e}");
        Config::default()
    })
}

/// Reviewer name: config `author`, else `$USER`, else `reviewer`.
pub fn author(home: &Path) -> String {
    let user = std::env::var("USER").ok();
    resolve_author(load(home).author, user)
}

fn resolve_author(configured: Option<String>, user: Option<String>) -> String {
    [configured, user]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "reviewer".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_author_and_falls_back() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load(d.path()).author, None);
        std::fs::write(d.path().join("config.toml"), "author = \"alice\"\n").unwrap();
        assert_eq!(load(d.path()).author.as_deref(), Some("alice"));
        std::fs::write(d.path().join("config.toml"), "author = [").unwrap();
        assert_eq!(load(d.path()).author, None);
        assert_eq!(resolve_author(Some("a".into()), Some("u".into())), "a");
        assert_eq!(resolve_author(Some(" ".into()), Some("u".into())), "u");
        assert_eq!(resolve_author(None, None), "reviewer");
    }
}
