use std::path::PathBuf;

use serde::Deserialize;

use crate::git::SortMode;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub env_patterns: Vec<String>,
    pub auto_copy_env: bool,
    pub default_base: String,
    /// Root-relative paths (files or directories) to copy between worktrees
    /// alongside env files, e.g. `.claude`, `.cursor`, `.vscode/settings.json`.
    pub copy_paths: Vec<String>,
    /// One of `date-desc`, `date-asc`, `name-asc`, `name-desc`. Kept as a
    /// string so an unrecognized value falls back instead of failing the whole
    /// file's deserialization and discarding every other setting.
    pub default_sort: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            env_patterns: vec![
                ".env".to_owned(),
                ".env.local".to_owned(),
                ".env.*".to_owned(),
            ],
            auto_copy_env: true,
            default_base: "main".to_owned(),
            copy_paths: vec![".claude".to_owned()],
            default_sort: "date-desc".to_owned(),
        }
    }
}

impl Config {
    pub fn sort_mode(&self) -> SortMode {
        SortMode::parse(&self.default_sort).unwrap_or_default()
    }

    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("wt")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_env_patterns() {
        let cfg = Config::default();
        assert_eq!(cfg.env_patterns.len(), 3);
        assert!(cfg.auto_copy_env);
        assert_eq!(cfg.default_base, "main");
        assert_eq!(cfg.copy_paths, vec![".claude"]);
    }

    #[test]
    fn unknown_default_sort_falls_back_without_losing_other_settings() {
        let toml_str = r#"
            default_base = "develop"
            default_sort = "date_desc"
        "#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.sort_mode(), SortMode::DateDesc);
        assert_eq!(cfg.default_base, "develop");
    }

    #[test]
    fn deserialize_partial_config() {
        let toml_str = r"
            auto_copy_env = false
        ";
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert!(!cfg.auto_copy_env);
        // defaults for missing fields
        assert_eq!(cfg.default_base, "main");
    }
}
