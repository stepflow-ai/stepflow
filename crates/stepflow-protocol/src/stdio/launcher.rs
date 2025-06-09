use std::{ffi::OsString, path::PathBuf, process::Stdio};

use crate::stdio::Result;
use error_stack::ResultExt as _;
use indexmap::IndexMap;
use tokio::process::Child;

use super::StdioError;

/// Helper for launching a sub-process.
pub(crate) struct Launcher {
    working_directory: PathBuf,
    pub command: PathBuf,
    pub args: Vec<OsString>,
    env: IndexMap<String, String>,
}

impl Launcher {
    pub fn try_new(
        working_directory: PathBuf,
        command: String,
        args: Vec<String>,
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
            env,
        })
    }

    pub fn spawn(&self) -> Result<Child> {
        let mut command = tokio::process::Command::new(&self.command);
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true);

        command.current_dir(
            std::env::current_dir()
                .unwrap()
                .join(&self.working_directory),
        );

        // Only pass explicit environment variables through.
        command.env_clear();
        for (key, value) in self.env.iter() {
            // TODO: Allow value to be a template referencing parent environment variables.
            command.env(key, value);
        }

        tracing::info!("Spawning child process: {:?}", command);
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
