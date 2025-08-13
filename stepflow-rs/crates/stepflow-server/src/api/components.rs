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

use axum::{
    extract::{Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::component::ComponentInfo;
use stepflow_execution::StepflowExecutor;
use stepflow_plugin::Plugin as _;
use utoipa::{IntoParams, ToSchema};

use crate::error::ErrorResponse;

/// Response for listing components
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListComponentsResponse {
    /// List of available components
    pub components: Vec<ComponentInfo>,
}

/// Query parameters for listing components
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListComponentsQuery {
    /// Whether to include schemas in the response (default: true)
    #[serde(default = "default_include_schemas")]
    pub include_schemas: bool,
}

fn default_include_schemas() -> bool {
    true
}

/// List all available components from plugins
#[utoipa::path(
    get,
    path = "/components",
    params(ListComponentsQuery),
    responses(
        (status = 200, description = "Components listed successfully", body = ListComponentsResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::COMPONENT_TAG,
)]
pub async fn list_components(
    State(executor): State<Arc<StepflowExecutor>>,
    Query(query): Query<ListComponentsQuery>,
) -> Result<Json<ListComponentsResponse>, ErrorResponse> {
    let include_schemas = query.include_schemas;

    // Get all registered plugins and query their components
    let mut all_components = Vec::new();

    // Get the list of plugins from the executor
    for plugin in executor.list_plugins().await {
        // List components available from this plugin
        let mut components = plugin.list_components().await?;
        if !include_schemas {
            for component in components.iter_mut() {
                component.input_schema = None;
                component.output_schema = None;
            }
        }
        all_components.extend(components);
    }

    // Sort components by their name for consistent output
    all_components.sort_by(|a, b| a.component.cmp(&b.component));

    Ok(Json(ListComponentsResponse {
        components: all_components,
    }))
}
