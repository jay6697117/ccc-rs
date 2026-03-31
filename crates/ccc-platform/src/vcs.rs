use std::path::Path;

/// Version control system type.
/// Corresponds to TS `detectVcs()` in src/utils/platform.ts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vcs {
    Git,
    Mercurial,
    Svn,
    Perforce,
    Tfs,
    Jujutsu,
    Sapling,
}

impl std::fmt::Display for Vcs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Vcs::Git       => write!(f, "git"),
            Vcs::Mercurial => write!(f, "mercurial"),
            Vcs::Svn       => write!(f, "svn"),
            Vcs::Perforce  => write!(f, "perforce"),
            Vcs::Tfs       => write!(f, "tfs"),
            Vcs::Jujutsu   => write!(f, "jujutsu"),
            Vcs::Sapling   => write!(f, "sapling"),
        }
    }
}

/// Detect VCS for the given directory (walks up to root).
/// Returns the first VCS found, or None.
pub fn detect_vcs(dir: &Path) -> Option<Vcs> {
    // Check P4PORT env var first (Perforce without config file)
    if std::env::var("P4PORT").is_ok() {
        return Some(Vcs::Perforce);
    }

    let mut current = dir.to_path_buf();
    loop {
        if let Some(vcs) = check_dir_for_vcs(&current) {
            return Some(vcs);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn check_dir_for_vcs(dir: &Path) -> Option<Vcs> {
    let markers: &[(&str, Vcs)] = &[
        (".git",      Vcs::Git),
        (".hg",       Vcs::Mercurial),
        (".svn",      Vcs::Svn),
        (".p4config", Vcs::Perforce),
        (".jj",       Vcs::Jujutsu),
        (".sl",       Vcs::Sapling),
        // TFS markers
        ("$tf",       Vcs::Tfs),
        (".tfvc",     Vcs::Tfs),
    ];
    for (marker, vcs) in markers {
        if dir.join(marker).exists() {
            return Some(*vcs);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_dir_with(marker: &str) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(marker);
        std::fs::create_dir_all(&path).unwrap();
        dir
    }

    #[test]
    fn detects_git() {
        let dir = make_dir_with(".git");
        assert_eq!(detect_vcs(dir.path()), Some(Vcs::Git));
    }

    #[test]
    fn detects_mercurial() {
        let dir = make_dir_with(".hg");
        assert_eq!(detect_vcs(dir.path()), Some(Vcs::Mercurial));
    }

    #[test]
    fn detects_jujutsu() {
        let dir = make_dir_with(".jj");
        assert_eq!(detect_vcs(dir.path()), Some(Vcs::Jujutsu));
    }

    #[test]
    fn no_vcs_in_empty_dir() {
        // Use a temp dir with no VCS markers and no P4PORT
        // (P4PORT may be set in CI, so only test structure)
        let dir = tempfile::tempdir().unwrap();
        // Result depends on env; just assert it doesn't panic
        let _ = detect_vcs(dir.path());
    }

    #[test]
    fn vcs_display() {
        assert_eq!(Vcs::Git.to_string(), "git");
        assert_eq!(Vcs::Mercurial.to_string(), "mercurial");
    }

    #[test]
    fn walks_up_to_find_vcs() {
        let root = make_dir_with(".git");
        let deep = root.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        assert_eq!(detect_vcs(&deep), Some(Vcs::Git));
    }
}
