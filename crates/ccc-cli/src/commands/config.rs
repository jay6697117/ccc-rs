use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use ccc_core::{claude_config_dir, normalize_project_key, GlobalConfig, ProjectConfig};

use crate::cli::{ConfigArgs, ConfigCommand};
use crate::error::CliError;

#[derive(Debug, Clone)]
pub struct ConfigPaths {
    pub cwd: PathBuf,
    pub global_candidates: Vec<PathBuf>,
    pub project_settings_path: PathBuf,
    pub project_local_settings_path: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct ConfigSnapshot {
    pub global: GlobalConfig,
    pub project: ProjectConfig,
    pub project_settings: Option<Value>,
    pub project_local_settings: Option<Value>,
    pub global_path: PathBuf,
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
    }
}

pub fn load_config_snapshot(_paths: &ConfigPaths) -> Result<ConfigSnapshot, CliError> {
    let global_path = _paths
        .global_candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .or_else(|| _paths.global_candidates.first().cloned())
        .ok_or_else(|| CliError::new("no global config candidate paths configured", 1))?;

    let global = if global_path.exists() {
        let text = fs::read_to_string(&global_path)?;
        serde_json::from_str(&text)?
    } else {
        GlobalConfig::default()
    };

    let project_key = normalize_project_key(&_paths.cwd);
    let project = global
        .projects
        .get(&project_key)
        .cloned()
        .unwrap_or_default();

    let project_settings_path = resolve_path(&_paths.cwd, &_paths.project_settings_path);
    let project_local_settings_path =
        resolve_path(&_paths.cwd, &_paths.project_local_settings_path);

    Ok(ConfigSnapshot {
        global,
        project,
        project_settings: read_json_file(&project_settings_path)?,
        project_local_settings: read_json_file(&project_local_settings_path)?,
        global_path,
    })
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
        };

        let snapshot = load_config_snapshot(&paths).unwrap();

        assert_eq!(snapshot.global.theme, Theme::Dark);
        assert!(snapshot.project.allowed_tools.is_empty());
        assert_eq!(snapshot.global_path, temp.path().join("settings.json"));
        assert!(snapshot.project_settings.is_none());
        assert!(snapshot.project_local_settings.is_none());
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
        };

        let snapshot = load_config_snapshot(&paths).unwrap();

        assert_eq!(snapshot.project.allowed_tools, vec!["bash".to_string()]);
    }
}
