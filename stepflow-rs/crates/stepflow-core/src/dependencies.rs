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

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;

use crate::workflow::{BaseRef, Expr, ValueRef, WorkflowRef};

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

    /// Get the set of step IDs this value depends on.
    pub fn step_dependency_set(&self) -> HashSet<String> {
        self.step_dependencies().map(|s| s.to_string()).collect()
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

/// Result of parsing a value for dependency analysis
enum ParseResult<'a> {
    /// A literal. Dependencies inside the value should be ignored.
    Literal,
    /// An expression.
    Expr(Expr),
    /// A value. Dependencies inside the value should be extracted.
    Value(&'a serde_json::Value),
}

impl<'a> TryFrom<&'a ValueRef> for ParseResult<'a> {
    type Error = error_stack::Report<DependencyError>;

    fn try_from(value: &'a ValueRef) -> Result<Self, Self::Error> {
        if let Some(fields) = value.as_object() {
            if fields.contains_key("$literal") {
                Ok(ParseResult::Literal)
            } else if fields.contains_key("$from") {
                let expr: Expr = serde_json::from_value(value.as_ref().clone()).map_err(|e| {
                    error_stack::report!(DependencyError::ParseError)
                        .attach_printable(format!("Failed to parse expression: {e}"))
                })?;
                Ok(ParseResult::Expr(expr))
            } else {
                Ok(ParseResult::Value(value.as_ref()))
            }
        } else {
            Ok(ParseResult::Value(value.as_ref()))
        }
    }
}

impl<'a> TryFrom<&'a serde_json::Value> for ParseResult<'a> {
    type Error = error_stack::Report<DependencyError>;

    fn try_from(value: &'a serde_json::Value) -> Result<Self, Self::Error> {
        if let Some(fields) = value.as_object() {
            if fields.contains_key("$literal") {
                Ok(ParseResult::Literal)
            } else if fields.contains_key("$from") {
                let expr: Expr = serde_json::from_value(value.clone()).map_err(|e| {
                    error_stack::report!(DependencyError::ParseError)
                        .attach_printable(format!("Failed to parse expression: {e}"))
                })?;
                Ok(ParseResult::Expr(expr))
            } else {
                Ok(ParseResult::Value(value))
            }
        } else {
            Ok(ParseResult::Value(value))
        }
    }
}

/// Extract detailed dependency information from a ValueRef
pub fn extract_value_dependencies(
    value_ref: &ValueRef,
) -> Result<ValueDependencies, error_stack::Report<DependencyError>> {
    match ParseResult::try_from(value_ref)? {
        ParseResult::Literal => Ok(ValueDependencies::Other(HashSet::new())),
        ParseResult::Expr(expr) => {
            let mut deps = HashSet::new();
            if let Some(dep) = extract_dependency_from_expr(&expr)? {
                deps.insert(dep);
            } else {
                return Err(error_stack::report!(DependencyError::MalformedReference)
                    .attach_printable(format!(
                        "Found object with '$from' key that parsed as a literal expression: {value_ref:?}"
                    )));
            }
            Ok(ValueDependencies::Other(deps))
        }
        ParseResult::Value(serde_json::Value::Object(fields)) => {
            let fields = fields
                .iter()
                .map(|(f, v)| {
                    let mut deps = HashSet::new();
                    extract_dependencies_from_value(v, &mut deps)?;
                    Ok((f.to_owned(), deps))
                })
                .collect::<Result<IndexMap<_, _>, error_stack::Report<DependencyError>>>()?;
            Ok(ValueDependencies::Object(fields))
        }
        ParseResult::Value(v) => {
            let mut deps = HashSet::new();
            extract_dependencies_from_value(v, &mut deps)?;
            Ok(ValueDependencies::Other(deps))
        }
    }
}

/// Extract dependencies from a JSON value
fn extract_dependencies_from_value(
    value: &serde_json::Value,
    deps: &mut HashSet<Dependency>,
) -> Result<(), error_stack::Report<DependencyError>> {
    match ParseResult::try_from(value)? {
        ParseResult::Literal => {}
        ParseResult::Expr(expr) => {
            if let Some(dep) = extract_dependency_from_expr(&expr)? {
                deps.insert(dep);
            }
        }
        ParseResult::Value(serde_json::Value::Object(fields)) => {
            for val in fields.values() {
                extract_dependencies_from_value(val, deps)?;
            }
        }
        ParseResult::Value(serde_json::Value::Array(elements)) => {
            for val in elements {
                extract_dependencies_from_value(val, deps)?;
            }
        }
        ParseResult::Value(_) => {}
    }
    Ok(())
}

/// Extract a dependency from an expression
fn extract_dependency_from_expr(
    expr: &Expr,
) -> Result<Option<Dependency>, error_stack::Report<DependencyError>> {
    match expr {
        Expr::Ref {
            from,
            path,
            on_skip,
        } => {
            let optional = on_skip.is_optional();
            match from {
                BaseRef::Step { step } => Ok(Some(Dependency::StepOutput {
                    step_id: step.clone(),
                    field: path.clone(),
                    optional,
                })),
                BaseRef::Workflow(WorkflowRef::Input) => Ok(Some(Dependency::FlowInput {
                    field: path.clone(),
                })),
            }
        }
        Expr::Literal(_) => Ok(None),
    }
}

/// Error types for dependency analysis
#[derive(Debug, thiserror::Error)]
pub enum DependencyError {
    #[error("Failed to parse expression")]
    ParseError,
    #[error("Malformed reference")]
    MalformedReference,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_simple_step_dependency() {
        let value = ValueRef::new(json!({
            "$from": {"step": "step1"}
        }));

        let deps = extract_value_dependencies(&value).unwrap();
        let step_deps = deps.step_dependency_set();
        let expected: HashSet<String> = ["step1"].iter().map(|s| s.to_string()).collect();
        assert_eq!(step_deps, expected);
    }

    #[test]
    fn test_extract_object_dependencies() {
        let value = ValueRef::new(json!({
            "field1": {"$from": {"step": "step1"}},
            "field2": {"$from": {"step": "step2"}},
            "literal": "value"
        }));

        let deps = extract_value_dependencies(&value).unwrap();
        let step_deps = deps.step_dependency_set();

        match &deps {
            ValueDependencies::Object(fields) => {
                assert_eq!(fields.len(), 3);
                // field1 should have step1 dependency
                assert_eq!(fields["field1"].len(), 1);
                // field2 should have step2 dependency
                assert_eq!(fields["field2"].len(), 1);
                // literal field should have no dependencies
                assert_eq!(fields["literal"].len(), 0);
            }
            _ => panic!("Expected Object dependencies"),
        }
        let expected: HashSet<String> = ["step1", "step2"].iter().map(|s| s.to_string()).collect();
        assert_eq!(step_deps, expected);
    }

    #[test]
    fn test_extract_flow_input_dependency() {
        let value = ValueRef::new(json!({
            "$from": {"workflow": "input"}
        }));

        let deps = extract_value_dependencies(&value).unwrap();
        // Should have no step dependencies (flow input dependency)
        let step_deps = deps.step_dependency_set();
        assert!(step_deps.is_empty());

        // But should have a flow input dependency
        let all_deps: Vec<_> = deps.dependencies().collect();
        assert_eq!(all_deps.len(), 1);
        assert!(matches!(all_deps[0], Dependency::FlowInput { .. }));
    }
}
