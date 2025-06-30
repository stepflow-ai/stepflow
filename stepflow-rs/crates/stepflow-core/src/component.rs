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

//! Component information and metadata types.

use crate::schema::SchemaRef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// The input schema for the component.
    ///
    /// Can be any valid JSON schema (object, primitive, array, etc.).
    pub input_schema: SchemaRef,

    /// The output schema for the component.
    ///
    /// Can be any valid JSON schema (object, primitive, array, etc.).
    pub output_schema: SchemaRef,

    /// Optional description of what the component does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
