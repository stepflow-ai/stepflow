// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

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
use serde::de::DeserializeOwned;
use std::{fs::File, path::Path};

use crate::{MainError, Result};

/// File format for loading/saving data
#[derive(Debug, Clone)]
pub enum Format {
    Yaml,
    Json,
}

impl Format {
    /// Determine format from file extension
    pub fn from_path(path: &Path) -> Result<Self> {
        let extension = path
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        match extension {
            "yml" | "yaml" => Ok(Self::Yaml),
            "json" => Ok(Self::Json),
            _ => Err(MainError::UnrecognizedFileExtension(path.to_owned()).into()),
        }
    }
}

/// Load and deserialize data from a file (JSON or YAML based on extension)
pub fn load<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let rdr = File::open(path).change_context_lazy(|| MainError::MissingFile(path.to_owned()))?;
    let value = match Format::from_path(path)? {
        Format::Json => serde_json::from_reader(rdr)
            .change_context_lazy(|| MainError::InvalidFile(path.to_owned()))?,
        Format::Yaml => serde_yaml_ng::from_reader(rdr)
            .change_context_lazy(|| MainError::InvalidFile(path.to_owned()))?,
    };
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_format_from_path_yaml() {
        let path = PathBuf::from("test.yaml");
        let format = Format::from_path(&path).unwrap();
        matches!(format, Format::Yaml);

        let path = PathBuf::from("test.yml");
        let format = Format::from_path(&path).unwrap();
        matches!(format, Format::Yaml);
    }

    #[test]
    fn test_format_from_path_json() {
        let path = PathBuf::from("test.json");
        let format = Format::from_path(&path).unwrap();
        matches!(format, Format::Json);
    }

    #[test]
    fn test_format_from_path_unknown() {
        let path = PathBuf::from("test.txt");
        let result = Format::from_path(&path);
        assert!(result.is_err());
    }
}
