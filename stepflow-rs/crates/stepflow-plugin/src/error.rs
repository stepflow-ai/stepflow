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

use stepflow_core::workflow::Component;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("error initializing plugin")]
    Initializing,
    #[error("error getting component info")]
    ComponentInfo,
    #[error("unknown component: {0}")]
    UnknownComponent(Component),
    #[error("error executing component")]
    Execution,
    #[error("error importing user-defined function")]
    UdfImport,
    #[error("error executing user-defined function")]
    UdfExecution,
    #[error("error decoding user-defined function")]
    UdfResults,
    #[error("no step plugin registered for protocol '{0}'")]
    UnknownScheme(String),
    #[error("unable to downcast plugin for protocol '{0}'")]
    DowncastErr(String),
    #[error("plugins already initialized")]
    AlreadyInitialized,
    #[error("invalid input")]
    InvalidInput,
    #[error("error creating plugin")]
    CreatePlugin,
    #[error("invalid rule index: {0}")]
    InvalidRuleIndex(usize),
    #[error("plugin not found: {0}")]
    PluginNotFound(String),
    #[error("configuration error")]
    Configuration,
}

pub type Result<T, E = error_stack::Report<PluginError>> = std::result::Result<T, E>;
