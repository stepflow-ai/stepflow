use error_stack::{self, ensure};
use std::path::{Path, PathBuf};

use crate::{MainError, Result, args::file_loader::load, stepflow_config::StepflowConfig};

/// Shared config arguments related to locating stepflow config.
#[derive(clap::Args, Debug, Clone)]
pub struct ConfigArgs {
    /// The path to the stepflow config file.
    ///
    /// If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file.
    /// If that isn't found, will also look in the current directory.
    #[arg(long="config", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub config_path: Option<PathBuf>,
}

// Look for any of these file names.
const FILE_NAMES: &[&str] = &[
    "stepflow-config.yml",
    "stepflow-config.yaml",
    "stepflow_config.yml",
    "stepflow_config.yaml",
];

/// Locate a config file in the given directory or current directory
fn locate_config(directory: Option<&Path>) -> Result<Option<PathBuf>> {
    // First look for any of the file names in the `directory`.
    if let Some(directory) = directory {
        let mut file_names = FILE_NAMES
            .iter()
            .map(|name| directory.join(name))
            .filter(|path| path.is_file());
        if let Some(path) = file_names.next() {
            // If there are multiple, it is ambiguous so report an error.
            ensure!(
                file_names.next().is_none(),
                MainError::MultipleStepflowConfigs(directory.to_owned())
            );
            return Ok(Some(path.to_owned()));
        }
    }

    // Then, look for any of the file names in the current directory.
    let mut file_names = FILE_NAMES
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    if let Some(path) = file_names.next() {
        // If there are multiple, it is ambiguous so report an error.
        error_stack::ensure!(
            file_names.next().is_none(),
            MainError::MultipleStepflowConfigs(std::env::current_dir().unwrap()),
        );
        return Ok(Some(path.to_owned()));
    }

    Ok(None)
}

/// Attempt to load a config file from `config_path`.
///
/// If that is not set, look either in the `flow_directory` or the current directory.
/// If no config file is found, return a default config with builtins only.
fn load_config_impl(
    flow_directory: Option<&Path>,
    mut config_path: Option<PathBuf>,
) -> Result<StepflowConfig> {
    if config_path.is_none() {
        config_path = locate_config(flow_directory)?;
    }

    tracing::info!("Loading config from {:?}", config_path);

    let mut config = if let Some(config_path) = config_path.as_ref() {
        load(config_path)?
    } else {
        tracing::info!("No config file found, using default config with builtins only");
        StepflowConfig::default()
    };

    if config.working_directory.is_none() {
        if let Some(config_dir) = config_path.as_ref().and_then(|p| p.parent()) {
            config.working_directory = Some(config_dir.to_owned());
        } else if let Some(flow_dir) = flow_directory {
            config.working_directory = Some(flow_dir.to_owned());
        } else {
            // If no config path or flow directory, use current directory
            config.working_directory =
                Some(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        }
    }
    tracing::info!("Using working directory: {:?}", config.working_directory);
    Ok(config)
}

impl ConfigArgs {
    /// Load config using the provided path or auto-detection
    ///
    /// # Arguments
    ///
    /// * `flow_directory` - Optional directory containing the workflow file for config auto-detection
    pub fn load_config(&self, flow_directory: Option<&Path>) -> Result<StepflowConfig> {
        load_config_impl(flow_directory, self.config_path.clone())
    }

    /// Create ConfigArgs with a specific path
    pub fn with_path(config_path: Option<PathBuf>) -> Self {
        Self { config_path }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_config_args_default() {
        let args = ConfigArgs { config_path: None };
        assert!(args.config_path.is_none());
    }

    #[test]
    fn test_config_args_with_path() {
        let path = PathBuf::from("custom-config.yml");
        let args = ConfigArgs::with_path(Some(path.clone()));
        assert_eq!(args.config_path, Some(path));
    }

    #[test]
    fn test_config_args_with_path_none() {
        let args = ConfigArgs::with_path(None);
        assert!(args.config_path.is_none());
    }
}
