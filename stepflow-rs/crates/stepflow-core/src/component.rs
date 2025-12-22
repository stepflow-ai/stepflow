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

//! Component information and metadata types.

use crate::schema::SchemaRef;
use crate::workflow::Component;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ComponentInfo {
    /// The component ID.
    pub component: Component,

    /// Optional description of the component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The input schema for the component.
    ///
    /// Can be any valid JSON schema (object, primitive, array, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<SchemaRef>,

    /// The output schema for the component.
    ///
    /// Can be any valid JSON schema (object, primitive, array, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<SchemaRef>,
}
