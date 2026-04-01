use std::fs;
use std::path::{Path, PathBuf};

pub fn claude_config_dir() -> PathBuf {
    if let Ok(v) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(v);
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".claude")
}

pub fn normalize_project_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{claude_config_dir, normalize_project_key};

    #[test]
    fn claude_config_dir_prefers_env_var() {
        std::env::set_var("CLAUDE_CONFIG_DIR", "/tmp/claude-config");
        assert_eq!(claude_config_dir(), PathBuf::from("/tmp/claude-config"));
        std::env::remove_var("CLAUDE_CONFIG_DIR");
    }

    #[test]
    fn normalize_project_key_canonicalizes_and_normalizes_separators() {
        let temp = tempfile::tempdir().unwrap();
        let key = normalize_project_key(temp.path());

        assert!(key.contains('/'));
        assert!(!key.contains('\\'));
    }
}
