use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::schema::SchemaRef;
use stepflow_execution::StepFlowExecutor;
use stepflow_plugin::Plugin as _;
use utoipa::{IntoParams, OpenApi, ToSchema};

/// Component information for API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ComponentInfoResponse {
    /// The component name/URL
    pub name: String,
    /// The plugin name that provides this component
    pub plugin_name: String,
    /// Component description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Input schema (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<SchemaRef>,
    /// Output schema (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<SchemaRef>,
}

/// Response for listing components
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListComponentsResponse {
    /// List of available components
    pub components: Vec<ComponentInfoResponse>,
}

/// Query parameters for listing components
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct ListComponentsQuery {
    /// Whether to include schemas in the response (default: true)
    #[serde(default = "default_include_schemas")]
    pub include_schemas: bool,
}

fn default_include_schemas() -> bool {
    true
}

/// Components API
#[derive(OpenApi)]
#[openapi(
    paths(list_components),
    components(schemas(ComponentInfoResponse, ListComponentsResponse, ListComponentsQuery))
)]
pub struct ComponentsApi;

/// List all available components from plugins
#[utoipa::path(
    get,
    path = "/components",
    params(ListComponentsQuery),
    responses(
        (status = 200, description = "Components listed successfully", body = ListComponentsResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_components(
    State(executor): State<Arc<StepFlowExecutor>>,
    Query(query): Query<ListComponentsQuery>,
) -> Result<Json<ListComponentsResponse>, StatusCode> {
    let include_schemas = query.include_schemas;

    // Get all registered plugins and query their components
    let mut all_components = Vec::new();

    // Get the list of plugins from the executor
    for (plugin_name, plugin) in executor.list_plugins().await {
        // List components available from this plugin
        let components = plugin
            .list_components()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // For each component, get detailed information
        for component in components {
            let info = plugin
                .component_info(&component)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let component_response = ComponentInfoResponse {
                name: component.url().to_string(),
                plugin_name: plugin_name.clone(),
                description: info.description,
                input_schema: if include_schemas {
                    Some(info.input_schema)
                } else {
                    None
                },
                output_schema: if include_schemas {
                    Some(info.output_schema)
                } else {
                    None
                },
            };

            all_components.push(component_response);
        }
    }

    // Sort components by their name for consistent output
    all_components.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(ListComponentsResponse {
        components: all_components,
    }))
}
