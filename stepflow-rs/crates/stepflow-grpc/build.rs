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

    let proto_files: Vec<String> = [
        "stepflow/v1/common.proto",
        "stepflow/v1/health.proto",
        "stepflow/v1/components.proto",
        "stepflow/v1/tasks.proto",
        "stepflow/v1/orchestrator.proto",
        "stepflow/v1/blobs.proto",
        "stepflow/v1/flows.proto",
        "stepflow/v1/runs.proto",
    ]
    .iter()
    .map(|p| proto_root.join(p).to_str().unwrap().to_string())
    .collect();

    let proto_file_refs: Vec<&str> = proto_files.iter().map(String::as_str).collect();
    let include_dirs: &[&str] = &[proto_root.to_str().unwrap()];

    // Phase 1: Compile protos to a file descriptor set (needed by tonic-rest)
    let descriptor_bytes = tonic_rest_build::dump_file_descriptor_set(
        &proto_file_refs,
        include_dirs,
        &descriptor_path,
    );

    // Phase 2: Generate REST routes from google.api.http annotations.
    // Proto types and gRPC service traits come from stepflow-proto; this crate
    // re-exports them at `crate::stepflow::v1::*` so the generated REST route
    // code can reference them.
    let rest_config = tonic_rest_build::RestCodegenConfig::new();
    let rest_code = tonic_rest_build::generate(&descriptor_bytes, &rest_config)?;
    std::fs::write(format!("{out_dir}/rest_routes.rs"), rest_code)?;

    // Recompile if any proto file changes
    for proto in &proto_files {
        println!("cargo:rerun-if-changed={proto}");
    }

    // Phase 3: Generate build-time metadata (git commit, dirty, tags)
    built::write_built_file()?;

    Ok(())
}
