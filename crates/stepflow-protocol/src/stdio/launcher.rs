use std::{ffi::OsString, path::PathBuf, process::Stdio};

use crate::stdio::Result;
use error_stack::ResultExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tokio::process::Child;

use super::StdioError;

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum InheritEnv {
    /// List of environment variables to inherit.
    Whitelist(Vec<String>),
    /// Inherit all environment variables or none.
    AllOrNone(bool),
}

impl Default for InheritEnv {
    fn default() -> Self {
        Self::AllOrNone(false)
    }
}

impl InheritEnv {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::AllOrNone(true))
    }
}

/// Helper for launching a sub-process.
pub(crate) struct Launcher {
    working_directory: PathBuf,
    pub command: PathBuf,
    pub args: Vec<OsString>,
    inherit_env: InheritEnv,
    env: IndexMap<String, String>,
}

impl Launcher {
    pub fn try_new(
        working_directory: PathBuf,
        command: String,
        args: Vec<String>,
        inherit_env: InheritEnv,
        env: IndexMap<String, String>,
    ) -> Result<Self> {
        let command = which::WhichConfig::new()
            .system_path_list()
            .custom_cwd(working_directory.clone())
            .binary_name(command.clone().into())
            .first_result()
            .change_context_lazy(|| StdioError::MissingCommand(command))?;
        error_stack::ensure!(command.is_file(), StdioError::InvalidCommand(command));

        Ok(Self {
            working_directory,
            command,
            args: args.into_iter().map(|s| s.into()).collect(),
            inherit_env,
            env,
        })
    }

    pub fn spawn(&self) -> Result<Child> {
        let mut command = tokio::process::Command::new(&self.command);
        command
            .current_dir(&self.working_directory)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true);

        // First setup the inheritted environment.
        match &self.inherit_env {
            InheritEnv::AllOrNone(true) => {
                // Nothing to filter.
            }
            InheritEnv::AllOrNone(false) => {
                command.env_clear();
            }
            InheritEnv::Whitelist(whitelist) => {
                command.env_clear();
                for key in whitelist {
                    if let Some(value) = std::env::var_os(key) {
                        command.env(key, value);
                    }
                }
            }
        }

        // Then setup oter environment variables.
        for (key, value) in self.env.iter() {
            command.env(key, value);
        }

        // Finally, spawn the child process.
        match command.spawn() {
            Ok(child) => Ok(child),
            Err(e) => {
                tracing::error!(
                    "Failed to spawn child process '{} {:?}': {e}",
                    self.command.display(),
                    self.args
                );

                Err(
                    error_stack::report!(StdioError::Spawn).attach_printable(format!(
                        "Failed to spawn '{} {:?}",
                        self.command.display(),
                        self.args
                    )),
                )
            }
        }
    }
}
