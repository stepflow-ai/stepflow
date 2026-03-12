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

//! Type-erased environment carrier for Stepflow execution.
//!
//! This module provides a type map pattern for storing shared resources
//! needed during workflow execution. The design decouples the environment
//! carrier from its contents, allowing stepflow-core to remain independent
//! of plugin and state store implementations.

use std::any::{Any, TypeId};
use std::path::PathBuf;

use dashmap::DashMap;

/// Environment for Stepflow flow execution.
///
/// A type map that holds shared resources needed for flow execution.
/// Resources are stored by their type and retrieved with type safety.
///
/// # Design
///
/// This uses a type map pattern where each unique type can have at most
/// one value stored. This enables dependency injection without hard-coding
/// specific dependencies in the core crate.
///
/// # Thread Safety
///
/// The environment supports concurrent reads and writes via interior mutability
/// (`DashMap`). All stored values must be `Send + Sync` for safe sharing across
/// async tasks.
///
/// # Example
///
/// ```ignore
/// use stepflow_core::StepflowEnvironment;
///
/// let env = StepflowEnvironment::new();
/// env.insert(PathBuf::from("/working/dir"));
///
/// // Later, retrieve typed values
/// let dir = env.working_directory();
/// ```
pub struct StepflowEnvironment {
    resources: DashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Default for StepflowEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl StepflowEnvironment {
    /// Create a new empty environment.
    pub fn new() -> Self {
        Self {
            resources: DashMap::new(),
        }
    }

    /// Insert a resource into the environment.
    ///
    /// If a resource of the same type already exists, it is replaced.
    pub fn insert<T: Send + Sync + 'static>(&self, value: T) {
        self.resources.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get a cloned copy of a resource by type.
    ///
    /// Returns `None` if no resource of that type has been inserted.
    pub fn get<T: Clone + 'static>(&self) -> Option<T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|r| (**r).downcast_ref::<T>().cloned())
    }

    /// Get a cloned copy of a resource by type.
    ///
    /// Panics if no resource of that type has been inserted.
    pub fn get_expected<T: Clone + 'static>(&self) -> T {
        self.get().unwrap_or_else(|| {
            panic!(
                "Missing expected value of type {}",
                std::any::type_name::<T>()
            )
        })
    }

    /// Check if a resource of the given type exists.
    pub fn contains<T: 'static>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    // ========================================================================
    // Built-in accessors for common types
    // These provide ergonomic access without requiring extension traits
    // ========================================================================

    /// Get the working directory.
    ///
    /// # Panics
    ///
    /// Panics if working directory was not set during environment construction.
    pub fn working_directory(&self) -> PathBuf {
        self.get::<PathBuf>()
            .expect("working_directory not set in environment")
    }
}

impl std::fmt::Debug for StepflowEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepflowEnvironment")
            .field("resource_count", &self.resources.len())
            .finish_non_exhaustive()
    }
}

impl Drop for StepflowEnvironment {
    fn drop(&mut self) {
        log::info!("StepflowEnvironment being dropped, cleaning up resources");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_insert_and_get() {
        let env = StepflowEnvironment::new();
        env.insert(PathBuf::from("/test/dir"));

        let path = env.get::<PathBuf>().unwrap();
        assert_eq!(path.as_path(), std::path::Path::new("/test/dir"));
    }

    #[test]
    fn test_insert_replaces_existing() {
        let env = StepflowEnvironment::new();
        env.insert(PathBuf::from("/first"));
        env.insert(PathBuf::from("/second"));

        assert_eq!(
            env.get::<PathBuf>().unwrap().as_path(),
            std::path::Path::new("/second")
        );
    }

    #[test]
    fn test_get_missing_returns_none() {
        let env = StepflowEnvironment::new();
        assert!(env.get::<PathBuf>().is_none());
    }

    #[test]
    fn test_contains() {
        let env = StepflowEnvironment::new();
        assert!(!env.contains::<PathBuf>());

        env.insert(PathBuf::from("/test"));
        assert!(env.contains::<PathBuf>());
    }

    #[test]
    fn test_working_directory() {
        let env = StepflowEnvironment::new();
        env.insert(PathBuf::from("/working/dir"));

        assert_eq!(env.working_directory(), PathBuf::from("/working/dir"));
    }

    #[test]
    #[should_panic(expected = "working_directory not set")]
    fn test_working_directory_panics_if_not_set() {
        let env = StepflowEnvironment::new();
        let _ = env.working_directory();
    }

    #[test]
    fn test_arc_wrapped_type() {
        let env = StepflowEnvironment::new();
        let value: Arc<String> = Arc::new("test".to_string());
        env.insert(value.clone());

        let retrieved = env.get::<Arc<String>>().unwrap();
        assert_eq!(*retrieved, "test");
    }

    #[test]
    fn test_multiple_types() {
        let env = StepflowEnvironment::new();
        env.insert(PathBuf::from("/path"));
        env.insert(42u32);
        env.insert("hello".to_string());

        assert_eq!(
            env.get::<PathBuf>().unwrap().as_path(),
            std::path::Path::new("/path")
        );
        assert_eq!(env.get::<u32>().unwrap(), 42);
        assert_eq!(env.get::<String>().unwrap(), "hello");
    }
}
