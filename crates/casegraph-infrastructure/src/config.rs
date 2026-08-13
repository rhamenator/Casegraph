//! Environment-based configuration with secure local defaults.

use std::collections::HashMap;
use std::env;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};

/// Whether model-provided interpretations may leave the deployment boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelPolicy {
    /// No model provider may be invoked.
    Disabled,
    /// Only an in-process or locally hosted provider may be invoked.
    LocalOnly,
    /// A separately configured allow-listed remote adapter may be invoked.
    AllowListedRemote,
}

/// Diagnostic rendering mode. Neither mode includes source contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogFormat {
    /// One JSON object per diagnostic event.
    Json,
    /// Human-readable local development diagnostics.
    Pretty,
}

/// Validated process configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Root for the SQLite database and immutable artifact store.
    pub data_dir: PathBuf,
    /// Local address used by the HTTP adapter.
    pub bind_addr: SocketAddr,
    /// Upper bound enforced before an artifact is persisted.
    pub max_artifact_bytes: u64,
    /// Privacy policy for optional reasoning providers.
    pub model_policy: ModelPolicy,
    /// Operational diagnostic format.
    pub log_format: LogFormat,
}

/// Actionable configuration validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigError {
    field: &'static str,
    message: String,
}

impl ConfigError {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid {}: {}", self.field, self.message)
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Read and validate `CASEGRAPH_*` environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        let values = env::vars().collect::<HashMap<_, _>>();
        Self::from_map(&values)
    }

    /// Build configuration from a map. Exposed to make startup validation deterministic in tests.
    pub fn from_map(values: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let data_dir = PathBuf::from(value(values, "CASEGRAPH_DATA_DIR", ".casegraph"));
        if data_dir.as_os_str().is_empty() || is_root_path(&data_dir) {
            return Err(ConfigError::new(
                "CASEGRAPH_DATA_DIR",
                "must name a dedicated non-root directory",
            ));
        }

        let bind_addr = value(values, "CASEGRAPH_BIND_ADDR", "127.0.0.1:8080")
            .parse::<SocketAddr>()
            .map_err(|error| ConfigError::new("CASEGRAPH_BIND_ADDR", error.to_string()))?;

        let max_artifact_bytes = value(values, "CASEGRAPH_MAX_ARTIFACT_BYTES", "26214400")
            .parse::<u64>()
            .map_err(|error| ConfigError::new("CASEGRAPH_MAX_ARTIFACT_BYTES", error.to_string()))?;
        if !(1..=1_073_741_824).contains(&max_artifact_bytes) {
            return Err(ConfigError::new(
                "CASEGRAPH_MAX_ARTIFACT_BYTES",
                "must be between 1 byte and 1 GiB",
            ));
        }

        let model_policy = match value(values, "CASEGRAPH_MODEL_POLICY", "disabled").as_str() {
            "disabled" => ModelPolicy::Disabled,
            "local-only" => ModelPolicy::LocalOnly,
            "allow-listed-remote" => ModelPolicy::AllowListedRemote,
            _ => {
                return Err(ConfigError::new(
                    "CASEGRAPH_MODEL_POLICY",
                    "expected disabled, local-only, or allow-listed-remote",
                ));
            }
        };

        let log_format = match value(values, "CASEGRAPH_LOG_FORMAT", "json").as_str() {
            "json" => LogFormat::Json,
            "pretty" => LogFormat::Pretty,
            _ => {
                return Err(ConfigError::new(
                    "CASEGRAPH_LOG_FORMAT",
                    "expected json or pretty",
                ));
            }
        };

        Ok(Self {
            data_dir,
            bind_addr,
            max_artifact_bytes,
            model_policy,
            log_format,
        })
    }
}

fn value(values: &HashMap<String, String>, key: &str, default: &str) -> String {
    values
        .get(key)
        .map_or_else(|| default.to_owned(), Clone::clone)
}

fn is_root_path(path: &Path) -> bool {
    let mut components = path.components();
    match components.next() {
        Some(Component::RootDir) => components.next().is_none(),
        Some(Component::Prefix(_)) => {
            matches!(components.next(), Some(Component::RootDir)) && components.next().is_none()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, LogFormat, ModelPolicy};
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn secure_defaults_disable_models_and_bind_loopback() {
        let config = Config::from_map(&HashMap::new()).expect("defaults should validate");
        assert_eq!(config.data_dir, PathBuf::from(".casegraph"));
        assert!(config.bind_addr.ip().is_loopback());
        assert_eq!(config.model_policy, ModelPolicy::Disabled);
        assert_eq!(config.log_format, LogFormat::Json);
    }

    #[test]
    fn invalid_external_input_is_rejected_with_field_name() {
        let values = HashMap::from([(
            "CASEGRAPH_MAX_ARTIFACT_BYTES".to_owned(),
            "unbounded".to_owned(),
        )]);
        let error = Config::from_map(&values).expect_err("malformed size must fail");
        assert!(error.to_string().contains("CASEGRAPH_MAX_ARTIFACT_BYTES"));
    }

    #[test]
    fn remote_model_use_requires_explicit_policy() {
        let values = HashMap::from([(
            "CASEGRAPH_MODEL_POLICY".to_owned(),
            "allow-listed-remote".to_owned(),
        )]);
        let config = Config::from_map(&values).expect("explicit policy should validate");
        assert_eq!(config.model_policy, ModelPolicy::AllowListedRemote);
    }

    #[test]
    fn filesystem_root_is_not_a_valid_data_directory() {
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        let values = HashMap::from([("CASEGRAPH_DATA_DIR".to_owned(), root.to_owned())]);
        assert!(Config::from_map(&values).is_err());
    }

    #[test]
    fn every_configuration_choice_and_boundary_is_validated() {
        let values = HashMap::from([
            ("CASEGRAPH_DATA_DIR".to_owned(), "fixture-data".to_owned()),
            (
                "CASEGRAPH_BIND_ADDR".to_owned(),
                "127.0.0.1:9191".to_owned(),
            ),
            (
                "CASEGRAPH_MAX_ARTIFACT_BYTES".to_owned(),
                "1073741824".to_owned(),
            ),
            ("CASEGRAPH_MODEL_POLICY".to_owned(), "local-only".to_owned()),
            ("CASEGRAPH_LOG_FORMAT".to_owned(), "pretty".to_owned()),
        ]);
        let config = Config::from_map(&values).expect("valid explicit configuration");
        assert_eq!(config.data_dir, PathBuf::from("fixture-data"));
        assert_eq!(config.bind_addr.port(), 9191);
        assert_eq!(config.max_artifact_bytes, 1_073_741_824);
        assert_eq!(config.model_policy, ModelPolicy::LocalOnly);
        assert_eq!(config.log_format, LogFormat::Pretty);

        for (field, invalid) in [
            ("CASEGRAPH_BIND_ADDR", "not-an-address"),
            ("CASEGRAPH_MAX_ARTIFACT_BYTES", "0"),
            ("CASEGRAPH_MAX_ARTIFACT_BYTES", "1073741825"),
            ("CASEGRAPH_MODEL_POLICY", "remote"),
            ("CASEGRAPH_LOG_FORMAT", "verbose"),
        ] {
            let values = HashMap::from([(field.to_owned(), invalid.to_owned())]);
            let error = Config::from_map(&values).expect_err("invalid configuration must fail");
            assert!(error.to_string().contains(field));
        }
    }
}
