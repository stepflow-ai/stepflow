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

use serde::{Deserialize, Serialize};

use crate::schema::{Method, Notification};

/// Request from the runtime to initialize the component server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub runtime_protocol_version: u32,
    /// The protocol prefix for components served by this plugin (e.g., "python", "typescript")
    pub protocol_prefix: String,
}

/// Response to tho initializaiton request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub server_protocol_version: u32,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "initialize";
    type Response = Response;
}

/// Notification from the runtime that initialization is complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Complete {}

impl Notification for Complete {
    const NOTIFICATION_NAME: &'static str = "initialized";
}
