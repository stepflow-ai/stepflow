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

use std::sync::Arc;

use serde::Serialize;

use crate::workflow::Flow;

/// A step identifier that provides access to both the step index and name.
///
/// This type has two variants:
/// - `WithFlow`: Used during execution when you have access to the flow.
///   More efficient as it doesn't allocate for the name string.
/// - `Standalone`: Used in storage/serialization contexts where the flow
///   isn't available.
///
/// # Choosing a Constructor
///
/// When you have access to `Arc<Flow>`, prefer [`StepId::for_step()`] as it
/// avoids allocating a string for the step name. Use [`StepId::new()`] only
/// when the flow reference is not available (e.g., deserializing from storage).
///
/// # Note on Flow Lifetime
///
/// The `WithFlow` variant keeps the `Arc<Flow>` alive. If you need to store
/// a `StepId` without keeping the flow alive, use [`StepId::into_standalone()`]
/// or [`StepId::to_standalone()`].
#[derive(Clone)]
pub enum StepId {
    /// Step with flow reference - borrows name from flow, no allocation.
    WithFlow {
        /// Reference to the flow containing this step.
        flow: Arc<Flow>,
        /// The step index in the workflow.
        index: usize,
    },
    /// Standalone step - owns the name string.
    Standalone {
        /// The step name/ID.
        name: String,
        /// The step index in the workflow.
        index: usize,
    },
}

impl StepId {
    /// Create a StepId from a flow reference.
    ///
    /// This is the preferred constructor when you have access to `Arc<Flow>`
    /// as it avoids allocating a string for the step name.
    pub fn for_step(flow: Arc<Flow>, index: usize) -> Self {
        Self::WithFlow { flow, index }
    }

    /// Create a standalone StepId without a flow reference.
    ///
    /// Use this only when the flow reference is not available, such as when
    /// deserializing from storage. If you have access to `Arc<Flow>`, prefer
    /// [`StepId::for_step()`] instead to avoid the string allocation.
    pub fn new(name: String, index: usize) -> Self {
        Self::Standalone { name, index }
    }

    /// Get the step index.
    pub fn index(&self) -> usize {
        match self {
            Self::WithFlow { index, .. } => *index,
            Self::Standalone { index, .. } => *index,
        }
    }

    /// Get the step name.
    pub fn name(&self) -> &str {
        match self {
            Self::WithFlow { flow, index } => &flow.steps[*index].id,
            Self::Standalone { name, .. } => name,
        }
    }

    /// Convert to a standalone StepId, cloning if necessary.
    ///
    /// This releases the flow reference if held. Useful when you need to
    /// store the StepId without keeping the flow alive, or when serializing.
    pub fn to_standalone(&self) -> Self {
        match self {
            Self::WithFlow { flow, index } => Self::Standalone {
                name: flow.steps[*index].id.clone(),
                index: *index,
            },
            Self::Standalone { name, index } => Self::Standalone {
                name: name.clone(),
                index: *index,
            },
        }
    }

    /// Convert into a standalone StepId, consuming self.
    ///
    /// This releases the flow reference if held. More efficient than
    /// [`to_standalone()`](Self::to_standalone) when you don't need the original.
    pub fn into_standalone(self) -> Self {
        match self {
            Self::WithFlow { flow, index } => Self::Standalone {
                name: flow.steps[index].id.clone(),
                index,
            },
            standalone @ Self::Standalone { .. } => standalone,
        }
    }

    /// Returns true if this StepId holds a flow reference.
    pub fn has_flow(&self) -> bool {
        matches!(self, Self::WithFlow { .. })
    }

    /// Get the flow reference if this is a WithFlow variant.
    pub fn flow(&self) -> Option<&Arc<Flow>> {
        match self {
            Self::WithFlow { flow, .. } => Some(flow),
            Self::Standalone { .. } => None,
        }
    }
}

// Equality is based on index and name, not flow identity.
// Two StepIds are equal if they refer to the same step (same index and name).
impl PartialEq for StepId {
    fn eq(&self, other: &Self) -> bool {
        self.index() == other.index() && self.name() == other.name()
    }
}

impl Eq for StepId {}

impl std::hash::Hash for StepId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index().hash(state);
        self.name().hash(state);
    }
}

impl PartialOrd for StepId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StepId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index().cmp(&other.index())
    }
}

impl std::fmt::Display for StepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::fmt::Debug for StepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepId")
            .field("index", &self.index())
            .field("name", &self.name())
            .finish()
    }
}

// Serialization: Serialize as just the step name string.
// The index is an internal implementation detail not exposed in the API.
impl Serialize for StepId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

// Note: StepId intentionally does NOT implement Deserialize.
// When deserializing, use String for the step name and construct StepId
// with the proper index from context (flow, database, etc.).
// This prevents bugs where a deserialized StepId has an incorrect index.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValueExpr;
    use crate::workflow::builders::{FlowBuilder, StepBuilder};
    use serde_json::json;

    fn create_test_flow() -> Arc<Flow> {
        let step_one = StepBuilder::new("step_one")
            .component("/builtin/eval")
            .input_literal(json!({"expression": "1 + 1"}))
            .build();
        let step_two = StepBuilder::new("step_two")
            .component("/builtin/eval")
            .input_literal(json!({"expression": "2 + 2"}))
            .build();

        let flow = FlowBuilder::new()
            .step(step_one)
            .step(step_two)
            .output(ValueExpr::step_output("step_two"))
            .build();

        Arc::new(flow)
    }

    #[test]
    fn test_for_step_creation() {
        let flow = create_test_flow();
        let step_id = StepId::for_step(flow.clone(), 0);

        assert_eq!(step_id.index(), 0);
        assert_eq!(step_id.name(), "step_one");
        assert!(step_id.has_flow());
    }

    #[test]
    fn test_standalone_creation() {
        let step_id = StepId::new("my_step".to_string(), 5);

        assert_eq!(step_id.index(), 5);
        assert_eq!(step_id.name(), "my_step");
        assert!(!step_id.has_flow());
    }

    #[test]
    fn test_to_standalone() {
        let flow = create_test_flow();
        let step_id = StepId::for_step(flow, 1);
        let standalone = step_id.to_standalone();

        assert_eq!(standalone.index(), 1);
        assert_eq!(standalone.name(), "step_two");
        assert!(!standalone.has_flow());
    }

    #[test]
    fn test_into_standalone() {
        let flow = create_test_flow();
        let step_id = StepId::for_step(flow, 0);
        let standalone = step_id.into_standalone();

        assert_eq!(standalone.index(), 0);
        assert_eq!(standalone.name(), "step_one");
        assert!(!standalone.has_flow());
    }

    #[test]
    fn test_equality_across_variants() {
        let flow = create_test_flow();
        let with_flow = StepId::for_step(flow, 0);
        let standalone = StepId::new("step_one".to_string(), 0);

        assert_eq!(with_flow, standalone);
    }

    #[test]
    fn test_inequality_different_index() {
        let step1 = StepId::new("step".to_string(), 0);
        let step2 = StepId::new("step".to_string(), 1);

        assert_ne!(step1, step2);
    }

    #[test]
    fn test_inequality_different_name() {
        let step1 = StepId::new("step_a".to_string(), 0);
        let step2 = StepId::new("step_b".to_string(), 0);

        assert_ne!(step1, step2);
    }

    #[test]
    fn test_ordering() {
        let step0 = StepId::new("z".to_string(), 0);
        let step1 = StepId::new("a".to_string(), 1);
        let step2 = StepId::new("m".to_string(), 2);

        assert!(step0 < step1);
        assert!(step1 < step2);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let flow = create_test_flow();
        let with_flow = StepId::for_step(flow, 0);
        let standalone = StepId::new("step_one".to_string(), 0);

        let mut hasher1 = DefaultHasher::new();
        with_flow.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        standalone.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_serialize_as_string() {
        let step_id = StepId::new("my_step".to_string(), 5);
        let json = serde_json::to_string(&step_id).unwrap();
        // Should serialize as just the step name string
        assert_eq!(json, "\"my_step\"");
    }

    #[test]
    fn test_serialize_with_flow() {
        // Verify WithFlow variant also serializes as just the name
        let flow = create_test_flow();
        let step_id = StepId::for_step(flow, 1);
        let json = serde_json::to_string(&step_id).unwrap();
        assert_eq!(json, "\"step_two\"");
    }

    // Note: StepId intentionally does NOT implement Deserialize.
    // This prevents bugs where deserialized StepIds have incorrect indices.
    // Use String for deserialization and construct StepId with proper index from context.

    #[test]
    fn test_display() {
        let step_id = StepId::new("my_step".to_string(), 3);
        assert_eq!(format!("{}", step_id), "my_step");
    }

    #[test]
    fn test_debug() {
        let step_id = StepId::new("my_step".to_string(), 3);
        let debug_str = format!("{:?}", step_id);
        assert!(debug_str.contains("index: 3"));
        assert!(debug_str.contains("name: \"my_step\""));
    }
}
