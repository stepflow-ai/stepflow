use poem_openapi::{OpenApi, payload::Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::schema::SchemaRef;
use stepflow_execution::StepFlowExecutor;
use stepflow_plugin::Plugin as _;

use super::api_type::ApiType;
use super::error::{IntoPoemError as _, ServerError, ServerResult};
use error_stack::ResultExt as _;

/// Component information for API responses
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

/// API wrapper for ComponentInfoResponse
pub type ApiComponentInfoResponse = ApiType<ComponentInfoResponse>;

/// Response for listing components
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListComponentsResponse {
    /// List of available components
    pub components: Vec<ComponentInfoResponse>,
}

/// API wrapper for ListComponentsResponse
pub type ApiListComponentsResponse = ApiType<ListComponentsResponse>;

pub struct ComponentsApi {
    executor: Arc<StepFlowExecutor>,
}

impl ComponentsApi {
    pub fn new(executor: Arc<StepFlowExecutor>) -> Self {
        Self { executor }
    }
}

#[OpenApi]
impl ComponentsApi {
    /// List all available components from plugins
    #[oai(path = "/components", method = "get")]
    pub async fn list_components(
        &self,
        include_schemas: poem::web::Query<Option<bool>>,
    ) -> ServerResult<Json<ApiListComponentsResponse>> {
        let include_schemas = include_schemas.0.unwrap_or(true);

        // Get all registered plugins and query their components
        let mut all_components = Vec::new();

        // Get the list of plugins from the executor
        for (plugin_name, plugin) in self.executor.list_plugins().await {
            // List components available from this plugin
            let components = plugin
                .list_components()
                .await
                .change_context(ServerError::ComponentListingFailed {
                    plugin_name: plugin_name.clone(),
                })
                .into_poem()?;

            // For each component, get detailed information
            for component in components {
                let info = plugin
                    .component_info(&component)
                    .await
                    .change_context(ServerError::ComponentInfoFailed {
                        component: component.url().to_string(),
                        plugin_name: plugin_name.clone(),
                    })
                    .into_poem()?;

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

        Ok(Json(ApiType(ListComponentsResponse {
            components: all_components,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poem::web::Query;
    use std::sync::Arc;
    use stepflow_execution::StepFlowExecutor;
    use stepflow_state::InMemoryStateStore;

    async fn create_test_executor() -> Arc<StepFlowExecutor> {
        let state_store = Arc::new(InMemoryStateStore::new());
        StepFlowExecutor::new(state_store)
    }

    #[tokio::test]
    async fn test_list_components_with_schemas() {
        let executor = create_test_executor().await;
        let api = ComponentsApi::new(executor);

        let result = api.list_components(Query(Some(true))).await;

        match result {
            Ok(json_payload) => {
                let response = json_payload.0;
                // With default executor (no plugins loaded), should return empty list
                // This is expected behavior since no plugins are registered
                assert!(response.components.is_empty());
            }
            Err(_) => panic!("Expected Ok result from list_components"),
        }
    }

    #[tokio::test]
    async fn test_list_components_without_schemas() {
        let executor = create_test_executor().await;
        let api = ComponentsApi::new(executor);

        let result = api.list_components(Query(Some(false))).await;

        match result {
            Ok(json_payload) => {
                let response = json_payload.0;
                // With default executor (no plugins loaded), should return empty list
                assert!(response.components.is_empty());
            }
            Err(_) => panic!("Expected Ok result from list_components"),
        }
    }

    #[tokio::test]
    async fn test_list_components_default_includes_schemas() {
        let executor = create_test_executor().await;
        let api = ComponentsApi::new(executor);

        // Default behavior should include schemas
        let result = api.list_components(Query(None)).await;

        match result {
            Ok(json_payload) => {
                let response = json_payload.0;
                // With default executor (no plugins loaded), should return empty list
                assert!(response.components.is_empty());
            }
            Err(_) => panic!("Expected Ok result from list_components"),
        }
    }

    #[test]
    fn test_component_info_response_structure() {
        use stepflow_core::schema::SchemaRef;

        let component = ComponentInfoResponse {
            name: "test_component".to_string(),
            plugin_name: "test_plugin".to_string(),
            description: Some("Test description".to_string()),
            input_schema: Some(SchemaRef::parse_json(r#"{"type": "boolean"}"#).unwrap()),
            output_schema: Some(SchemaRef::parse_json(r#"{"type": "string"}"#).unwrap()),
        };

        assert_eq!(component.name, "test_component");
        assert_eq!(component.plugin_name, "test_plugin");
        assert_eq!(component.description, Some("Test description".to_string()));
        assert!(component.input_schema.is_some());
        assert!(component.output_schema.is_some());
    }

    #[test]
    fn test_list_components_response_serialization() {
        let response = ListComponentsResponse {
            components: vec![
                ComponentInfoResponse {
                    name: "comp1".to_string(),
                    plugin_name: "plugin1".to_string(),
                    description: None,
                    input_schema: None,
                    output_schema: None,
                },
                ComponentInfoResponse {
                    name: "comp2".to_string(),
                    plugin_name: "plugin2".to_string(),
                    description: Some("Description".to_string()),
                    input_schema: None,
                    output_schema: None,
                },
            ],
        };

        // Should be able to serialize/deserialize
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListComponentsResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.components.len(), 2);
        assert_eq!(deserialized.components[0].name, "comp1");
        assert_eq!(deserialized.components[1].name, "comp2");
        assert_eq!(
            deserialized.components[1].description,
            Some("Description".to_string())
        );
    }
}
