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

//! Smoke test validating aide works with our axum version and schemars types.

use aide::axum::{ApiRouter, routing::post_with};
use aide::openapi::OpenApi;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct TestInput {
    name: String,
    count: Option<i32>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TestOutput {
    message: String,
}

async fn test_handler(Json(_input): Json<TestInput>) -> Json<TestOutput> {
    Json(TestOutput {
        message: "hello".to_string(),
    })
}

#[test]
fn aide_generates_openapi_from_schemars_types() {
    let mut api = OpenApi::default();
    let _app: axum::Router = ApiRouter::new()
        .api_route(
            "/test",
            post_with(test_handler, |op| op.summary("Test endpoint")),
        )
        .finish_api(&mut api);

    let spec = serde_json::to_value(&api).unwrap();

    // Should have paths
    let paths = spec.get("paths").expect("should have paths");
    assert!(
        paths.get("/test").is_some(),
        "should have /test path in: {paths}"
    );

    // Should have the POST operation
    let test_path = paths.get("/test").unwrap();
    assert!(
        test_path.get("post").is_some(),
        "should have POST operation"
    );

    // Should have components/schemas with our types
    let components = spec.get("components").expect("should have components");
    let schemas = components.get("schemas").expect("should have schemas");
    assert!(
        schemas.get("TestInput").is_some(),
        "should have TestInput schema in: {schemas}"
    );
    assert!(
        schemas.get("TestOutput").is_some(),
        "should have TestOutput schema in: {schemas}"
    );
}
