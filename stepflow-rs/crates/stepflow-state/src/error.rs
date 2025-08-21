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

use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("State store initialization error")]
    Initialization,

    #[error("State store connection error")]
    Connection,

    #[error("Internal state store error")]
    Internal,

    #[error("Blob not found: {blob_id}")]
    BlobNotFound { blob_id: String },

    #[error("Step result not found for run {run_id}, step index {step_idx}")]
    StepResultNotFoundByIndex { run_id: String, step_idx: usize },

    #[error("Step result not found for run {run_id}, step id '{step_id}'")]
    StepResultNotFoundById { run_id: Uuid, step_id: String },

    #[error("Run not found: {run_id}")]
    RunNotFound { run_id: Uuid },

    #[error("Serialization error")]
    Serialization,
}

pub type Result<T, E = error_stack::Report<StateError>> = std::result::Result<T, E>;
