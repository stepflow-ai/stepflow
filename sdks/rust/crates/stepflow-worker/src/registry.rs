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

//! Component registration and lookup.

use std::sync::Arc;

use indexmap::IndexMap;

use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::{ComponentContext, ComponentError};

// ---------------------------------------------------------------------------
// Component trait
// ---------------------------------------------------------------------------

/// A Stepflow component that can be registered with a [`ComponentRegistry`].
///
/// Implement this trait directly for full control, or use
/// [`ComponentRegistry::register_fn`] for a simpler closure-based approach.
///
/// Two concepts are at play:
///
/// * **Component ID** ([`name()`](Component::name)) — the unique identifier
///   used as the dispatch key. The orchestrator sends this value as
///   `component_id` in `ComponentExecuteRequest`.
/// * **Subpath** ([`path()`](Component::path)) — the path pattern the
///   component is mounted at, relative to the worker's route prefix.
///   Defaults to `"/{name}"`.
///
/// For example, if the orchestrator config maps prefix `"/python"` to this
/// worker, and a component has `name() = "echo"` (subpath defaults to
/// `"/echo"`), then workflows reference it as `/python/echo` — **not** `/echo`.
///
/// ```text
/// Orchestrator config          Component ID / subpath       Full workflow path
/// ─────────────────────        ──────────────────────       ──────────────────
/// routes:                      name() → "echo"              /python/echo
///   "/python":                 path() → "/echo"
///     - plugin: python
///
///   Full path = prefix "/python" + subpath "/echo" = "/python/echo"
/// ```
///
/// Do **not** include the prefix in component IDs or subpaths — the
/// orchestrator adds it automatically.
///
/// # Example
///
/// ```rust,no_run
/// use async_trait::async_trait;
/// use stepflow_worker::{Component, ComponentContext, ComponentError};
///
/// struct EchoComponent;
///
/// #[async_trait]
/// impl Component for EchoComponent {
///     fn name(&self) -> &str { "echo" }
///
///     async fn execute(
///         &self,
///         input: serde_json::Value,
///         _ctx: &ComponentContext,
///     ) -> Result<serde_json::Value, ComponentError> {
///         Ok(input)
///     }
/// }
/// ```
#[async_trait]
pub trait Component: Send + Sync + 'static {
    /// The component's unique identifier, used as the dispatch key.
    ///
    /// The orchestrator sends this value as `component_id` in
    /// `ComponentExecuteRequest`. It is unrelated to the component's
    /// subpath — see [`path()`](Component::path) for the subpath.
    fn name(&self) -> &str;

    /// The subpath pattern this component is registered at, relative to the
    /// worker's route prefix.
    ///
    /// This is the component's path *within* the worker — the orchestrator
    /// prepends the route prefix to form the full workflow path. For example,
    /// with route prefix `"/python"` and subpath `"/echo"`, the full workflow
    /// path is `/python/echo`.
    ///
    /// Defaults to `"/{name}"` (e.g., name `"echo"` → subpath `"/echo"`).
    ///
    /// Override this for wildcard patterns like `"/core/{*path}"`. With prefix
    /// `"/python"`, the full workflow path becomes `/python/core/...`.
    fn path(&self) -> String {
        format!("/{}", self.name())
    }

    /// Optional human-readable description reported during component discovery.
    fn description(&self) -> Option<&str> {
        None
    }

    /// Optional JSON Schema for this component's input.
    fn input_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// Optional JSON Schema for this component's output.
    fn output_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// Execute the component with the given input and context.
    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ComponentContext,
    ) -> Result<serde_json::Value, ComponentError>;
}

// ---------------------------------------------------------------------------
// Typed closure wrapper
// ---------------------------------------------------------------------------

/// A component built from a typed async closure.
///
/// Created by [`ComponentRegistry::register_fn`].
struct FnComponent<I, O, F> {
    name: String,
    path: Option<String>,
    description: Option<String>,
    input_schema: Option<serde_json::Value>,
    output_schema: Option<serde_json::Value>,
    f: F,
    _phantom: std::marker::PhantomData<fn(I) -> O>,
}

#[async_trait]
impl<I, O, F, Fut> Component for FnComponent<I, O, F>
where
    I: DeserializeOwned + Send + 'static,
    O: Serialize + Send + 'static,
    F: Fn(I, ComponentContext) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<O, ComponentError>> + Send + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn path(&self) -> String {
        self.path
            .clone()
            .unwrap_or_else(|| format!("/{}", self.name))
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn input_schema(&self) -> Option<serde_json::Value> {
        self.input_schema.clone()
    }

    fn output_schema(&self) -> Option<serde_json::Value> {
        self.output_schema.clone()
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ComponentContext,
    ) -> Result<serde_json::Value, ComponentError> {
        let typed_input: I = serde_json::from_value(input)
            .map_err(|e| ComponentError::InvalidInput(e.to_string()))?;
        let result = (self.f)(typed_input, ctx.clone()).await?;
        serde_json::to_value(result)
            .map_err(|e| ComponentError::WorkerError(format!("Serialization error: {e}")))
    }
}

// ---------------------------------------------------------------------------
// ComponentRegistry
// ---------------------------------------------------------------------------

/// Registry of components that a [`crate::Worker`] can execute.
///
/// Components are looked up by their [`Component::name`] (the component ID).
/// The orchestrator handles all path matching and sends the `component_id`
/// (which corresponds to `name()`) in each `ComponentExecuteRequest`.
///
/// # Example
///
/// ```rust,no_run
/// use stepflow_worker::{ComponentRegistry, ComponentError};
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Deserialize)]
/// struct AddInput { a: f64, b: f64 }
///
/// #[derive(Serialize)]
/// struct AddOutput { result: f64 }
///
/// let mut registry = ComponentRegistry::new();
/// registry.register_fn("add", |input: AddInput, _ctx| async move {
///     Ok(AddOutput { result: input.a + input.b })
/// });
/// ```
pub struct ComponentRegistry {
    /// O(1) lookup by component ID, with insertion-order iteration for
    /// deterministic `ListComponents` responses.
    components: IndexMap<String, Arc<dyn Component>>,
}

impl ComponentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            components: IndexMap::new(),
        }
    }

    /// Register a component that implements the [`Component`] trait.
    ///
    /// # Panics
    ///
    /// Panics if a component with the same ID is already registered.
    /// Use [`try_register`](Self::try_register) for recoverable error handling.
    pub fn register(&mut self, component: impl Component) -> &mut Self {
        self.try_register(component)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// Register a component, returning an error instead of panicking on failure.
    ///
    /// Returns [`crate::error::WorkerError::DuplicateComponent`] if a component with the same
    /// ID is already registered.
    pub fn try_register(
        &mut self,
        component: impl Component,
    ) -> Result<&mut Self, crate::error::WorkerError> {
        let name = component.name().to_string();
        if self.components.contains_key(&name) {
            return Err(crate::error::WorkerError::DuplicateComponent(name));
        }
        self.components.insert(name, Arc::new(component));
        Ok(self)
    }

    /// Register a typed async closure as a component.
    ///
    /// `I` and `O` are automatically inferred from the closure's type signature.
    /// The input is deserialized from JSON before calling the closure, and the
    /// output is serialized back to JSON afterwards.
    ///
    /// # Panics
    ///
    /// Panics if the component ID is already registered. Use
    /// [`try_register_fn`](Self::try_register_fn) for recoverable error handling.
    pub fn register_fn<I, O, F, Fut>(&mut self, name: impl Into<String>, f: F) -> &mut Self
    where
        I: DeserializeOwned + Send + 'static,
        O: Serialize + Send + 'static,
        F: Fn(I, ComponentContext) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<O, ComponentError>> + Send + 'static,
    {
        self.try_register_fn(name, f)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// Register a typed async closure, returning an error instead of panicking on failure.
    pub fn try_register_fn<I, O, F, Fut>(
        &mut self,
        name: impl Into<String>,
        f: F,
    ) -> Result<&mut Self, crate::error::WorkerError>
    where
        I: DeserializeOwned + Send + 'static,
        O: Serialize + Send + 'static,
        F: Fn(I, ComponentContext) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<O, ComponentError>> + Send + 'static,
    {
        let component = FnComponent {
            name: name.into(),
            path: None,
            description: None,
            input_schema: None,
            output_schema: None,
            f,
            _phantom: std::marker::PhantomData,
        };
        self.try_register(component)
    }

    /// Look up a component by ID.
    ///
    /// The orchestrator resolves path patterns and sends the `component_id`
    /// (which matches [`Component::name`]) in the task assignment.
    pub(crate) fn lookup(&self, component_id: &str) -> Option<Arc<dyn Component>> {
        self.components.get(component_id).cloned()
    }

    /// Return metadata for all registered components in insertion order.
    ///
    /// Used for `ListComponents` responses sent to the orchestrator.
    pub(crate) fn list_components(&self) -> Vec<ComponentInfo> {
        self.components
            .values()
            .map(|c| ComponentInfo {
                name: c.name().to_string(),
                path: c.path(),
                description: c.description().map(str::to_string),
                input_schema: c.input_schema(),
                output_schema: c.output_schema(),
            })
            .collect()
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata about a registered component.
#[derive(Debug, Clone)]
pub(crate) struct ComponentInfo {
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_lookup() {
        let mut registry = ComponentRegistry::new();
        registry.register_fn(
            "echo",
            |input: serde_json::Value, _ctx| async move { Ok(input) },
        );

        assert!(registry.lookup("echo").is_some());
        assert!(registry.lookup("other").is_none());
    }

    #[test]
    fn test_lookup_is_flat_by_id() {
        // Lookup is by exact component ID, not by path pattern matching.
        let mut registry = ComponentRegistry::new();
        registry.register_fn(
            "add",
            |input: serde_json::Value, _ctx| async move { Ok(input) },
        );

        assert!(registry.lookup("add").is_some());
        // Path-style lookups do not match — only exact name does.
        assert!(registry.lookup("/math/add").is_none());
    }

    #[test]
    fn test_list_components_insertion_order() {
        let mut registry = ComponentRegistry::new();
        registry.register_fn("a", |_: serde_json::Value, _ctx| async move {
            Ok(serde_json::Value::Null)
        });
        registry.register_fn("b", |_: serde_json::Value, _ctx| async move {
            Ok(serde_json::Value::Null)
        });
        registry.register_fn("c", |_: serde_json::Value, _ctx| async move {
            Ok(serde_json::Value::Null)
        });

        let list = registry.list_components();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "a");
        assert_eq!(list[1].name, "b");
        assert_eq!(list[2].name, "c");
    }

    #[test]
    fn test_list_components_includes_path() {
        let mut registry = ComponentRegistry::new();
        registry.register_fn("echo", |_: serde_json::Value, _ctx| async move {
            Ok(serde_json::Value::Null)
        });

        let list = registry.list_components();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "echo");
        assert_eq!(list[0].path, "/echo");
    }

    #[test]
    fn test_duplicate_registration_returns_error() {
        let mut registry = ComponentRegistry::new();
        registry.register_fn(
            "echo",
            |input: serde_json::Value, _ctx| async move { Ok(input) },
        );

        let result = registry.try_register_fn(
            "echo",
            |input: serde_json::Value, _ctx| async move { Ok(input) },
        );

        assert!(
            matches!(
                result,
                Err(crate::error::WorkerError::DuplicateComponent(_))
            ),
            "expected DuplicateComponent error"
        );
    }

    #[test]
    fn test_register_panics_on_duplicate() {
        let result = std::panic::catch_unwind(|| {
            let mut registry = ComponentRegistry::new();
            registry.register_fn(
                "echo",
                |input: serde_json::Value, _ctx| async move { Ok(input) },
            );
            registry.register_fn(
                "echo",
                |input: serde_json::Value, _ctx| async move { Ok(input) },
            );
        });
        assert!(result.is_err(), "register should panic on duplicate");
    }
}
