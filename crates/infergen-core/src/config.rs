//! Infergen project configuration (`infergen.config.{json,toml}`).
//!
//! The config is engine contract: `scan`, `generate`, and `check` all read it.
//! It is discovered in the project root, parsed by file extension, and
//! deserialized with serde defaults so partial configs load cleanly.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::detect::{Framework, Language};

/// Candidate config filenames, in discovery precedence order.
pub const CONFIG_FILENAMES: &[&str] = &["infergen.config.json", "infergen.config.toml"];

/// Serialized on-disk format, chosen by `init` / inferred from extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    /// JSON (`infergen.config.json`).
    Json,
    /// TOML (`infergen.config.toml`).
    Toml,
}

impl ConfigFormat {
    /// Infer the format from a path's extension. `None` for unknown extensions.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<ConfigFormat> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "json" => Some(ConfigFormat::Json),
            "toml" => Some(ConfigFormat::Toml),
            _ => None,
        }
    }
}

/// Top-level Infergen project configuration.
///
/// Field order is TOML-safe: scalars and arrays precede the `naming` table and
/// the `providers` array-of-tables, so `toml::to_string_pretty` never nests a
/// later scalar under an earlier table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Config-file schema version (see [`crate::CONFIG_SCHEMA_VERSION`]).
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Path to the event catalog, relative to the project root.
    #[serde(default = "default_catalog_path")]
    pub catalog: PathBuf,
    /// Output directory for the generated SDK, relative to the project root.
    #[serde(default = "default_output_dir")]
    pub output: PathBuf,
    /// Detected/declared source languages.
    #[serde(default)]
    pub languages: Vec<Language>,
    /// Detected/declared frameworks.
    #[serde(default)]
    pub frameworks: Vec<Framework>,
    /// Naming-convention settings.
    #[serde(default)]
    pub naming: NamingConfig,
    /// Configured destinations/providers (empty until Milestone 3).
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

/// Naming convention for generated event names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingConfig {
    /// Convention template, e.g. `entity.action.state` (PRD §10 default).
    #[serde(default = "default_convention")]
    pub convention: String,
    /// Casing applied to identifiers, e.g. `snake_case`.
    #[serde(default = "default_case")]
    pub case: String,
}

/// A configured destination/provider entry. Shape firms up in M3 (E3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Provider identifier, e.g. `posthog`, `segment`, `file`.
    pub name: String,
}

fn default_schema_version() -> u32 {
    crate::CONFIG_SCHEMA_VERSION
}

fn default_catalog_path() -> PathBuf {
    PathBuf::from(".infergen/catalog.yaml")
}

fn default_output_dir() -> PathBuf {
    PathBuf::from("infergen/generated")
}

fn default_convention() -> String {
    "entity.action.state".to_string()
}

fn default_case() -> String {
    "snake_case".to_string()
}

impl Default for NamingConfig {
    fn default() -> Self {
        NamingConfig {
            convention: default_convention(),
            case: default_case(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            schema_version: default_schema_version(),
            catalog: default_catalog_path(),
            output: default_output_dir(),
            languages: Vec::new(),
            frameworks: Vec::new(),
            naming: NamingConfig::default(),
            providers: Vec::new(),
        }
    }
}

impl Config {
    /// Find the first existing config file in `dir` (precedence per
    /// [`CONFIG_FILENAMES`]). Returns the full path, or `None` if none exist.
    #[must_use]
    pub fn discover(dir: &Path) -> Option<PathBuf> {
        CONFIG_FILENAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())
    }

    /// Load and parse a config file, dispatching on its extension.
    ///
    /// # Errors
    /// - [`Error::UnsupportedFormat`] if the extension is not `.json`/`.toml`.
    /// - [`Error::Io`] if the file cannot be read.
    /// - [`Error::ConfigParse`] if the contents are malformed.
    pub fn load(path: &Path) -> Result<Config, Error> {
        // Resolve format first so an unsupported extension does not surface as
        // an I/O "not found" error.
        let format = ConfigFormat::from_path(path).ok_or_else(|| Error::UnsupportedFormat {
            path: path.to_path_buf(),
        })?;
        let text = std::fs::read_to_string(path)?;
        let config = match format {
            ConfigFormat::Json => serde_json::from_str(&text).map_err(|e| Error::ConfigParse {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?,
            ConfigFormat::Toml => toml::from_str(&text).map_err(|e| Error::ConfigParse {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?,
        };
        Ok(config)
    }

    /// Discover then load a config from `dir`.
    ///
    /// # Errors
    /// - [`Error::ConfigNotFound`] if no config file exists in `dir`.
    /// - Any error from [`Config::load`].
    pub fn load_from_dir(dir: &Path) -> Result<Config, Error> {
        let path = Config::discover(dir).ok_or_else(|| Error::ConfigNotFound {
            dir: dir.to_path_buf(),
        })?;
        Config::load(&path)
    }

    /// Serialize to `path`, format inferred from the extension. Creates parent
    /// directories as needed.
    ///
    /// # Errors
    /// - [`Error::UnsupportedFormat`] if the extension is not `.json`/`.toml`.
    /// - [`Error::ConfigParse`] if serialization fails.
    /// - [`Error::Io`] if the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let format = ConfigFormat::from_path(path).ok_or_else(|| Error::UnsupportedFormat {
            path: path.to_path_buf(),
        })?;
        let text = match format {
            ConfigFormat::Json => {
                let mut s = serde_json::to_string_pretty(self).map_err(|e| Error::ConfigParse {
                    path: path.to_path_buf(),
                    message: format!("serialize: {e}"),
                })?;
                s.push('\n');
                s
            }
            ConfigFormat::Toml => toml::to_string_pretty(self).map_err(|e| Error::ConfigParse {
                path: path.to_path_buf(),
                message: format!("serialize: {e}"),
            })?,
        };
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_config_has_expected_paths() {
        let c = Config::default();
        assert_eq!(c.catalog, PathBuf::from(".infergen/catalog.yaml"));
        assert_eq!(c.output, PathBuf::from("infergen/generated"));
        assert_eq!(c.naming.convention, "entity.action.state");
        assert_eq!(c.naming.case, "snake_case");
        assert_eq!(c.schema_version, crate::CONFIG_SCHEMA_VERSION);
    }

    #[test]
    fn empty_json_deserializes_to_defaults() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn json_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("infergen.config.json");
        let c = Config::default();
        c.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded, c);
    }

    #[test]
    fn toml_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("infergen.config.toml");
        let mut c = Config::default();
        c.languages.push(Language::TypeScript);
        c.frameworks.push(Framework::NextJs);
        c.providers.push(ProviderConfig {
            name: "posthog".to_string(),
        });
        c.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded, c);
    }

    #[test]
    fn discover_precedence() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("infergen.config.json"), "{}").unwrap();
        std::fs::write(dir.path().join("infergen.config.toml"), "").unwrap();
        let found = Config::discover(dir.path()).unwrap();
        assert_eq!(found, dir.path().join("infergen.config.json"));
    }

    #[test]
    fn load_from_dir_not_found() {
        let dir = tempdir().unwrap();
        let err = Config::load_from_dir(dir.path()).unwrap_err();
        assert!(matches!(err, Error::ConfigNotFound { .. }));
    }

    #[test]
    fn load_unsupported_extension() {
        let err = Config::load(Path::new("x.yaml")).unwrap_err();
        assert!(matches!(err, Error::UnsupportedFormat { .. }));
    }

    #[test]
    fn load_malformed_json() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("infergen.config.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, Error::ConfigParse { .. }));
    }

    #[test]
    fn camel_case_keys() {
        let s = serde_json::to_string_pretty(&Config::default()).unwrap();
        assert!(s.contains("schemaVersion"));
        assert!(!s.contains("schema_version"));
    }
}
