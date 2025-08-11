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
