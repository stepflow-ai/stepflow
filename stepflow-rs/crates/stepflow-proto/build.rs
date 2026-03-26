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

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")?;
    let descriptor_path = format!("{out_dir}/stepflow_descriptor.bin");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_root = manifest_dir.join("../../proto");
    // Use dunce::canonicalize to avoid \\?\ UNC prefix on Windows,
    // which protoc cannot handle as an include path.
    let proto_root = dunce::canonicalize(&proto_root).expect("proto directory should exist");
    let proto_root_str = proto_root.to_str().unwrap();

    let proto_files: Vec<String> = [
        "stepflow/v1/common.proto",
        "stepflow/v1/health.proto",
        "stepflow/v1/components.proto",
        "stepflow/v1/tasks.proto",
        "stepflow/v1/orchestrator.proto",
        "stepflow/v1/blobs.proto",
        "stepflow/v1/flows.proto",
        "stepflow/v1/runs.proto",
        "stepflow/v1/vsock.proto",
    ]
    .iter()
    .map(|p| proto_root.join(p).to_str().unwrap().to_string())
    .collect();

    let proto_file_refs: Vec<&str> = proto_files.iter().map(String::as_str).collect();
    let include_dirs: &[&str] = &[proto_root_str];

    // Phase 1: Compile protos to a file descriptor set
    let descriptor_bytes = tonic_rest_build::dump_file_descriptor_set(
        &proto_file_refs,
        include_dirs,
        &descriptor_path,
    );

    // Phase 2: Configure prost with serde attributes
    let mut config = prost_build::Config::new();
    config.file_descriptor_set_path(&descriptor_path);

    // Map google.protobuf WKT types to prost-wkt-types (serde-enabled).
    config.extern_path(".google.protobuf.Value", "::prost_wkt_types::Value");
    config.extern_path(".google.protobuf.Struct", "::prost_wkt_types::Struct");
    config.extern_path(".google.protobuf.ListValue", "::prost_wkt_types::ListValue");
    config.extern_path(".google.protobuf.Timestamp", "::prost_wkt_types::Timestamp");
    config.extern_path(".google.protobuf.Duration", "::prost_wkt_types::Duration");
    config.extern_path(".google.protobuf.Any", "::prost_wkt_types::Any");
    config.extern_path(".google.protobuf.FieldMask", "::prost_wkt_types::FieldMask");

    // Box large enum variants.
    config.boxed(".stepflow.v1.TaskAssignment.task.execute");

    tonic_rest_build::ProstSerdeConfig::new(&descriptor_bytes, &proto_file_refs).apply(&mut config);

    // Add schemars::JsonSchema derive and SCREAMING_SNAKE_CASE serde for TaskErrorCode.
    config.type_attribute(
        ".stepflow.v1.TaskErrorCode",
        "#[derive(schemars::JsonSchema)] #[serde(rename_all = \"SCREAMING_SNAKE_CASE\")]",
    );

    // Request types deserialized from query strings or JSON bodies need
    // #[serde(default)] so that path-extracted fields (e.g. run_id),
    // repeated fields (e.g. event_types), and scalar fields with proto3
    // defaults can be absent. We use message_attribute (not type_attribute)
    // so the attribute is placed AFTER the #[derive(serde::Deserialize)]
    // added by ProstSerdeConfig — serde helper attributes must follow their
    // derive.
    for msg in [
        ".stepflow.v1.GetRunRequest",
        ".stepflow.v1.GetRunStepsRequest",
        ".stepflow.v1.GetRunFlowRequest",
        ".stepflow.v1.GetRunItemsRequest",
        ".stepflow.v1.GetRunEventsRequest",
        ".stepflow.v1.GetStepDetailRequest",
        ".stepflow.v1.GetFlowRequest",
        ".stepflow.v1.GetFlowVariablesRequest",
        ".stepflow.v1.ListRunsRequest",
        ".stepflow.v1.ListComponentsRequest",
        ".stepflow.v1.ListRegisteredComponentsRequest",
        ".stepflow.v1.HealthCheckRequest",
        ".stepflow.v1.StoreFlowRequest",
        ".stepflow.v1.CreateRunRequest",
        ".stepflow.v1.DeleteRunRequest",
        ".stepflow.v1.DeleteFlowRequest",
        ".stepflow.v1.CancelRunRequest",
    ] {
        config.message_attribute(msg, "#[serde(default)]");
    }

    // Phase 3: Compile protos with tonic-prost-build (types + gRPC server/client codegen)
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_with_config(config, &proto_file_refs, include_dirs)?;

    // Recompile if any proto file changes
    for proto in &proto_files {
        println!("cargo:rerun-if-changed={proto}");
    }

    Ok(())
}
