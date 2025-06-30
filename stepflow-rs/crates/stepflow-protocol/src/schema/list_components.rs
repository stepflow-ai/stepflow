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
use stepflow_core::workflow::Component;

use crate::schema::Method;

/// Request from the runtime to initialize the component server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {}

/// Response to the initializaiton request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub components: Vec<Component>,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "list_components";
    type Response = Response;
}
