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

use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum StdioError {
    #[error("error spawning stepflow component process")]
    Spawn,
    #[error("error sending message")]
    Send,
    #[error("error receiving message")]
    Recv,
    #[error("received invalid message")]
    InvalidMessage,
    #[error("received invalid response")]
    InvalidResponse,
    #[error("components server error({code}): {message}")]
    ServerError {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
    #[error("components server failed with exit code {exit_code:?}")]
    ServerFailure { exit_code: Option<i32> },
    #[error("error closing stepflow component process")]
    Close,
    #[error("invalid command: {}", .0.display())]
    InvalidCommand(PathBuf),
    #[error("error in receive loop")]
    RecvLoop,
    #[error("command not found: {0}")]
    MissingCommand(String),
    #[error("unknown method: {method}")]
    UnknownMethod { method: String },
}

pub type Result<T, E = error_stack::Report<StdioError>> = std::result::Result<T, E>;
