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

use serde::{Deserialize, Serialize};
use stepflow_core::component::ComponentInfo;
use stepflow_core::schema::SchemaRef;
use stepflow_core::workflow::{Component, ValueRef};
use utoipa::ToSchema;

use crate::protocol::Method;

use super::{ObservabilityContext, ProtocolMethod};

/// Sent from Stepflow to the component server to execute a specific component with the provided input.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComponentExecuteParams {
    /// The component to execute.
    pub component: Component,
    /// The input to the component.
    pub input: ValueRef,
    /// The attempt number for this execution (1-based, for retry logic).
    pub attempt: u32,
    /// Observability context for tracing and logging.
    pub observability: ObservabilityContext,
}

/// Sent from the component server back to Stepflow with the result of the component execution.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComponentExecuteResult {
    /// The result of the component execution.
    pub output: ValueRef,
}

impl ProtocolMethod for ComponentExecuteParams {
    const METHOD_NAME: Method = Method::ComponentsExecute;
    type Response = ComponentExecuteResult;
}

/// Sent from Stepflow to the component server to request information about a specific component.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComponentInfoParams {
    /// The component to get information about.
    pub component: Component,
}

/// Sent from the component server back to Stepflow with information about the requested component.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComponentInfoResult {
    /// Information about the component.
    pub info: ComponentInfo,
}

impl ProtocolMethod for ComponentInfoParams {
    const METHOD_NAME: Method = Method::ComponentsInfo;
    type Response = ComponentInfoResult;
}

/// Sent from Stepflow to the component server to request a list of all available components.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComponentListParams {}

/// Sent from the component server back to Stepflow with a list of all available components.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ListComponentsResult {
    /// A list of all available components.
    pub components: Vec<ComponentInfo>,
}

impl ProtocolMethod for ComponentListParams {
    const METHOD_NAME: Method = Method::ComponentsList;
    type Response = ListComponentsResult;
}

/// Sent from Stepflow to the component server to infer the output schema for a component
/// given an input schema. This enables static type checking of workflows.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComponentInferSchemaParams {
    /// The component to infer the schema for.
    pub component: Component,
    /// The schema of the input that will be provided to the component.
    pub input_schema: SchemaRef,
}

/// Sent from the component server back to Stepflow with the inferred output schema.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ComponentInferSchemaResult {
    /// The inferred output schema, or None if the component cannot determine it.
    pub output_schema: Option<SchemaRef>,
}

impl ProtocolMethod for ComponentInferSchemaParams {
    const METHOD_NAME: Method = Method::ComponentsInferSchema;
    type Response = ComponentInferSchemaResult;
}
