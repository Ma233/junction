use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RawConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse YAML config: {0}")]
    ParseError(#[from] serde_yaml::Error),
}

#[derive(Debug, Error)]
pub enum ResolvedConfigError {
    #[error("Duplicate public key found: {0}")]
    DuplicatePublicKey(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub outputs: Vec<OutputConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResolvedConfig {
    pub outputs: HashMap<String, OutputConfig>,
    pub data_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OutputConfig {
    pub slug: String,
    pub cmd: String,
    pub args: Vec<String>,
}

impl OutputConfig {
    pub fn get_command_parts(&self) -> (String, Vec<String>) {
        (self.cmd.clone(), self.args.clone())
    }
}

impl Config {
    pub fn from_yaml_file(path: impl AsRef<Path>) -> Result<Self, RawConfigError> {
        let file = std::fs::File::open(path)?;
        let config = serde_yaml::from_reader(file)?;
        Ok(config)
    }
    pub fn from_yaml_str(yaml: &str) -> Result<Self, RawConfigError> {
        let config = serde_yaml::from_str(yaml)?;
        Ok(config)
    }
}

impl ResolvedConfig {
    pub fn get_output_by_slug(&self, slug: &str) -> Option<&OutputConfig> {
        self.outputs.get(slug)
    }
}

impl ResolvedConfig {
    pub fn new(config: Config, data_dir: PathBuf) -> Result<Self, ResolvedConfigError> {
        let mut outputs = HashMap::new();

        for output in config.outputs {
            if outputs.contains_key(&output.slug) {
                return Err(ResolvedConfigError::DuplicatePublicKey(output.slug));
            }

            outputs.insert(output.slug.clone(), output);
        }

        Ok(ResolvedConfig { outputs, data_dir })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn sample_output_config() -> OutputConfig {
        OutputConfig {
            slug: "test-output".to_string(),
            cmd: "echo".to_string(),
            args: vec!["hello".to_string()],
        }
    }

    fn sample_config() -> Config {
        Config {
            outputs: vec![sample_output_config()],
        }
    }

    #[test]
    fn test_output_config_get_command_parts() {
        let output = OutputConfig {
            slug: "test".to_string(),
            cmd: "ls".to_string(),
            args: vec!["-la".to_string(), "/tmp".to_string()],
        };

        let (cmd, args) = output.get_command_parts();
        assert_eq!(cmd, "ls");
        assert_eq!(args, vec!["-la", "/tmp"]);
    }

    #[test]
    fn test_config_from_yaml_str() {
        let yaml = r#"
outputs:
  - slug: "test"
    cmd: "echo"
    args: ["hello", "world"]
"#;

        let config = Config::from_yaml_str(yaml).unwrap();
        assert_eq!(config.outputs.len(), 1);
        assert_eq!(config.outputs[0].slug, "test");
        assert_eq!(config.outputs[0].cmd, "echo");
        assert_eq!(config.outputs[0].args, vec!["hello", "world"]);
    }

    #[test]
    fn test_config_from_yaml_str_invalid() {
        let yaml = "invalid: yaml: content: [";
        let result = Config::from_yaml_str(yaml);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RawConfigError::ParseError(_)));
    }

    #[test]
    fn test_resolved_config_new() {
        let config = sample_config();
        let data_dir = PathBuf::from("/test/data");

        let resolved = ResolvedConfig::new(config, data_dir.clone()).unwrap();
        assert_eq!(resolved.data_dir, data_dir);
        assert_eq!(resolved.outputs.len(), 1);
        assert!(resolved.outputs.contains_key("test-output"));
    }

    #[test]
    fn test_resolved_config_new_duplicate_slug() {
        let config = Config {
            outputs: vec![
                OutputConfig {
                    slug: "duplicate".to_string(),
                    cmd: "echo".to_string(),
                    args: vec!["first".to_string()],
                },
                OutputConfig {
                    slug: "duplicate".to_string(),
                    cmd: "echo".to_string(),
                    args: vec!["second".to_string()],
                },
            ],
        };
        let data_dir = PathBuf::from("/test/data");

        let result = ResolvedConfig::new(config, data_dir);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResolvedConfigError::DuplicatePublicKey(slug) if slug == "duplicate"
        ));
    }

    #[test]
    fn test_resolved_config_get_output_by_slug() {
        let config = sample_config();
        let data_dir = PathBuf::from("/test/data");
        let resolved = ResolvedConfig::new(config, data_dir).unwrap();

        let output = resolved.get_output_by_slug("test-output");
        assert!(output.is_some());
        assert_eq!(output.unwrap().slug, "test-output");

        let missing = resolved.get_output_by_slug("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_config_from_yaml_file_not_found() {
        let result = Config::from_yaml_file("/nonexistent/file.yaml");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RawConfigError::IoError(_)));
    }

    #[test]
    fn test_multiple_outputs() {
        let yaml = r#"
outputs:
  - slug: "first"
    cmd: "echo"
    args: ["first"]
  - slug: "second"
    cmd: "ls"
    args: ["-la"]
"#;

        let config = Config::from_yaml_str(yaml).unwrap();
        let data_dir = PathBuf::from("/test");
        let resolved = ResolvedConfig::new(config, data_dir).unwrap();

        assert_eq!(resolved.outputs.len(), 2);
        assert!(resolved.outputs.contains_key("first"));
        assert!(resolved.outputs.contains_key("second"));

        let first = resolved.get_output_by_slug("first").unwrap();
        assert_eq!(first.cmd, "echo");
        assert_eq!(first.args, vec!["first"]);

        let second = resolved.get_output_by_slug("second").unwrap();
        assert_eq!(second.cmd, "ls");
        assert_eq!(second.args, vec!["-la"]);
    }

    #[test]
    fn test_empty_args() {
        let yaml = r#"
outputs:
  - slug: "no-args"
    cmd: "pwd"
    args: []
"#;

        let config = Config::from_yaml_str(yaml).unwrap();
        assert_eq!(config.outputs[0].args.len(), 0);
    }
}
