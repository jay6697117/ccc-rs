use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use ccc_core::{
    ManagedSettingsSnapshot, claude_config_dir, normalize_project_key, GlobalConfig, ProjectConfig,
};

use crate::cli::{ConfigArgs, ConfigCommand};
use crate::error::CliError;
use crate::managed::{EnterpriseMcpSnapshot, load_enterprise_mcp_snapshot, load_managed_settings, managed_root};

#[derive(Debug, Clone)]
pub struct ConfigPaths {
    pub cwd: PathBuf,
    pub global_candidates: Vec<PathBuf>,
    pub project_settings_path: PathBuf,
    pub project_local_settings_path: PathBuf,
    pub managed_root: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSnapshot {
    pub global: GlobalConfig,
    pub global_settings: Option<Value>,
    pub project: ProjectConfig,
    pub project_settings: Option<Value>,
    pub project_local_settings: Option<Value>,
    pub managed_settings: ManagedSettingsSnapshot,
    pub enterprise_mcp: EnterpriseMcpSnapshot,
    pub global_path: PathBuf,
    pub project_settings_path: PathBuf,
    pub project_local_settings_path: PathBuf,
}

pub async fn run(args: ConfigArgs) -> Result<(), CliError> {
    match args.command {
        ConfigCommand::Show => {
            let snapshot = load_config_snapshot(&default_paths(std::env::current_dir()?))?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            Ok(())
        }
    }
}

pub fn default_paths(cwd: PathBuf) -> ConfigPaths {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let config_dir = claude_config_dir();

    ConfigPaths {
        cwd,
        global_candidates: vec![
            config_dir.join("settings.json"),
            PathBuf::from(home.clone()).join(".claude.json"),
            PathBuf::from(home).join(".config.json"),
        ],
        project_settings_path: PathBuf::from(".claude/settings.json"),
        project_local_settings_path: PathBuf::from(".claude/settings.local.json"),
        managed_root: managed_root(&config_dir),
    }
}

pub fn load_config_snapshot(paths: &ConfigPaths) -> Result<ConfigSnapshot, CliError> {
    let global_path = paths
        .global_candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .or_else(|| paths.global_candidates.first().cloned())
        .ok_or_else(|| CliError::new("no global config candidate paths configured", 1))?;

    let global_settings = if global_path.exists() {
        let text = fs::read_to_string(&global_path)?;
        Some(serde_json::from_str::<Value>(&text)?)
    } else {
        None
    };
    let global = match &global_settings {
        Some(raw) => serde_json::from_value(raw.clone())?,
        None => GlobalConfig::default(),
    };

    let project_key = normalize_project_key(&paths.cwd);
    let project = global
        .projects
        .get(&project_key)
        .cloned()
        .unwrap_or_default();

    let project_settings_path = resolve_path(&paths.cwd, &paths.project_settings_path);
    let project_local_settings_path =
        resolve_path(&paths.cwd, &paths.project_local_settings_path);
    let managed_settings = load_managed_settings(&paths.managed_root)?;
    let enterprise_mcp = load_enterprise_mcp_snapshot(&paths.managed_root)?;

    Ok(ConfigSnapshot {
        global,
        global_settings,
        project,
        project_settings: read_json_file(&project_settings_path)?,
        project_local_settings: read_json_file(&project_local_settings_path)?,
        managed_settings,
        enterprise_mcp,
        global_path,
        project_settings_path,
        project_local_settings_path,
    })
}

pub fn write_last_session_id(
    paths: &ConfigPaths,
    project_key: &str,
    session_id: &ccc_core::SessionId,
) -> Result<(), CliError> {
    let mut snapshot = load_config_snapshot(paths)?;
    snapshot
        .global
        .projects
        .entry(project_key.to_string())
        .or_default()
        .last_session_id = Some(session_id.as_str().to_string());

    if let Some(parent) = snapshot.global_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(
        &snapshot.global_path,
        serde_json::to_string_pretty(&snapshot.global)?,
    )?;

    Ok(())
}
fn read_json_file(path: &Path) -> Result<Option<Value>, CliError> {
    if !path.exists() {
        return Ok(None);
    }

    let text = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

fn resolve_path(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccc_core::Theme;

    #[test]
    fn loads_default_snapshot_when_files_are_missing() {
        let temp = tempfile::tempdir().unwrap();
        let paths = ConfigPaths {
            cwd: temp.path().to_path_buf(),
            global_candidates: vec![temp.path().join("settings.json")],
            project_settings_path: temp.path().join(".claude/settings.json"),
            project_local_settings_path: temp.path().join(".claude/settings.local.json"),
            managed_root: temp.path().join("managed"),
        };

        let snapshot = load_config_snapshot(&paths).unwrap();

        assert_eq!(snapshot.global.theme, Theme::Dark);
        assert!(snapshot.project.allowed_tools.is_empty());
        assert_eq!(snapshot.global_path, temp.path().join("settings.json"));
        assert!(snapshot.global_settings.is_none());
        assert!(snapshot.project_settings.is_none());
        assert!(snapshot.project_local_settings.is_none());
        assert_eq!(snapshot.managed_settings, ManagedSettingsSnapshot::missing());
        assert!(snapshot.enterprise_mcp.servers.is_empty());
    }

    #[test]
    fn uses_project_view_from_global_projects_map() {
        let temp = tempfile::tempdir().unwrap();
        let global_path = temp.path().join("settings.json");
        let project_key = normalize_project_key(temp.path());

        fs::write(
            &global_path,
            serde_json::json!({
                "projects": {
                    project_key: {
                        "allowedTools": ["bash"]
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let paths = ConfigPaths {
            cwd: temp.path().to_path_buf(),
            global_candidates: vec![global_path],
            project_settings_path: temp.path().join(".claude/settings.json"),
            project_local_settings_path: temp.path().join(".claude/settings.local.json"),
            managed_root: temp.path().join("managed"),
        };

        let snapshot = load_config_snapshot(&paths).unwrap();

        assert_eq!(snapshot.project.allowed_tools, vec!["bash".to_string()]);
        assert!(snapshot.global_settings.is_some());
    }

    #[test]
    fn writes_last_session_id_into_project_view() {
        let temp = tempfile::tempdir().unwrap();
        let global_path = temp.path().join("settings.json");
        let project_key = normalize_project_key(temp.path());
        let paths = ConfigPaths {
            cwd: temp.path().to_path_buf(),
            global_candidates: vec![global_path.clone()],
            project_settings_path: temp.path().join(".claude/settings.json"),
            project_local_settings_path: temp.path().join(".claude/settings.local.json"),
            managed_root: temp.path().join("managed"),
        };

        fs::write(&global_path, "{}").unwrap();

        write_last_session_id(&paths, &project_key, &ccc_core::SessionId::new("sess-1")).unwrap();

        let written: GlobalConfig =
            serde_json::from_str(&fs::read_to_string(global_path).unwrap()).unwrap();
        assert_eq!(
            written
                .projects
                .get(&project_key)
                .and_then(|project| project.last_session_id.as_deref()),
            Some("sess-1")
        );
    }
}
