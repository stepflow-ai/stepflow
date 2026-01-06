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

use owning_ref::ArcRef;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;
use utoipa::openapi::schema::Schema;
use utoipa::openapi::{ObjectBuilder, RefOr};

/// A literal value which may be passed to a component.
///
/// This is a reference to a value owned by an `Arc<serde_json::Value>`.
///
/// The value is projected to a subfield of the `Arc` when accessed.
///
/// This is useful for avoiding cloning the value when accessing nested fields.
///
/// The value is projected to a subfield of the `Arc` when accessed.
// TODO: Look at expanding ValueRef representaiton to be an explicit enum
// allowing expansion of templates without duplicating literal sub-trees.
#[derive(Clone, PartialEq, Hash, Eq)]
#[repr(transparent)]
pub struct ValueRef<T: 'static = serde_json::Value>(ArcRef<'static, serde_json::Value, T>);

impl<T: std::fmt::Debug> std::fmt::Debug for ValueRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Default for ValueRef {
    fn default() -> Self {
        Self(ArcRef::new(Arc::new(serde_json::Value::Null)))
    }
}

impl utoipa::PartialSchema for ValueRef {
    fn schema() -> RefOr<Schema> {
        // Use an empty object schema to represent "any JSON value"
        // This avoids the invalid empty allOf: [] pattern
        RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .description(Some(
                    "Any JSON value (object, array, string, number, boolean, or null)",
                ))
                .build(),
        ))
    }
}

impl utoipa::ToSchema for ValueRef {
    fn name() -> Cow<'static, str> {
        // Use "Value" as the semantic name (the "Ref" is an implementation detail)
        Cow::Borrowed("Value")
    }
}

impl Serialize for ValueRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.as_ref().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ValueRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_json::Value::deserialize(deserializer).map(Self::new)
    }
}

impl<T: Into<serde_json::Value>> From<T> for ValueRef {
    fn from(value: T) -> Self {
        Self::new(value.into())
    }
}

impl ValueRef<serde_json::Value> {
    pub fn new(value: serde_json::Value) -> Self {
        Self(ArcRef::new(Arc::new(value)))
    }

    /// Return a redacted version of this value ref for printing.
    pub fn redacted<'a>(
        &'a self,
        secrets: &'a crate::values::Secrets,
    ) -> crate::values::RedactedValue<'a> {
        secrets.redacted(self.value())
    }

    pub fn is_truthy(&self) -> bool {
        match self.0.as_ref() {
            serde_json::Value::Bool(b) => *b,
            serde_json::Value::Number(n) => {
                if let Some(n) = n.as_u64() {
                    n != 0
                } else if let Some(n) = n.as_i64() {
                    n != 0
                } else {
                    n.as_f64().unwrap() != 0.0
                }
            }
            serde_json::Value::String(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// Access an object field by name.
    pub fn path(&self, path: &str) -> Option<ValueRef> {
        match self.0.as_ref() {
            serde_json::Value::Object(obj) => obj
                .get(path)
                .map(|v| ValueRef(project_to_subfield(self.0.clone(), v))),
            _ => None,
        }
    }

    /// Access an array element by index.
    pub fn index(&self, index: usize) -> Option<ValueRef> {
        match self.0.as_ref() {
            serde_json::Value::Array(arr) => arr
                .get(index)
                .map(|v| ValueRef(project_to_subfield(self.0.clone(), v))),
            _ => None,
        }
    }

    /// Access value using a JSON path
    pub fn resolve_json_path(&self, json_path: &crate::workflow::JsonPath) -> Option<ValueRef> {
        use crate::workflow::PathPart;

        let mut current = self.clone();

        for part in json_path.parts() {
            match part {
                PathPart::Field(field_name) => {
                    current = current.path(field_name)?;
                }
                PathPart::Index(index) => {
                    current = current.index(*index)?;
                }
                PathPart::IndexStr(index_str) => {
                    current = current.path(index_str)?;
                }
            }
        }

        Some(current)
    }

    /// Cast to an object if this value is an object
    pub fn as_object(&self) -> Option<ValueRef<serde_json::Map<String, serde_json::Value>>> {
        match self.0.as_ref() {
            serde_json::Value::Object(obj) => {
                Some(ValueRef(project_to_subfield(self.0.clone(), obj)))
            }
            _ => None,
        }
    }

    /// Cast to an array if this value is an array
    pub fn as_array(&self) -> Option<ValueRef<Vec<serde_json::Value>>> {
        match self.0.as_ref() {
            serde_json::Value::Array(arr) => {
                Some(ValueRef(project_to_subfield(self.0.clone(), arr)))
            }
            _ => None,
        }
    }

    pub fn value(&self) -> &serde_json::Value {
        self.0.as_ref()
    }

    /// Clone the underlying JSON value
    pub fn clone_value(&self) -> serde_json::Value {
        self.0.as_ref().clone()
    }

    /// Deserialize the value into a specific type
    pub fn deserialize<T>(&self) -> Result<T, serde_json::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(self.as_ref().clone())
    }

    /// Get the value as a boolean if it is one
    pub fn as_bool(&self) -> Option<bool> {
        match self.0.as_ref() {
            serde_json::Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get the value as a string if it is one
    pub fn as_str(&self) -> Option<&str> {
        match self.0.as_ref() {
            serde_json::Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Get the value as a number if it is one
    pub fn as_number(&self) -> Option<&serde_json::Number> {
        match self.0.as_ref() {
            serde_json::Value::Number(n) => Some(n),
            _ => None,
        }
    }

    /// Check if the value is null
    pub fn is_null(&self) -> bool {
        matches!(self.0.as_ref(), serde_json::Value::Null)
    }
}

impl ValueRef<Vec<serde_json::Value>> {
    pub fn iter(&self) -> impl Iterator<Item = ValueRef> + '_ {
        (0..self.0.len()).map(move |i| ValueRef(self.0.clone().map(move |vec| &vec[i])))
    }

    pub fn get(&self, index: usize) -> Option<ValueRef> {
        if index < self.0.len() {
            Some(ValueRef(self.0.clone().map(move |vec| &vec[index])))
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// Helper function to project an ArcRef to a subfield
// SAFETY: This is safe because we're projecting to a subfield of the same object
// that the ArcRef already owns, and the lifetime is constrained appropriately.
fn project_to_subfield<C, T1, T2>(arc: ArcRef<'static, C, T1>, value: &T2) -> ArcRef<'static, C, T2>
where
    T1: 'static,
    T2: 'static,
{
    let ptr = value as *const T2;
    // SAFETY: The pointer is valid because it points to a subfield of the object
    // that ArcRef owns, and T2: 'static ensures the subfield has the right lifetime
    arc.map(move |_| unsafe { &*ptr })
}

impl ValueRef<serde_json::Map<String, serde_json::Value>> {
    pub fn iter(&self) -> impl Iterator<Item = (&str, ValueRef)> + '_ {
        self.0.iter().map(|(k, v)| {
            let k = k.as_str();
            (k, ValueRef(project_to_subfield(self.0.clone(), v)))
        })
    }

    pub fn get(&self, key: &str) -> Option<ValueRef> {
        self.0
            .get(key)
            .map(|v| ValueRef(project_to_subfield(self.0.clone(), v)))
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(|s| s.as_str())
    }
}

// Implement AsRef trait for compatibility with existing code
impl AsRef<serde_json::Value> for ValueRef {
    fn as_ref(&self) -> &serde_json::Value {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use serde_json::json;

    #[test]
    fn test_value_ref_new() {
        let value = json!({"key": "value"});
        let value_ref = ValueRef::new(value.clone());
        assert_eq!(value_ref.as_ref(), &value);
    }

    #[test]
    fn test_value_ref_path_object() {
        let value = json!({
            "name": "Alice",
            "age": 30,
            "nested": {
                "city": "San Francisco"
            }
        });
        let value_ref = ValueRef::new(value);

        // Test accessing top-level field
        let name = value_ref.path("name").unwrap();
        assert_eq!(name.as_ref(), &json!("Alice"));

        // Test accessing nested object
        let nested = value_ref.path("nested").unwrap();
        assert_eq!(nested.as_ref(), &json!({"city": "San Francisco"}));

        // Test accessing nested field through path of nested object
        let city = nested.path("city").unwrap();
        assert_eq!(city.as_ref(), &json!("San Francisco"));

        // Test non-existent field
        assert!(value_ref.path("nonexistent").is_none());
    }

    #[test]
    fn test_value_ref_path_array() {
        let value = json!(["first", "second", {"nested": "value"}]);
        let value_ref = ValueRef::new(value);

        // Test accessing array elements by index
        let first = value_ref.index(0).unwrap();
        assert_eq!(first.as_ref(), &json!("first"));

        let second = value_ref.index(1).unwrap();
        assert_eq!(second.as_ref(), &json!("second"));

        let third = value_ref.index(2).unwrap();
        assert_eq!(third.as_ref(), &json!({"nested": "value"}));

        // Test accessing nested field in array element
        let nested = third.path("nested").unwrap();
        assert_eq!(nested.as_ref(), &json!("value"));

        // Test out of bounds
        assert!(value_ref.index(10).is_none());
    }

    #[test]
    fn test_value_ref_as_object() {
        let value = json!({
            "key1": "value1",
            "key2": "value2"
        });
        let value_ref = ValueRef::new(value);

        let obj = value_ref.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert!(obj.contains_key("key1"));
        assert!(obj.contains_key("key2"));

        let val1 = obj.get("key1").unwrap();
        assert_eq!(val1.as_ref(), &json!("value1"));

        // Test keys iterator
        let keys: Vec<&str> = obj.keys().collect();
        assert!(keys.contains(&"key1"));
        assert!(keys.contains(&"key2"));

        // Test iter
        let items: Vec<_> = obj.iter().collect();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_value_ref_as_array() {
        let value = json!(["item1", "item2", "item3"]);
        let value_ref = ValueRef::new(value);

        let arr = value_ref.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert!(!arr.is_empty());

        let item1 = arr.get(0).unwrap();
        assert_eq!(item1.as_ref(), &json!("item1"));

        let item2 = arr.get(1).unwrap();
        assert_eq!(item2.as_ref(), &json!("item2"));

        // Test out of bounds
        assert!(arr.get(10).is_none());

        // Test iterator
        let items: Vec<_> = arr.iter().collect();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].as_ref(), &json!("item1"));
        assert_eq!(items[1].as_ref(), &json!("item2"));
        assert_eq!(items[2].as_ref(), &json!("item3"));
    }

    #[test]
    fn test_value_ref_is_truthy() {
        assert!(ValueRef::new(json!(true)).is_truthy());
        assert!(!ValueRef::new(json!(false)).is_truthy());

        assert!(ValueRef::new(json!(1)).is_truthy());
        assert!(ValueRef::new(json!(-1)).is_truthy());
        assert!(ValueRef::new(json!(1.5)).is_truthy());
        assert!(!ValueRef::new(json!(0)).is_truthy());
        assert!(!ValueRef::new(json!(0.0)).is_truthy());

        assert!(ValueRef::new(json!("hello")).is_truthy());
        assert!(!ValueRef::new(json!("")).is_truthy());

        assert!(ValueRef::new(json!(null)).is_truthy());
        assert!(ValueRef::new(json!({})).is_truthy());
        assert!(ValueRef::new(json!([])).is_truthy());
    }

    #[test]
    fn test_value_ref_deserialize() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct TestStruct {
            name: String,
            age: u32,
        }

        let data = TestStruct {
            name: "Alice".to_string(),
            age: 30,
        };
        let value = serde_json::to_value(&data).unwrap();
        let value_ref = ValueRef::new(value);

        let deserialized: TestStruct = value_ref.deserialize().unwrap();
        assert_eq!(deserialized, data);

        // Test deserialization error
        let invalid_value = ValueRef::new(json!("not a struct"));
        let result: Result<TestStruct, _> = invalid_value.deserialize();
        assert!(result.is_err());
    }

    #[test]
    fn test_value_ref_type_accessors() {
        // Test as_bool
        assert_eq!(ValueRef::new(json!(true)).as_bool(), Some(true));
        assert_eq!(ValueRef::new(json!(false)).as_bool(), Some(false));
        assert_eq!(ValueRef::new(json!("hello")).as_bool(), None);
        assert_eq!(ValueRef::new(json!(42)).as_bool(), None);

        // Test as_str
        assert_eq!(ValueRef::new(json!("hello")).as_str(), Some("hello"));
        assert_eq!(ValueRef::new(json!("")).as_str(), Some(""));
        assert_eq!(ValueRef::new(json!(42)).as_str(), None);
        assert_eq!(ValueRef::new(json!(true)).as_str(), None);

        // Test as_number
        let num_value = ValueRef::new(json!(42));
        assert!(num_value.as_number().is_some());
        assert_eq!(num_value.as_number().unwrap().as_u64(), Some(42));

        let float_value = ValueRef::new(json!(PI));
        assert!(float_value.as_number().is_some());
        assert_eq!(float_value.as_number().unwrap().as_f64(), Some(PI));

        assert_eq!(ValueRef::new(json!("hello")).as_number(), None);
        assert_eq!(ValueRef::new(json!(true)).as_number(), None);

        // Test is_null
        assert!(ValueRef::new(json!(null)).is_null());
        assert!(!ValueRef::new(json!(0)).is_null());
        assert!(!ValueRef::new(json!("")).is_null());
        assert!(!ValueRef::new(json!(false)).is_null());
    }

    // Helper function to assert that two ValueRefs share the same underlying Arc
    fn assert_same_base<T1, T2>(base: &ValueRef<T1>, derived: &ValueRef<T2>)
    where
        T1: 'static,
        T2: 'static,
    {
        assert!(
            Arc::ptr_eq(base.0.as_owner(), derived.0.as_owner()),
            "ValueRefs should share the same underlying Arc (no cloning)"
        );
    }

    #[test]
    fn test_path_access_no_cloning() {
        let value = json!({
            "data": {
                "nested": {
                    "deep": "value"
                }
            }
        });

        let root = ValueRef::new(value);
        let data = root.path("data").unwrap();
        let nested = data.path("nested").unwrap();
        let deep = nested.path("deep").unwrap();

        // Verify all path access shares the same root Arc
        assert_same_base(&root, &data);
        assert_same_base(&root, &nested);
        assert_same_base(&root, &deep);

        // Verify the value is correct
        assert_eq!(deep.as_ref(), &json!("value"));
    }

    #[test]
    fn test_array_index_access_no_cloning() {
        let value = json!([
            {"name": "first"},
            {"name": "second"},
            [1, 2, 3]
        ]);

        let root = ValueRef::new(value);
        let first_item = root.index(0).unwrap();
        let second_item = root.index(1).unwrap();
        let third_item = root.index(2).unwrap();
        let nested_array_item = third_item.index(1).unwrap();

        // Verify all array path access shares the same root Arc
        assert_same_base(&root, &first_item);
        assert_same_base(&root, &second_item);
        assert_same_base(&root, &third_item);
        assert_same_base(&root, &nested_array_item);

        // Verify the values are correct
        assert_eq!(first_item.as_ref(), &json!({"name": "first"}));
        assert_eq!(nested_array_item.as_ref(), &json!(2));
    }

    #[test]
    fn test_as_object_access_no_cloning() {
        let value = json!({
            "metadata": {
                "count": 42,
                "items": ["a", "b", "c"]
            }
        });

        let root = ValueRef::new(value);
        let metadata = root.path("metadata").unwrap();
        let metadata_obj = metadata.as_object().unwrap();

        // Access fields through the object interface
        let count = metadata_obj.get("count").unwrap();
        let items = metadata_obj.get("items").unwrap();

        // Verify object casting and field access shares the same root Arc
        assert_same_base(&root, &metadata);
        assert_same_base(&root, &metadata_obj);
        assert_same_base(&root, &count);
        assert_same_base(&root, &items);

        // Verify the values are correct
        assert_eq!(count.as_ref(), &json!(42));
        assert_eq!(items.as_ref(), &json!(["a", "b", "c"]));
    }

    #[test]
    fn test_as_array_access_no_cloning() {
        let value = json!({
            "items": [
                {"id": 1, "name": "first"},
                {"id": 2, "name": "second"},
                {"id": 3, "name": "third"}
            ]
        });

        let root = ValueRef::new(value);
        let items = root.path("items").unwrap();
        let items_array = items.as_array().unwrap();

        // Access items through the array interface
        let first_item = items_array.get(0).unwrap();
        let second_item = items_array.get(1).unwrap();
        let third_item = items_array.get(2).unwrap();

        // Access nested field in array item
        let first_name = first_item.path("name").unwrap();

        // Verify array casting and element access shares the same root Arc
        assert_same_base(&root, &items);
        assert_same_base(&root, &items_array);
        assert_same_base(&root, &first_item);
        assert_same_base(&root, &second_item);
        assert_same_base(&root, &third_item);
        assert_same_base(&root, &first_name);

        // Verify the values are correct
        assert_eq!(first_item.as_ref(), &json!({"id": 1, "name": "first"}));
        assert_eq!(first_name.as_ref(), &json!("first"));
    }

    #[test]
    fn test_object_iter_no_cloning() {
        let value = json!({
            "key1": "value1",
            "key2": {"nested": "value2"},
            "key3": [1, 2, 3]
        });

        let root = ValueRef::new(value);
        let obj = root.as_object().unwrap();

        // Verify object casting shares the same root Arc
        assert_same_base(&root, &obj);

        // Iterate over key-value pairs and verify each shares the same root
        let items = obj.iter();
        for (_key, value_ref) in items {
            assert_same_base(&root, &value_ref);
        }
    }

    #[test]
    fn test_array_iter_no_cloning() {
        let value = json!([
            {"type": "first"},
            {"type": "second"},
            [1, 2, {"nested": "deep"}]
        ]);

        let root = ValueRef::new(value);
        let arr = root.as_array().unwrap();

        // Verify array casting shares the same root Arc
        assert_same_base(&root, &arr);

        // Iterate over array elements and verify each shares the same root
        let items: Vec<_> = arr.iter().collect();
        for item in &items {
            assert_same_base(&root, item);
        }

        // Access nested content from iterated items
        let nested_array = &items[2];
        let nested_arr = nested_array.as_array().unwrap();
        let deep_object = nested_arr.get(2).unwrap();
        let deep_value = deep_object.path("nested").unwrap();

        // Verify all nested access still shares the same root
        assert_same_base(&root, &nested_arr);
        assert_same_base(&root, &deep_object);
        assert_same_base(&root, &deep_value);

        // Verify the deep value is correct
        assert_eq!(deep_value.as_ref(), &json!("deep"));
    }
}
