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

use std::borrow::Cow;
use std::path::PathBuf;

use thiserror::Error;

use crate::protocol::Method;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("error spawning stepflow component process")]
    Spawn,
    #[error("error sending message")]
    Send,
    #[error("error receiving message")]
    Recv,
    #[error("expecting response, but got {0:?}")]
    NotResponse(String),
    #[error("invalid response for {0}")]
    InvalidResponse(Method),
    #[error("invalid message: {0}")]
    InvalidMessage(String),
    #[error("failed to serialize request for method '{0}'")]
    SerializeRequest(Method),
    #[error("component server error({code}): {message}")]
    ServerError {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
    #[error("component server failed with exit code {exit_code:?}")]
    ServerFailure { exit_code: Option<i32> },
    #[error("error closing stepflow component process")]
    Close,
    #[error("invalid command: {}", .0.display())]
    InvalidCommand(PathBuf),
    #[error("error in receive loop")]
    RecvLoop,
    #[error("command not found: {0}")]
    MissingCommand(String),
    #[error("unknown method: {method:?}")]
    UnknownMethod { method: String },
    #[error("method requested without ID: {method:?}")]
    MethodMissingId { method: Cow<'static, str> },
    #[error("invalid environment variable: {0}")]
    InvalidEnvironmentVariable(String),
    #[error("invalid value for field '{field}': expected {expected}")]
    InvalidValue {
        field: &'static str,
        expected: &'static str,
    },
}

pub type Result<T, E = error_stack::Report<TransportError>> = std::result::Result<T, E>;
