// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use error_stack::ResultExt as _;
use std::{fs::File, path::PathBuf};

use crate::{MainError, Result, args::file_loader::Format};

/// Shared output arguments used by multiple commands
#[derive(clap::Args, Debug, Clone)]
pub struct OutputArgs {
    /// Path to write the output to.
    ///
    /// If not set, will write to stdout.
    #[arg(long = "output", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub output_path: Option<PathBuf>,
}

/// Write output to file or stdout
fn write_output_impl(path: Option<PathBuf>, output: impl serde::Serialize) -> Result<()> {
    match path {
        Some(path) => {
            let format = Format::from_path(&path)?;
            let wtr = File::create(&path)
                .change_context_lazy(|| MainError::CreateOutput(path.clone()))?;
            match format {
                Format::Json => serde_json::to_writer(wtr, &output)
                    .change_context_lazy(|| MainError::WriteOutput(path.clone()))?,
                Format::Yaml => serde_yaml_ng::to_writer(wtr, &output)
                    .change_context_lazy(|| MainError::WriteOutput(path.clone()))?,
            };
            Ok(())
        }
        None => {
            serde_json::to_writer(std::io::stdout(), &output)
                .change_context_lazy(|| MainError::WriteOutput(PathBuf::from("stdout")))?;
            Ok(())
        }
    }
}

impl OutputArgs {
    /// Write output using the configured path or stdout
    pub fn write_output(&self, output: impl serde::Serialize) -> Result<()> {
        write_output_impl(self.output_path.clone(), output)
    }

    /// Create OutputArgs with a specific path
    pub fn with_path(output_path: Option<PathBuf>) -> Self {
        Self { output_path }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_output_args_default() {
        let args = OutputArgs { output_path: None };
        assert!(args.output_path.is_none());
    }

    #[test]
    fn test_output_args_with_path() {
        let path = PathBuf::from("output.json");
        let args = OutputArgs::with_path(Some(path.clone()));
        assert_eq!(args.output_path, Some(path));
    }

    #[test]
    fn test_output_args_with_path_none() {
        let args = OutputArgs::with_path(None);
        assert!(args.output_path.is_none());
    }
}
