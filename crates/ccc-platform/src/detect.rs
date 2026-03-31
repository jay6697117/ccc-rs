/// Corresponds to TS `getPlatform()` in src/utils/platform.ts.
/// Returns a stable identifier for the current OS/environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Linux,
    Wsl,
    Windows,
    Unknown,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::MacOs   => write!(f, "mac"),
            Platform::Linux   => write!(f, "linux"),
            Platform::Wsl     => write!(f, "wsl"),
            Platform::Windows => write!(f, "windows"),
            Platform::Unknown => write!(f, "unknown"),
        }
    }
}

/// Detect the current platform. Checks WSL before returning Linux.
pub fn get_platform() -> Platform {
    #[cfg(target_os = "macos")]
    return Platform::MacOs;

    #[cfg(target_os = "windows")]
    return Platform::Windows;

    #[cfg(target_os = "linux")]
    {
        if is_wsl() {
            return Platform::Wsl;
        }
        return Platform::Linux;
    }

    #[allow(unreachable_code)]
    Platform::Unknown
}

/// Detect WSL by reading /proc/version.
/// Returns true on both WSL1 and WSL2.
pub fn is_wsl() -> bool {
    wsl_version().is_some()
}

/// Return WSL version string ("1" or "2") if running under WSL, else None.
pub fn wsl_version() -> Option<String> {
    #[cfg(not(target_os = "linux"))]
    return None;

    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/version").ok()?;
        let lower = content.to_lowercase();
        // WSL2 / WSL3 etc. — explicit version marker
        if let Some(pos) = lower.find("wsl") {
            let after = &lower[pos + 3..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return Some(digits);
            }
        }
        // WSL1: contains "microsoft" but no explicit WSL version
        if lower.contains("microsoft") {
            return Some("1".to_owned());
        }
        None
    }
}

/// Linux distro info from /etc/os-release.
#[derive(Debug, Clone, PartialEq)]
pub struct LinuxDistroInfo {
    pub id: String,
    pub version_id: Option<String>,
}

/// Parse /etc/os-release. Returns None on non-Linux or read failure.
pub fn linux_distro_info() -> Option<LinuxDistroInfo> {
    #[cfg(not(target_os = "linux"))]
    return None;

    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/etc/os-release").ok()?;
        let mut id = None;
        let mut version_id = None;
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("ID=") {
                id = Some(val.trim_matches('"').to_owned());
            } else if let Some(val) = line.strip_prefix("VERSION_ID=") {
                version_id = Some(val.trim_matches('"').to_owned());
            }
        }
        Some(LinuxDistroInfo {
            id: id.unwrap_or_default(),
            version_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_platform_returns_known_value() {
        let p = get_platform();
        // must be one of the known variants — not Unknown on CI
        assert!(
            p == Platform::MacOs
                || p == Platform::Linux
                || p == Platform::Wsl
                || p == Platform::Windows,
            "unexpected platform: {p}"
        );
    }

    #[test]
    fn platform_display() {
        assert_eq!(Platform::MacOs.to_string(), "mac");
        assert_eq!(Platform::Linux.to_string(), "linux");
        assert_eq!(Platform::Wsl.to_string(), "wsl");
        assert_eq!(Platform::Windows.to_string(), "windows");
    }
}
