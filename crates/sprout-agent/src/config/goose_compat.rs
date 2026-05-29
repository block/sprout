//! Goose config compatibility layer.
//!
//! Reads `~/.config/goose/config.yaml` to extract Databricks credentials
//! as a fallback when env vars aren't set. Bridge code that shrinks as
//! Sprout's spawn-time env injection improves.

use std::{collections::HashMap, path::PathBuf};

#[derive(Default)]
pub(super) struct GooseDatabricksConfig {
    pub(super) host: Option<String>,
    pub(super) model: Option<String>,
}

impl GooseDatabricksConfig {
    pub(super) fn load_default() -> Self {
        goose_config_path()
            .and_then(|p| Self::load_from_path(&p))
            .unwrap_or_default()
    }

    pub(super) fn load_from_path(path: &std::path::Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        let map: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(&raw).ok()?;
        Some(Self::from_map(&map))
    }

    pub(super) fn from_map(map: &HashMap<String, serde_yaml::Value>) -> Self {
        let host = yaml_string(map, "DATABRICKS_HOST");
        let explicit_model = yaml_string(map, "DATABRICKS_MODEL");
        let goose_provider = yaml_string(map, "GOOSE_PROVIDER");
        let goose_model = yaml_string(map, "GOOSE_MODEL");
        let goose_mode = yaml_string(map, "GOOSE_MODE");

        // Flat-key model resolution (existing)
        let flat_model = explicit_model.or_else(|| {
            if goose_provider
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case("databricks"))
            {
                goose_model.or(goose_mode)
            } else {
                None
            }
        });

        // Nested provider format fallback (active_provider + providers block)
        let active_provider = yaml_string(map, "active_provider");
        let (nested_host, nested_model) = active_provider
            .as_deref()
            .filter(|ap| ap.to_ascii_lowercase().starts_with("databricks"))
            .and_then(|ap| nested_provider_config(map, ap))
            .unwrap_or((None, None));

        Self {
            host: host.or(nested_host),
            model: flat_model.or(nested_model),
        }
    }
}

fn nested_provider_config(
    map: &HashMap<String, serde_yaml::Value>,
    active_provider: &str,
) -> Option<(Option<String>, Option<String>)> {
    let providers = map.get("providers").and_then(|v| v.as_mapping())?;
    let provider_config = providers
        .get(serde_yaml::Value::String(active_provider.to_owned()))?
        .as_mapping()?;

    let model = provider_config
        .get(serde_yaml::Value::String("model".to_owned()))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let host = provider_config
        .get(serde_yaml::Value::String("host".to_owned()))
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Some((host, model))
}

fn yaml_string(map: &HashMap<String, serde_yaml::Value>, key: &str) -> Option<String> {
    map.get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn goose_config_path() -> Option<PathBuf> {
    if let Ok(root) = std::env::var("GOOSE_PATH_ROOT") {
        return Some(PathBuf::from(root).join("config").join("config.yaml"));
    }
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("goose")
            .join("config.yaml"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Existing flat-key tests ──────────────────────────────────────────────

    #[test]
    fn goose_databricks_config_reads_host_and_model() {
        let map = HashMap::from([
            (
                "DATABRICKS_HOST".to_string(),
                serde_yaml::Value::String("https://dbc.example".into()),
            ),
            (
                "GOOSE_PROVIDER".to_string(),
                serde_yaml::Value::String("databricks".into()),
            ),
            (
                "GOOSE_MODEL".to_string(),
                serde_yaml::Value::String("goose-claude-4-6-sonnet".into()),
            ),
        ]);
        let cfg = GooseDatabricksConfig::from_map(&map);
        assert_eq!(cfg.host.as_deref(), Some("https://dbc.example"));
        assert_eq!(cfg.model.as_deref(), Some("goose-claude-4-6-sonnet"));
    }

    #[test]
    fn goose_databricks_config_prefers_explicit_databricks_model() {
        let map = HashMap::from([
            (
                "DATABRICKS_HOST".to_string(),
                serde_yaml::Value::String("https://dbc.example".into()),
            ),
            (
                "DATABRICKS_MODEL".to_string(),
                serde_yaml::Value::String("explicit-db-model".into()),
            ),
            (
                "GOOSE_PROVIDER".to_string(),
                serde_yaml::Value::String("databricks".into()),
            ),
            (
                "GOOSE_MODEL".to_string(),
                serde_yaml::Value::String("goose-model".into()),
            ),
        ]);
        let cfg = GooseDatabricksConfig::from_map(&map);
        assert_eq!(cfg.model.as_deref(), Some("explicit-db-model"));
    }

    #[test]
    fn goose_databricks_config_ignores_goose_model_for_other_provider() {
        let map = HashMap::from([
            (
                "DATABRICKS_HOST".to_string(),
                serde_yaml::Value::String("https://dbc.example".into()),
            ),
            (
                "GOOSE_PROVIDER".to_string(),
                serde_yaml::Value::String("anthropic".into()),
            ),
            (
                "GOOSE_MODEL".to_string(),
                serde_yaml::Value::String("claude".into()),
            ),
        ]);
        let cfg = GooseDatabricksConfig::from_map(&map);
        assert_eq!(cfg.host.as_deref(), Some("https://dbc.example"));
        assert!(cfg.model.is_none());
    }

    // ── Nested active_provider + providers block (newer goose format) ────────

    #[test]
    fn from_map_reads_nested_active_provider_databricks_v2() {
        // Simulates:
        //   active_provider: databricks_v2
        //   providers:
        //     databricks_v2:
        //       model: goose-claude-4-6-opus
        //       host: https://dbc.example
        let providers_map = {
            let mut inner = serde_yaml::Mapping::new();
            let mut provider_entry = serde_yaml::Mapping::new();
            provider_entry.insert(
                serde_yaml::Value::String("model".into()),
                serde_yaml::Value::String("goose-claude-4-6-opus".into()),
            );
            provider_entry.insert(
                serde_yaml::Value::String("host".into()),
                serde_yaml::Value::String("https://dbc.example".into()),
            );
            inner.insert(
                serde_yaml::Value::String("databricks_v2".into()),
                serde_yaml::Value::Mapping(provider_entry),
            );
            serde_yaml::Value::Mapping(inner)
        };

        let map = HashMap::from([
            (
                "active_provider".to_string(),
                serde_yaml::Value::String("databricks_v2".into()),
            ),
            ("providers".to_string(), providers_map),
        ]);

        let cfg = GooseDatabricksConfig::from_map(&map);
        assert_eq!(cfg.host.as_deref(), Some("https://dbc.example"));
        assert_eq!(cfg.model.as_deref(), Some("goose-claude-4-6-opus"));
    }

    #[test]
    fn from_map_flat_keys_win_over_nested() {
        // Flat DATABRICKS_MODEL takes precedence over the nested providers block.
        let providers_map = {
            let mut inner = serde_yaml::Mapping::new();
            let mut provider_entry = serde_yaml::Mapping::new();
            provider_entry.insert(
                serde_yaml::Value::String("model".into()),
                serde_yaml::Value::String("nested-model".into()),
            );
            provider_entry.insert(
                serde_yaml::Value::String("host".into()),
                serde_yaml::Value::String("https://nested-host.example".into()),
            );
            inner.insert(
                serde_yaml::Value::String("databricks_v2".into()),
                serde_yaml::Value::Mapping(provider_entry),
            );
            serde_yaml::Value::Mapping(inner)
        };

        let map = HashMap::from([
            (
                "active_provider".to_string(),
                serde_yaml::Value::String("databricks_v2".into()),
            ),
            ("providers".to_string(), providers_map),
            (
                "DATABRICKS_HOST".to_string(),
                serde_yaml::Value::String("https://flat-host.example".into()),
            ),
            (
                "DATABRICKS_MODEL".to_string(),
                serde_yaml::Value::String("flat-model".into()),
            ),
        ]);

        let cfg = GooseDatabricksConfig::from_map(&map);
        // Flat keys win
        assert_eq!(cfg.host.as_deref(), Some("https://flat-host.example"));
        assert_eq!(cfg.model.as_deref(), Some("flat-model"));
    }

    #[test]
    fn from_map_non_databricks_active_provider_is_ignored() {
        // active_provider = anthropic should not trigger nested lookup
        let providers_map = {
            let mut inner = serde_yaml::Mapping::new();
            let mut provider_entry = serde_yaml::Mapping::new();
            provider_entry.insert(
                serde_yaml::Value::String("model".into()),
                serde_yaml::Value::String("claude-opus-4".into()),
            );
            inner.insert(
                serde_yaml::Value::String("anthropic".into()),
                serde_yaml::Value::Mapping(provider_entry),
            );
            serde_yaml::Value::Mapping(inner)
        };

        let map = HashMap::from([
            (
                "active_provider".to_string(),
                serde_yaml::Value::String("anthropic".into()),
            ),
            ("providers".to_string(), providers_map),
        ]);

        let cfg = GooseDatabricksConfig::from_map(&map);
        assert!(cfg.host.is_none());
        assert!(cfg.model.is_none());
    }

    #[test]
    fn load_from_path_returns_none_for_nonexistent_file() {
        let result = GooseDatabricksConfig::load_from_path(std::path::Path::new(
            "/tmp/sprout-test-nonexistent-goose-config-99999999.yaml",
        ));
        assert!(result.is_none());
    }

    #[test]
    fn load_from_path_parses_valid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "DATABRICKS_HOST: https://dbc.example\nGOOSE_PROVIDER: databricks\nGOOSE_MODEL: goose-claude-4-6-sonnet\n",
        )
        .unwrap();
        let cfg = GooseDatabricksConfig::load_from_path(&path).unwrap();
        assert_eq!(cfg.host.as_deref(), Some("https://dbc.example"));
        assert_eq!(cfg.model.as_deref(), Some("goose-claude-4-6-sonnet"));
    }

    #[test]
    fn load_from_path_returns_none_for_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "{{{{not valid yaml at all::::").unwrap();
        let result = GooseDatabricksConfig::load_from_path(&path);
        assert!(result.is_none());
    }

    #[test]
    fn goose_config_path_falls_back_to_home_when_root_unset() {
        // When GOOSE_PATH_ROOT is not set, goose_config_path() constructs a
        // path under $HOME. We can verify the suffix without mutating env vars.
        // If HOME is set (virtually all environments), the path ends with the
        // expected goose config suffix.
        if let Ok(home) = std::env::var("HOME") {
            // Only run the check when GOOSE_PATH_ROOT is not already set, so
            // this test doesn't interfere with the override logic.
            if std::env::var("GOOSE_PATH_ROOT").is_err() {
                let result = goose_config_path();
                let expected = std::path::PathBuf::from(&home)
                    .join(".config")
                    .join("goose")
                    .join("config.yaml");
                assert_eq!(result, Some(expected));
            }
        }
    }
}
