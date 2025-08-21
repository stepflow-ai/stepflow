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


use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;

/// How a value receives its input data
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ValueDependencies {
    /// Value is an object with named fields
    Object(IndexMap<String, HashSet<Dependency>>),
    /// Value is not an object (single value, array, etc.)
    Other(HashSet<Dependency>),
}

impl ValueDependencies {
    /// Iterator over all dependencies of this value.
    pub fn dependencies(&self) -> Box<dyn Iterator<Item = &Dependency> + '_> {
        match self {
            ValueDependencies::Object(map) => Box::new(map.values().flatten()),
            ValueDependencies::Other(set) => Box::new(set.iter()),
        }
    }

    /// Iterator over all steps this value depends on.
    pub fn step_dependencies(&self) -> impl Iterator<Item = &str> {
        self.dependencies().filter_map(Dependency::step_id)
    }
}

/// Source of input data
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Hash, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum Dependency {
    /// Comes from workflow input
    #[serde(rename_all = "camelCase")]
    FlowInput {
        /// Optional field path within workflow input
        field: Option<String>,
    },
    /// Comes from another step's output
    #[serde(rename_all = "camelCase")]
    StepOutput {
        /// Which step produces this data
        step_id: String,
        /// Optional field path within step output
        field: Option<String>,
        /// If true, the step_id may be skipped and this step still executed.
        optional: bool,
    },
}

impl Dependency {
    /// Return the step this dependency is on, if any.
    pub fn step_id(&self) -> Option<&str> {
        match self {
            Dependency::FlowInput { .. } => None,
            Dependency::StepOutput { step_id, .. } => Some(step_id),
        }
    }
}
