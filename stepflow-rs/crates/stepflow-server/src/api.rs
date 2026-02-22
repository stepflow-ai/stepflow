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

use std::sync::Arc;

use aide::axum::ApiRouter;
use aide::axum::routing::{get_with, post_with};
use aide::openapi::OpenApi;
use serde_json::Value;
use stepflow_plugin::StepflowEnvironment;

mod blobs;
mod components;
mod flows;
mod health;
mod runs;

pub use flows::{StoreFlowRequest, StoreFlowResponse};
pub use runs::{CreateRunRequest, CreateRunResponse};

pub fn create_api_router() -> (axum::Router<Arc<StepflowEnvironment>>, OpenApi) {
    let mut api = OpenApi {
        info: aide::openapi::Info {
            title: "Stepflow API".into(),
            description: Some("API for Stepflow flows and runs".into()),
            version: env!("CARGO_PKG_VERSION").into(),
            license: Some(aide::openapi::License {
                name: "Apache-2.0".into(),
                url: Some("https://www.apache.org/licenses/LICENSE-2.0".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        servers: vec![aide::openapi::Server {
            url: "http://localhost:7840/api/v1".into(),
            description: Some("Default development server (port 7840)".into()),
            ..Default::default()
        }],
        tags: vec![
            aide::openapi::Tag {
                name: "Blob".into(),
                description: Some("Blob API endpoints".into()),
                ..Default::default()
            },
            aide::openapi::Tag {
                name: "Component".into(),
                description: Some("Component API endpoints".into()),
                ..Default::default()
            },
            aide::openapi::Tag {
                name: "Flow".into(),
                description: Some("Flow API endpoints".into()),
                ..Default::default()
            },
            aide::openapi::Tag {
                name: "Run".into(),
                description: Some("Run API endpoints".into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let router = ApiRouter::new()
        .api_route(
            "/health",
            get_with(health::health_check, health::health_check_docs),
        )
        .api_route(
            "/blobs",
            post_with(blobs::store_blob, blobs::store_blob_docs),
        )
        .api_route(
            "/blobs/{blob_id}",
            get_with(blobs::get_blob, blobs::get_blob_docs),
        )
        .api_route(
            "/components",
            get_with(
                components::list_components,
                components::list_components_docs,
            ),
        )
        .api_route(
            "/runs",
            get_with(runs::list_runs, runs::list_runs_docs)
                .post_with(runs::create_run, runs::create_run_docs),
        )
        .api_route(
            "/runs/{run_id}",
            get_with(runs::get_run, runs::get_run_docs)
                .delete_with(runs::delete_run, runs::delete_run_docs),
        )
        .api_route(
            "/runs/{run_id}/items",
            get_with(runs::get_run_items, runs::get_run_items_docs),
        )
        .api_route(
            "/runs/{run_id}/flow",
            get_with(runs::get_run_flow, runs::get_run_flow_docs),
        )
        .api_route(
            "/runs/{run_id}/steps",
            get_with(runs::get_run_steps, runs::get_run_steps_docs),
        )
        .api_route(
            "/runs/{run_id}/cancel",
            post_with(runs::cancel_run, runs::cancel_run_docs),
        )
        .api_route(
            "/flows",
            post_with(flows::store_flow, flows::store_flow_docs),
        )
        .api_route(
            "/flows/{flow_id}",
            get_with(flows::get_flow, flows::get_flow_docs)
                .delete_with(flows::delete_flow, flows::delete_flow_docs),
        )
        .finish_api(&mut api);

    (router, api)
}

/// Serialize an OpenAPI spec to JSON and apply post-processing fixes.
///
/// Fixes issues that aide doesn't handle:
/// - Renames schemas, builds discriminator mappings, and adds defaults (same
///   pipeline as JSON Schema generation — see [`stepflow_core::json_schema`])
/// - Removes unreferenced component schemas (aide registers query parameter structs
///   in `components.schemas` but inlines the individual fields in `parameters`)
pub fn finalize_openapi(api: &OpenApi) -> Value {
    let mut json = serde_json::to_value(api).expect("Failed to serialize OpenAPI");
    stepflow_core::json_schema::finalize_discriminators_with_prefix(
        &mut json,
        "#/components/schemas/",
    );
    remove_unreferenced_schemas(&mut json);
    json
}

/// Remove component schemas that are not referenced anywhere in the document.
///
/// Aide registers query parameter structs as `components.schemas` entries but
/// inlines the individual fields as operation `parameters`, leaving the schema
/// unreferenced.  Removing them avoids `no-unused-components` linter warnings.
fn remove_unreferenced_schemas(root: &mut Value) {
    // Collect all $ref targets
    let mut refs = std::collections::HashSet::new();
    collect_refs(root, &mut refs);

    // Remove unreferenced schemas from components.schemas
    if let Some(schemas) = root
        .pointer_mut("/components/schemas")
        .and_then(|s| s.as_object_mut())
    {
        let schema_names: Vec<String> = schemas.keys().cloned().collect();
        for name in schema_names {
            let ref_string = format!("#/components/schemas/{name}");
            if !refs.contains(&ref_string) {
                schemas.remove(&name);
            }
        }
    }
}

/// Recursively collect all `$ref` string values in the JSON tree.
fn collect_refs(value: &Value, refs: &mut std::collections::HashSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(ref_str)) = map.get("$ref") {
                refs.insert(ref_str.clone());
            }
            for v in map.values() {
                collect_refs(v, refs);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_refs(v, refs);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    /// This test generates the OpenAPI schema to schemas/openapi.json.
    /// Run with: STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-server --lib test_openapi_schema_generation
    #[test]
    fn test_openapi_schema_generation() {
        let (_, openapi) = create_api_router();
        let api_json = finalize_openapi(&openapi);
        let openapi_json =
            serde_json::to_string_pretty(&api_json).expect("Failed to serialize OpenAPI schema");

        // Get path to schemas/openapi.json
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let openapi_schema_path = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("schemas")
            .join("openapi.json");

        if env::var("STEPFLOW_OVERWRITE_SCHEMA").is_ok() {
            fs::write(&openapi_schema_path, &openapi_json).expect("Failed to write OpenAPI schema");
            println!(
                "Updated OpenAPI schema at {}",
                openapi_schema_path.display()
            );
        } else if openapi_schema_path.exists() {
            let existing = fs::read_to_string(&openapi_schema_path)
                .expect("Failed to read existing OpenAPI schema");

            // Parse both as JSON to compare (ignoring formatting differences)
            let existing_json: serde_json::Value =
                serde_json::from_str(&existing).expect("Failed to parse existing schema");

            if existing_json != api_json {
                panic!(
                    "OpenAPI schema mismatch. Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-server --lib test_openapi_schema_generation' to update."
                );
            }
        } else {
            panic!(
                "OpenAPI schema file not found at {}. Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-server --lib test_openapi_schema_generation' to create it.",
                openapi_schema_path.display()
            );
        }
    }
}
