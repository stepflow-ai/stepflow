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
/// - Rewrites `#/$defs/X` references to `#/components/schemas/X` (schemars generates
///   `$defs`-style refs which are valid JSON Schema but not valid in an OpenAPI document
///   where schemas live under `components.schemas`)
/// - Adds missing path parameter definitions (aide doesn't auto-generate path params
///   from `Path<T>` extractors when T is a simple type rather than a named struct)
/// - Removes unreferenced component schemas (aide registers query parameter structs
///   in `components.schemas` but inlines the individual fields in `parameters`)
pub fn finalize_openapi(api: &OpenApi) -> Value {
    let mut json = serde_json::to_value(api).expect("Failed to serialize OpenAPI");
    finalize_openapi_discriminators(&mut json);
    // Note: rewrite_defs_to_components is called inside finalize_openapi_discriminators
    add_missing_path_parameters(&mut json);
    remove_unreferenced_schemas(&mut json);
    json
}

/// Run the discriminator finalization pipeline on OpenAPI `components.schemas`.
///
/// The pipeline expects schemas under root `$defs` with `#/$defs/X` refs.
/// Aide generates `#/components/schemas/X` refs, so we:
/// 1. Rewrite refs to `#/$defs/X`
/// 2. Move `components.schemas` to root `$defs`
/// 3. Run the finalization pipeline (rename, extract, build mappings, add defaults)
/// 4. Move `$defs` back to `components.schemas`
/// 5. Rewrite refs back to `#/components/schemas/X`
fn finalize_openapi_discriminators(root: &mut Value) {
    // Step 1: Rewrite #/components/schemas/X → #/$defs/X
    rewrite_components_to_defs(root);

    // Step 2: Move components.schemas → root $defs
    let root_obj = root.as_object_mut().unwrap();
    let schemas = root_obj
        .get_mut("components")
        .and_then(|c| c.as_object_mut())
        .and_then(|c| c.remove("schemas"));

    let Some(schemas) = schemas else { return };
    root_obj.insert("$defs".to_string(), schemas);

    // Step 3: Run discriminator finalization pipeline
    stepflow_core::json_schema::finalize_discriminators(root);

    // Step 4: Move $defs → components.schemas
    let root_obj = root.as_object_mut().unwrap();
    if let Some(new_defs) = root_obj.remove("$defs") {
        if let Some(comp) = root_obj
            .get_mut("components")
            .and_then(|c| c.as_object_mut())
        {
            comp.insert("schemas".to_string(), new_defs);
        }
    }

    // Step 5: Rewrite #/$defs/X → #/components/schemas/X
    rewrite_defs_to_components(root);
}

/// Recursively rewrite `#/components/schemas/X` references to `#/$defs/X`.
fn rewrite_components_to_defs(value: &mut Value) {
    match value {
        Value::String(s) if s.starts_with("#/components/schemas/") => {
            *s = s.replacen("#/components/schemas/", "#/$defs/", 1);
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                rewrite_components_to_defs(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                rewrite_components_to_defs(v);
            }
        }
        _ => {}
    }
}

/// Recursively rewrite `#/$defs/X` references to `#/components/schemas/X`.
fn rewrite_defs_to_components(value: &mut Value) {
    match value {
        Value::String(s) if s.starts_with("#/$defs/") => {
            *s = s.replacen("#/$defs/", "#/components/schemas/", 1);
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                rewrite_defs_to_components(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                rewrite_defs_to_components(v);
            }
        }
        _ => {}
    }
}

/// Add missing path parameter definitions by parsing `{param}` from path templates.
fn add_missing_path_parameters(root: &mut Value) {
    let paths = match root
        .get("paths")
        .and_then(|p| p.as_object())
        .map(|p| p.keys().cloned().collect::<Vec<_>>())
    {
        Some(keys) => keys,
        None => return,
    };

    for path_template in paths {
        let params = extract_path_params(&path_template);
        if params.is_empty() {
            continue;
        }

        let path_item = match root
            .get_mut("paths")
            .and_then(|p| p.get_mut(&path_template))
            .and_then(|p| p.as_object_mut())
        {
            Some(o) => o,
            None => continue,
        };

        for method in ["get", "post", "put", "delete", "patch"] {
            let op = match path_item.get_mut(method).and_then(|o| o.as_object_mut()) {
                Some(o) => o,
                None => continue,
            };

            let parameters = op
                .entry("parameters")
                .or_insert_with(|| Value::Array(vec![]))
                .as_array_mut()
                .unwrap();

            for param_name in &params {
                let already_defined = parameters.iter().any(|p| {
                    p.get("name").and_then(|n| n.as_str()) == Some(param_name)
                        && p.get("in").and_then(|i| i.as_str()) == Some("path")
                });

                if !already_defined {
                    parameters.push(serde_json::json!({
                        "name": param_name,
                        "in": "path",
                        "required": true,
                        "schema": { "type": "string" }
                    }));
                }
            }
        }
    }
}

/// Extract `{param}` names from a path template string.
fn extract_path_params(template: &str) -> Vec<String> {
    let mut params = vec![];
    let mut remaining = template;
    while let Some(start) = remaining.find('{') {
        if let Some(end) = remaining[start..].find('}') {
            let param = &remaining[start + 1..start + end];
            params.push(param.to_string());
            remaining = &remaining[start + end + 1..];
        } else {
            break;
        }
    }
    params
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
