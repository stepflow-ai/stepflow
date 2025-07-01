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

use error_stack::ResultExt as _;

use crate::Message;
use crate::lazy_value::LazyValue;
use crate::stdio::{Result, StdioError};

/// Owned wrapper around a `Message` parsed from a `String`.
///
/// This doesn't quite fit the behavior of `owning_ref` because
/// the message is *owned* with interior references -- eg., it is
/// the result of parsing the string.
pub struct OwnedJson<T = Message<'static>> {
    /// Not used directly, but it is necessary to keep the JSON content alive.
    /// The `message` field potentially references data from this string.
    #[allow(dead_code)]
    json: Box<str>,
    /// The incoming message.
    ///
    /// This is not *really* `'static` because it references data from `json`.
    /// But, it needs to look `'static` to appease the borrow checker. It is critical
    /// that we only return a reference to this with a lifetime shotrter than or equal
    /// to `self`.
    value: T,
}

impl<T: std::fmt::Debug> std::fmt::Debug for OwnedJson<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Owned({:?})", self.value)
    }
}

impl<T> OwnedJson<T> {
    pub fn json(&self) -> &str {
        &self.json
    }

    pub fn map<O>(self, f: impl FnOnce(T) -> O) -> OwnedJson<O> {
        OwnedJson {
            json: self.json,
            value: f(self.value),
        }
    }
}

impl OwnedJson<Message<'static>> {
    pub fn try_new(json: String) -> Result<Self> {
        let json = json.into_boxed_str();
        let message: Message<'_> = serde_json::from_str(&json)
            .change_context_lazy(|| StdioError::InvalidMessage(json.to_string()))?;

        // SAFETY: we take a reference to JSON, which we will ensure lives at least as long
        // as the message.
        let message = unsafe { std::mem::transmute::<Message<'_>, Message<'static>>(message) };
        Ok(Self {
            json,
            value: message,
        })
    }

    pub fn message(&self) -> &Message<'_> {
        &self.value
    }
}

impl OwnedJson<LazyValue<'static>> {
    pub fn value(&self) -> &LazyValue<'_> {
        &self.value
    }
}
