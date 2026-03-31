/// Corresponds to TS `ExternalPermissionMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalPermissionMode {
    AcceptEdits,
    BypassPermissions,
    Default,
    DontAsk,
    Plan,
}

/// Corresponds to TS `InternalPermissionMode` (superset of External).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    AcceptEdits,
    BypassPermissions,
    Default,
    DontAsk,
    Plan,
    Auto,
    Bubble,
}

impl From<ExternalPermissionMode> for PermissionMode {
    fn from(m: ExternalPermissionMode) -> Self {
        match m {
            ExternalPermissionMode::AcceptEdits      => Self::AcceptEdits,
            ExternalPermissionMode::BypassPermissions => Self::BypassPermissions,
            ExternalPermissionMode::Default          => Self::Default,
            ExternalPermissionMode::DontAsk          => Self::DontAsk,
            ExternalPermissionMode::Plan             => Self::Plan,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_mode_serde() {
        let json = serde_json::to_string(&PermissionMode::BypassPermissions).unwrap();
        assert_eq!(json, "\"bypassPermissions\"");
    }

    #[test]
    fn external_converts_to_internal() {
        let m: PermissionMode = ExternalPermissionMode::Plan.into();
        assert_eq!(m, PermissionMode::Plan);
    }
}
