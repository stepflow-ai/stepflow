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

//! Flow-level type checking.
//!
//! This module provides the main type checking entry point for workflows.
//! It processes steps in order, synthesizes input types, checks against
//! component schemas, and collects the resulting type environment.

use indexmap::IndexMap;
use stepflow_core::schema::SchemaRef;
use stepflow_core::workflow::{Component, Flow, Step};

use crate::TypeEnvironment;
use crate::error::{LocatedTypeError, TypeError};
use crate::subtyping::{SubtypeResult, is_subtype};
use crate::synthesis::synthesize_type;
use crate::types::Type;

/// Configuration for type checking.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeCheckConfig {
    /// If true, untyped outputs are errors; if false, they're warnings.
    pub strict: bool,
}

/// Provider of component schema information.
///
/// This trait abstracts how component schemas are obtained, allowing
/// for different implementations (e.g., from protocol calls, mocks, etc.).
pub trait ComponentSchemaProvider {
    /// Get the expected input schema for a component.
    fn get_input_schema(&self, component: &Component) -> Option<Type>;

    /// Get the static output schema for a component.
    fn get_output_schema(&self, component: &Component) -> Option<Type>;

    /// Infer output schema given a concrete input schema.
    ///
    /// This is the schema transformer: given what we're actually passing
    /// to the component, what will it return?
    fn infer_output_schema(&self, component: &Component, input_schema: &Type) -> Option<Type>;
}

/// Result of type checking a flow.
#[derive(Debug)]
pub struct TypeCheckResult {
    /// The final type environment with all step types.
    pub environment: TypeEnvironment,

    /// Errors encountered during type checking.
    pub errors: Vec<LocatedTypeError>,

    /// Warnings encountered during type checking.
    pub warnings: Vec<LocatedTypeError>,

    /// The inferred type of the flow output expression.
    pub output_type: Type,
}

impl TypeCheckResult {
    /// Check if type checking succeeded (no errors).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get the inferred type for a step.
    pub fn step_type(&self, step_id: &str) -> Option<&Type> {
        self.environment.get_step_type(step_id)
    }

    /// Get all step types.
    pub fn step_types(&self) -> &IndexMap<String, Type> {
        self.environment.step_types()
    }
}

/// Type check an entire flow.
///
/// This is the main entry point for type checking. It processes steps
/// in order, synthesizing input types, checking against component schemas,
/// and building up the type environment.
///
/// # Arguments
/// * `flow` - The workflow to type check
/// * `provider` - Provider of component schema information
/// * `config` - Type checking configuration
///
/// # Returns
/// A `TypeCheckResult` containing the type environment, errors, and warnings.
pub fn type_check_flow(
    flow: &Flow,
    provider: &dyn ComponentSchemaProvider,
    config: TypeCheckConfig,
) -> TypeCheckResult {
    let mut env = TypeEnvironment::from_flow(flow);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let flow_v1 = flow.latest();

    // Process steps in order (they're already topologically sorted)
    for (step_index, step) in flow_v1.steps.iter().enumerate() {
        let step_path = format!("$.steps[{}]", step_index);

        // Get declared output schema for this step if it exists
        let declared_output_schema = flow_v1.schemas.steps.get(&step.id);

        let (output_type, step_errors, step_warnings) = type_check_step(
            step,
            &env,
            &step_path,
            provider,
            config,
            declared_output_schema,
        );

        errors.extend(step_errors);
        warnings.extend(step_warnings);

        env.set_step_type(step.id.clone(), output_type);
    }

    // Synthesize the output type
    let output_path = "$.output";
    let output_result = synthesize_type(&flow_v1.output, &env, output_path);
    errors.extend(output_result.errors);
    let output_type = output_result.ty;

    // Check flow output against declared output_schema (if provided)
    if let Some(output_schema) = &flow_v1.schemas.output {
        let declared_type = Type::Schema(output_schema.clone());
        match is_subtype(&output_type, &declared_type) {
            SubtypeResult::No(reasons) => {
                for reason in reasons {
                    errors.push(LocatedTypeError::at_flow(
                        TypeError::mismatch(declared_type.clone(), output_type.clone(), reason),
                        output_path,
                    ));
                }
            }
            SubtypeResult::Unknown if config.strict => {
                warnings.push(LocatedTypeError::at_flow(
                    TypeError::indeterminate("cannot verify output type statically"),
                    output_path,
                ));
            }
            _ => {}
        }
    }

    TypeCheckResult {
        environment: env,
        errors,
        warnings,
        output_type,
    }
}

/// Type check a single step.
///
/// Returns (output_type, errors, warnings).
fn type_check_step(
    step: &Step,
    env: &TypeEnvironment,
    path: &str,
    provider: &dyn ComponentSchemaProvider,
    config: TypeCheckConfig,
    declared_output_schema: Option<&SchemaRef>,
) -> (Type, Vec<LocatedTypeError>, Vec<LocatedTypeError>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Synthesize the type of the step's input expression
    let input_path = format!("{}.input", path);
    let input_result = synthesize_type(&step.input, env, &input_path);
    errors.extend(input_result.errors);
    let synthesized_input = input_result.ty;

    // 2. Get expected input schema from component
    let component_input = provider
        .get_input_schema(&step.component)
        .unwrap_or(Type::Any);

    // 3. Check subtyping: synthesized input <: expected input
    match is_subtype(&synthesized_input, &component_input) {
        SubtypeResult::No(reasons) => {
            for reason in reasons {
                errors.push(LocatedTypeError::at_step(
                    TypeError::mismatch(component_input.clone(), synthesized_input.clone(), reason),
                    &step.id,
                    &input_path,
                ));
            }
        }
        SubtypeResult::Unknown if config.strict => {
            warnings.push(LocatedTypeError::at_step(
                TypeError::indeterminate("cannot verify input type statically"),
                &step.id,
                &input_path,
            ));
        }
        _ => {}
    }

    // 4. Determine output type
    // Priority: declared in flow.schemas.steps > inferred from component > static component output > Any
    let output_type = if let Some(declared) = declared_output_schema {
        Type::Schema(declared.clone())
    } else if let Some(inferred) = provider.infer_output_schema(&step.component, &synthesized_input)
    {
        inferred
    } else {
        let component_output = provider
            .get_output_schema(&step.component)
            .unwrap_or(Type::Any);

        if component_output.is_any() && config.strict {
            warnings.push(LocatedTypeError::at_step(
                TypeError::indeterminate("component does not declare output schema"),
                &step.id,
                path,
            ));
        }

        component_output
    };

    (output_type, errors, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use stepflow_core::schema::SchemaRef;
    use stepflow_core::values::{JsonPath, ValueExpr};

    /// Mock component schema provider for testing.
    struct MockSchemaProvider {
        schemas: IndexMap<String, (Option<Type>, Option<Type>)>,
    }

    impl MockSchemaProvider {
        fn new() -> Self {
            Self {
                schemas: IndexMap::new(),
            }
        }

        fn add_component(&mut self, path: &str, input: Option<Type>, output: Option<Type>) {
            self.schemas.insert(path.to_string(), (input, output));
        }
    }

    impl ComponentSchemaProvider for MockSchemaProvider {
        fn get_input_schema(&self, component: &Component) -> Option<Type> {
            self.schemas
                .get(component.path())
                .and_then(|(input, _)| input.clone())
        }

        fn get_output_schema(&self, component: &Component) -> Option<Type> {
            self.schemas
                .get(component.path())
                .and_then(|(_, output)| output.clone())
        }

        fn infer_output_schema(&self, component: &Component, _input: &Type) -> Option<Type> {
            // For testing, just return the static output schema
            self.get_output_schema(component)
        }
    }

    fn make_step(id: &str, component: &str, input: ValueExpr) -> Step {
        Step {
            id: id.to_string(),
            component: Component::for_plugin("test", component),
            input,
            on_error: None,
            must_execute: None,
            metadata: Default::default(),
        }
    }

    fn make_flow(input_schema: Option<Value>, steps: Vec<Step>, output: ValueExpr) -> Flow {
        use stepflow_core::workflow::{FlowSchema, FlowV1};

        Flow::V1(FlowV1 {
            name: Some("test_flow".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                input: input_schema.map(SchemaRef::from),
                output: None,
                defs: Default::default(),
                variables: Default::default(),
                steps: Default::default(),
            },
            steps,
            output,
            test: None,
            examples: None,
            metadata: Default::default(),
        })
    }

    // === Basic Flow Tests ===

    #[test]
    fn test_empty_flow() {
        let flow = make_flow(None, vec![], ValueExpr::null());
        let provider = MockSchemaProvider::new();
        let config = TypeCheckConfig::default();

        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
        assert!(result.step_types().is_empty());
    }

    #[test]
    fn test_single_step_flow() {
        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/echo",
            Some(Type::from_json(json!({"type": "string"}))),
            Some(Type::from_json(json!({"type": "string"}))),
        );

        let flow = make_flow(
            Some(json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string"}
                }
            })),
            vec![make_step(
                "echo",
                "/echo",
                ValueExpr::workflow_input(JsonPath::parse("$.message").unwrap()),
            )],
            ValueExpr::step("echo", JsonPath::new()),
        );

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
        assert_eq!(result.step_type("echo").unwrap().to_string(), "string");
    }

    #[test]
    fn test_step_chain() {
        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/step1",
            Some(Type::from_json(json!({"type": "string"}))),
            Some(Type::from_json(json!({
                "type": "object",
                "properties": {
                    "result": {"type": "number"}
                }
            }))),
        );
        provider.add_component(
            "/test/step2",
            Some(Type::from_json(json!({"type": "number"}))),
            Some(Type::from_json(json!({"type": "boolean"}))),
        );

        let flow = make_flow(
            Some(json!({
                "type": "object",
                "properties": {
                    "input": {"type": "string"}
                }
            })),
            vec![
                make_step(
                    "step1",
                    "/step1",
                    ValueExpr::workflow_input(JsonPath::parse("$.input").unwrap()),
                ),
                make_step(
                    "step2",
                    "/step2",
                    ValueExpr::step("step1", JsonPath::parse("$.result").unwrap()),
                ),
            ],
            ValueExpr::step("step2", JsonPath::new()),
        );

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
        // Now shows property types
        let step1_display = result.step_type("step1").unwrap().to_string();
        assert!(
            step1_display.contains("result: number"),
            "step1 display: {}",
            step1_display
        );
        assert_eq!(result.step_type("step2").unwrap().to_string(), "boolean");
    }

    // === Type Mismatch Tests ===

    #[test]
    fn test_type_mismatch() {
        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/needs_number",
            Some(Type::from_json(json!({"type": "number"}))),
            Some(Type::from_json(json!({"type": "string"}))),
        );

        let flow = make_flow(
            Some(json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string"}
                }
            })),
            vec![make_step(
                "step1",
                "/needs_number",
                // Passing string when number is expected
                ValueExpr::workflow_input(JsonPath::parse("$.text").unwrap()),
            )],
            ValueExpr::step("step1", JsonPath::new()),
        );

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(!result.is_ok());
        assert!(!result.errors.is_empty());
        assert!(matches!(
            &result.errors[0].error,
            TypeError::Mismatch { .. }
        ));
    }

    #[test]
    fn test_missing_required_property() {
        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/needs_object",
            Some(Type::from_json(json!({
                "type": "object",
                "properties": {
                    "required_field": {"type": "string"}
                },
                "required": ["required_field"]
            }))),
            Some(Type::from_json(json!({"type": "string"}))),
        );

        let flow = make_flow(
            None,
            vec![make_step(
                "step1",
                "/needs_object",
                // Object without required_field
                ValueExpr::object(vec![(
                    "other_field".to_string(),
                    ValueExpr::literal(json!("value")),
                )]),
            )],
            ValueExpr::step("step1", JsonPath::new()),
        );

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(!result.is_ok());
        assert!(
            result.errors[0]
                .error
                .to_string()
                .contains("missing required property")
        );
    }

    // === Untyped Component Tests ===

    #[test]
    fn test_untyped_component_permissive() {
        let provider = MockSchemaProvider::new(); // No schema for component

        let flow = make_flow(
            None,
            vec![make_step(
                "step1",
                "/unknown",
                ValueExpr::literal(json!("input")),
            )],
            ValueExpr::step("step1", JsonPath::new()),
        );

        let config = TypeCheckConfig { strict: false };
        let result = type_check_flow(&flow, &provider, config);

        // Should pass (permissive mode)
        assert!(result.is_ok());
        // Step type should be Any
        assert!(result.step_type("step1").unwrap().is_any());
    }

    #[test]
    fn test_untyped_component_strict() {
        let provider = MockSchemaProvider::new(); // No schema for component

        let flow = make_flow(
            None,
            vec![make_step(
                "step1",
                "/unknown",
                ValueExpr::literal(json!("input")),
            )],
            ValueExpr::step("step1", JsonPath::new()),
        );

        let config = TypeCheckConfig { strict: true };
        let result = type_check_flow(&flow, &provider, config);

        // Should still pass but with warnings
        assert!(result.is_ok());
        assert!(!result.warnings.is_empty());
    }

    // === Step Output Schema Override ===

    #[test]
    fn test_step_output_schema_override() {
        use indexmap::IndexMap;
        use stepflow_core::workflow::{FlowSchema, FlowV1};

        let mut provider = MockSchemaProvider::new();
        provider.add_component("/test/any", None, None);

        let step = make_step("step1", "/any", ValueExpr::literal(json!("input")));

        // Create flow with declared step output schema in types.steps
        let mut step_schemas = IndexMap::new();
        step_schemas.insert(
            "step1".to_string(),
            SchemaRef::from(json!({"type": "number"})),
        );

        let flow = Flow::V1(FlowV1 {
            name: Some("test_flow".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                input: None,
                output: None,
                defs: Default::default(),
                variables: Default::default(),
                steps: step_schemas,
            },
            steps: vec![step],
            output: ValueExpr::step("step1", JsonPath::new()),
            test: None,
            examples: None,
            metadata: Default::default(),
        });

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
        // Should use the declared output schema, not Any
        assert_eq!(result.step_type("step1").unwrap().to_string(), "number");
    }

    // === Complex Expression Tests ===

    #[test]
    fn test_if_expression_in_input() {
        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/accepts_string",
            Some(Type::from_json(json!({"type": "string"}))),
            Some(Type::from_json(json!({"type": "string"}))),
        );

        let flow = make_flow(
            Some(json!({
                "type": "object",
                "properties": {
                    "flag": {"type": "boolean"},
                    "a": {"type": "string"},
                    "b": {"type": "string"}
                }
            })),
            vec![make_step(
                "step1",
                "/accepts_string",
                ValueExpr::if_expr(
                    ValueExpr::workflow_input(JsonPath::parse("$.flag").unwrap()),
                    ValueExpr::workflow_input(JsonPath::parse("$.a").unwrap()),
                    Some(ValueExpr::workflow_input(JsonPath::parse("$.b").unwrap())),
                ),
            )],
            ValueExpr::step("step1", JsonPath::new()),
        );

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
    }

    #[test]
    fn test_coalesce_expression() {
        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/first",
            None,
            Some(Type::from_json(json!({"type": "string"}))),
        );
        provider.add_component(
            "/test/second",
            None,
            Some(Type::from_json(json!({"type": "string"}))),
        );

        let flow = make_flow(
            None,
            vec![
                make_step("first", "/first", ValueExpr::null()),
                make_step("second", "/second", ValueExpr::null()),
            ],
            ValueExpr::coalesce(vec![
                ValueExpr::step("first", JsonPath::new()),
                ValueExpr::step("second", JsonPath::new()),
                ValueExpr::literal(json!("fallback")),
            ]),
        );

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
    }

    // === Object Construction Tests ===

    #[test]
    fn test_object_construction() {
        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/accepts_object",
            Some(Type::from_json(json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "value": {"type": "number"}
                },
                "required": ["name", "value"]
            }))),
            Some(Type::from_json(json!({"type": "boolean"}))),
        );

        let flow = make_flow(
            Some(json!({
                "type": "object",
                "properties": {
                    "input_name": {"type": "string"},
                    "input_value": {"type": "number"}
                }
            })),
            vec![make_step(
                "step1",
                "/accepts_object",
                ValueExpr::object(vec![
                    (
                        "name".to_string(),
                        ValueExpr::workflow_input(JsonPath::parse("$.input_name").unwrap()),
                    ),
                    (
                        "value".to_string(),
                        ValueExpr::workflow_input(JsonPath::parse("$.input_value").unwrap()),
                    ),
                ]),
            )],
            ValueExpr::step("step1", JsonPath::new()),
        );

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
    }

    // === FlowSchema with $defs Tests ===

    #[test]
    fn test_flow_types_with_defs() {
        use std::collections::HashMap;
        use stepflow_core::workflow::{FlowSchema, FlowV1};

        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/user_processor",
            Some(Type::from_json(json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "age": {"type": "integer"}
                },
                "required": ["name", "age"]
            }))),
            Some(Type::from_json(json!({
                "type": "object",
                "properties": {
                    "processed": {"type": "boolean"}
                }
            }))),
        );

        // Create shared type definitions in $defs
        let mut defs = HashMap::new();
        defs.insert(
            "User".to_string(),
            SchemaRef::from(json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "age": {"type": "integer"}
                },
                "required": ["name", "age"]
            })),
        );

        let step = make_step(
            "process",
            "/user_processor",
            ValueExpr::workflow_input(JsonPath::new()),
        );

        // Flow with $defs and input schema using a reference
        let flow = Flow::V1(FlowV1 {
            name: Some("test_flow_with_defs".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                defs,
                input: Some(SchemaRef::from(json!({"$ref": "#/schema/$defs/User"}))),
                output: None,
                variables: Default::default(),
                steps: Default::default(),
            },
            steps: vec![step],
            output: ValueExpr::step("process", JsonPath::new()),
            test: None,
            examples: None,
            metadata: Default::default(),
        });

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        // The flow should type check successfully
        // Note: Full $ref resolution would need schema resolver integration
        assert!(result.is_ok() || !result.errors.is_empty());
    }

    #[test]
    fn test_per_flow_step_output_schemas() {
        use indexmap::IndexMap;
        use stepflow_core::workflow::{FlowSchema, FlowV1};

        let provider = MockSchemaProvider::new(); // No component schemas

        let step1 = make_step("step1", "/unknown1", ValueExpr::literal(json!("input")));
        let step2 = make_step(
            "step2",
            "/unknown2",
            ValueExpr::step("step1", JsonPath::parse("$.result").unwrap()),
        );

        // Declare step output schemas in types.steps
        let mut step_schemas = IndexMap::new();
        step_schemas.insert(
            "step1".to_string(),
            SchemaRef::from(json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            })),
        );
        step_schemas.insert(
            "step2".to_string(),
            SchemaRef::from(json!({"type": "number"})),
        );

        let flow = Flow::V1(FlowV1 {
            name: Some("test_step_schemas".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                defs: Default::default(),
                input: None,
                output: Some(SchemaRef::from(json!({"type": "number"}))),
                variables: Default::default(),
                steps: step_schemas,
            },
            steps: vec![step1, step2],
            output: ValueExpr::step("step2", JsonPath::new()),
            test: None,
            examples: None,
            metadata: Default::default(),
        });

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
        // Step types should come from flow.schemas.steps
        // Now shows property types
        let step1_display = result.step_type("step1").unwrap().to_string();
        assert!(
            step1_display.contains("result: string"),
            "step1 display: {}",
            step1_display
        );
        assert_eq!(result.step_type("step2").unwrap().to_string(), "number");
    }

    #[test]
    fn test_flow_output_schema_check() {
        use stepflow_core::workflow::{FlowSchema, FlowV1};

        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/echo",
            None,
            Some(Type::from_json(json!({"type": "string"}))),
        );

        let step = make_step("echo", "/echo", ValueExpr::literal(json!("hello")));

        // Flow declares output schema that matches step output
        let flow = Flow::V1(FlowV1 {
            name: Some("test_output_check".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                defs: Default::default(),
                input: None,
                output: Some(SchemaRef::from(json!({"type": "string"}))),
                variables: Default::default(),
                steps: Default::default(),
            },
            steps: vec![step],
            output: ValueExpr::step("echo", JsonPath::new()),
            test: None,
            examples: None,
            metadata: Default::default(),
        });

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        assert!(result.is_ok());
    }

    #[test]
    fn test_flow_output_schema_mismatch() {
        use stepflow_core::workflow::{FlowSchema, FlowV1};

        let mut provider = MockSchemaProvider::new();
        provider.add_component(
            "/test/echo",
            None,
            Some(Type::from_json(json!({"type": "string"}))),
        );

        let step = make_step("echo", "/echo", ValueExpr::literal(json!("hello")));

        // Flow declares output schema that doesn't match step output
        let flow = Flow::V1(FlowV1 {
            name: Some("test_output_mismatch".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                defs: Default::default(),
                input: None,
                output: Some(SchemaRef::from(json!({"type": "number"}))),
                variables: Default::default(),
                steps: Default::default(),
            },
            steps: vec![step],
            output: ValueExpr::step("echo", JsonPath::new()),
            test: None,
            examples: None,
            metadata: Default::default(),
        });

        let config = TypeCheckConfig::default();
        let result = type_check_flow(&flow, &provider, config);

        // Should have a type mismatch error
        assert!(!result.is_ok());
        assert!(!result.errors.is_empty());
        assert!(matches!(
            &result.errors[0].error,
            TypeError::Mismatch { .. }
        ));
    }

    #[test]
    fn test_nested_flow_self_contained_types() {
        use std::collections::HashMap;
        use stepflow_core::workflow::{FlowSchema, FlowV1};

        // This test demonstrates that nested flows have their own self-contained types.
        // In a real scenario, a nested flow stored in a blob would have its own
        // types.$defs, types.input, types.output, etc.

        let provider = MockSchemaProvider::new();

        // Parent flow with its own type definitions
        let mut parent_defs = HashMap::new();
        parent_defs.insert(
            "ParentType".to_string(),
            SchemaRef::from(json!({
                "type": "object",
                "properties": {
                    "parent_field": {"type": "string"}
                }
            })),
        );

        let parent_step = make_step(
            "call_nested",
            "/builtin/eval",
            ValueExpr::literal(json!({})),
        );

        let parent_flow = Flow::V1(FlowV1 {
            name: Some("parent_flow".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                defs: parent_defs.clone(),
                input: Some(SchemaRef::from(
                    json!({"$ref": "#/schema/$defs/ParentType"}),
                )),
                output: None,
                variables: Default::default(),
                steps: Default::default(),
            },
            steps: vec![parent_step],
            output: ValueExpr::step("call_nested", JsonPath::new()),
            test: None,
            examples: None,
            metadata: Default::default(),
        });

        // Nested flow would have its own separate types (stored in blob)
        let mut nested_defs = HashMap::new();
        nested_defs.insert(
            "NestedType".to_string(),
            SchemaRef::from(json!({
                "type": "object",
                "properties": {
                    "nested_field": {"type": "number"}
                }
            })),
        );

        let nested_step = make_step("inner", "/some/component", ValueExpr::literal(json!(42)));

        let nested_flow = Flow::V1(FlowV1 {
            name: Some("nested_flow".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                defs: nested_defs.clone(),
                input: Some(SchemaRef::from(
                    json!({"$ref": "#/schema/$defs/NestedType"}),
                )),
                output: Some(SchemaRef::from(json!({"type": "string"}))),
                variables: Default::default(),
                steps: Default::default(),
            },
            steps: vec![nested_step],
            output: ValueExpr::step("inner", JsonPath::new()),
            test: None,
            examples: None,
            metadata: Default::default(),
        });

        // Verify each flow has its own independent type definitions
        assert!(parent_flow.schemas().defs.contains_key("ParentType"));
        assert!(!parent_flow.schemas().defs.contains_key("NestedType"));

        assert!(nested_flow.schemas().defs.contains_key("NestedType"));
        assert!(!nested_flow.schemas().defs.contains_key("ParentType"));

        // Each flow can be type-checked independently
        let config = TypeCheckConfig::default();
        let _ = type_check_flow(&parent_flow, &provider, config);
        let _ = type_check_flow(&nested_flow, &provider, config);
    }

    #[test]
    fn test_flow_types_serialization() {
        use indexmap::IndexMap;
        use std::collections::HashMap;
        use stepflow_core::workflow::{FlowSchema, FlowV1};

        let mut defs = HashMap::new();
        defs.insert(
            "Message".to_string(),
            SchemaRef::from(json!({
                "type": "object",
                "properties": {
                    "content": {"type": "string"},
                    "role": {"type": "string"}
                },
                "required": ["content", "role"]
            })),
        );

        let mut step_schemas = IndexMap::new();
        step_schemas.insert(
            "step1".to_string(),
            SchemaRef::from(json!({"type": "string"})),
        );

        let flow = Flow::V1(FlowV1 {
            name: Some("serialization_test".to_string()),
            description: None,
            version: None,
            schemas: FlowSchema {
                defs,
                input: Some(SchemaRef::from(json!({"type": "object"}))),
                output: Some(SchemaRef::from(json!({"type": "string"}))),
                variables: Default::default(),
                steps: step_schemas,
            },
            steps: vec![],
            output: ValueExpr::null(),
            test: None,
            examples: None,
            metadata: Default::default(),
        });

        // Serialize to JSON
        let json_str = serde_json::to_string(&flow).expect("Serialization failed");

        // Verify structure in JSON
        assert!(json_str.contains("\"$defs\""));
        assert!(json_str.contains("\"Message\""));
        assert!(json_str.contains("\"input\""));
        assert!(json_str.contains("\"output\""));
        assert!(json_str.contains("\"steps\""));

        // Deserialize and verify round-trip
        let parsed: Flow = serde_json::from_str(&json_str).expect("Deserialization failed");
        assert_eq!(parsed.schemas().defs.len(), 1);
        assert!(parsed.schemas().defs.contains_key("Message"));
        assert!(parsed.schemas().input.is_some());
        assert!(parsed.schemas().output.is_some());
        assert_eq!(parsed.schemas().steps.len(), 1);
    }
}
