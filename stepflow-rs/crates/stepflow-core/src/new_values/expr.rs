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

use super::JsonPath;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An expression that can be either a literal value or a template expression.
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum ValueExpr {
    /// Reference flow input by name or JSON path.
    Input(JsonPath),
    /// Reference flow variables by name or JSON path.
    Variable {
        variable: JsonPath,

        /// Optional default value to use if the variable is not available.
        ///
        /// This will be preferred over any default value defined in the workflow variable schema.
        default: Option<Value>,
    },
    /// Reference step output with optional JSON path.
    Step { step: String, path: JsonPath },
    /// Literal JSON value.
    Literal(serde_json::Value),
    /// Array of expressions.
    Array(Vec<ValueExpr>),
    /// Object/map of expressions.
    Object(Vec<(String, ValueExpr)>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_tests() {
        panic!("Write tests")
    }
}
