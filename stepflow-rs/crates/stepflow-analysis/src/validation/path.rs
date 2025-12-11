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

#[derive(
    Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub(crate) enum PathPart {
    String(Cow<'static, str>),
    Index(usize),
}

impl From<&'static str> for PathPart {
    fn from(s: &'static str) -> Self {
        PathPart::String(Cow::Borrowed(s))
    }
}

impl From<String> for PathPart {
    fn from(value: String) -> Self {
        PathPart::String(Cow::Owned(value))
    }
}

impl From<usize> for PathPart {
    fn from(value: usize) -> Self {
        PathPart::Index(value)
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[repr(transparent)]
pub struct Path(pub(crate) Vec<PathPart>);

fn needs_quotation(s: &str) -> bool {
    s.contains('.') || s.contains('[') || s.contains(']')
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.0.is_empty() {
            write!(f, "$")?;
            for part in self.0.iter() {
                match part {
                    PathPart::String(v) if needs_quotation(v) => write!(f, "[{v:?}]")?,
                    PathPart::String(v) => write!(f, ".{v}")?,
                    PathPart::Index(v) => write!(f, "[{v}]")?,
                }
            }
        }
        Ok(())
    }
}

macro_rules! make_path {
    ($($es:expr),*) => {
        {
            $crate::validation::Path(vec![
                $($crate::validation::PathPart::from($es),)*
            ])
        }
    }
}
use std::borrow::Cow;

pub(crate) use make_path;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

impl Path {
    pub fn new() -> Path {
        Path(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn push(&mut self, part: impl Into<PathPart>) {
        self.0.push(part.into());
    }

    pub(crate) fn pop(&mut self) {
        self.0.pop();
    }

    pub(crate) fn prepend(&mut self, prefix: &Path) {
        let mut new_path = prefix.0.clone();
        new_path.append(&mut self.0);
        self.0 = new_path;
    }
}

#[cfg(test)]
mod tests {
    use super::make_path;

    #[test]
    fn test_make_path() {
        // Basic path creations
        assert_eq!(make_path!().to_string(), "");
        assert_eq!(make_path!("hello").to_string(), "$.hello");
        assert_eq!(make_path!("hello", 5usize).to_string(), "$.hello[5]");
        assert_eq!(
            make_path!("hello", 5usize, "world").to_string(),
            "$.hello[5].world"
        );
    }
}
