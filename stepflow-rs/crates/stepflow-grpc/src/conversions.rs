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

//! Shared conversion helpers between domain types and proto types.

use crate::proto::stepflow::v1 as proto;

/// Convert a `chrono::DateTime<Utc>` to a `prost_wkt_types::Timestamp`.
pub fn chrono_to_timestamp(dt: chrono::DateTime<chrono::Utc>) -> prost_wkt_types::Timestamp {
    prost_wkt_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Convert a domain `ExecutionStatus` to the proto enum value (i32).
pub fn execution_status_to_proto(status: stepflow_core::status::ExecutionStatus) -> i32 {
    match status {
        stepflow_core::status::ExecutionStatus::Running => proto::ExecutionStatus::Running as i32,
        stepflow_core::status::ExecutionStatus::Completed => {
            proto::ExecutionStatus::Completed as i32
        }
        stepflow_core::status::ExecutionStatus::Failed => proto::ExecutionStatus::Failed as i32,
        stepflow_core::status::ExecutionStatus::Cancelled => {
            proto::ExecutionStatus::Cancelled as i32
        }
        stepflow_core::status::ExecutionStatus::Paused => proto::ExecutionStatus::Paused as i32,
        stepflow_core::status::ExecutionStatus::RecoveryFailed => {
            proto::ExecutionStatus::RecoveryFailed as i32
        }
    }
}

/// Convert a proto `ExecutionStatus` i32 to a domain `ExecutionStatus`.
///
/// Returns `None` for unrecognized or unspecified values.
pub fn proto_to_execution_status(value: i32) -> Option<stepflow_core::status::ExecutionStatus> {
    match proto::ExecutionStatus::try_from(value) {
        Ok(proto::ExecutionStatus::Running) => {
            Some(stepflow_core::status::ExecutionStatus::Running)
        }
        Ok(proto::ExecutionStatus::Completed) => {
            Some(stepflow_core::status::ExecutionStatus::Completed)
        }
        Ok(proto::ExecutionStatus::Failed) => Some(stepflow_core::status::ExecutionStatus::Failed),
        Ok(proto::ExecutionStatus::Cancelled) => {
            Some(stepflow_core::status::ExecutionStatus::Cancelled)
        }
        Ok(proto::ExecutionStatus::Paused) => Some(stepflow_core::status::ExecutionStatus::Paused),
        Ok(proto::ExecutionStatus::RecoveryFailed) => {
            Some(stepflow_core::status::ExecutionStatus::RecoveryFailed)
        }
        Ok(proto::ExecutionStatus::Unspecified) | Err(_) => None,
    }
}

/// Convert a domain `StepStatus` to the proto `ExecutionStatus` enum value.
///
/// StepStatus has finer granularity (Blocked, Runnable, Skipped) that
/// doesn't exist in the proto enum. We map them to the closest match.
pub fn step_status_to_proto(status: stepflow_core::status::StepStatus) -> i32 {
    match status {
        stepflow_core::status::StepStatus::Running => proto::ExecutionStatus::Running as i32,
        stepflow_core::status::StepStatus::Completed
        | stepflow_core::status::StepStatus::Skipped => proto::ExecutionStatus::Completed as i32,
        stepflow_core::status::StepStatus::Failed => proto::ExecutionStatus::Failed as i32,
        // Blocked and Runnable are pre-execution states; map to UNSPECIFIED
        // since the proto API shouldn't normally expose these.
        stepflow_core::status::StepStatus::Blocked
        | stepflow_core::status::StepStatus::Runnable => proto::ExecutionStatus::Unspecified as i32,
    }
}

/// Convert a proto `ResultOrder` i32 to a domain `ResultOrder`.
pub fn proto_to_result_order(value: i32) -> stepflow_domain::ResultOrder {
    match proto::ResultOrder::try_from(value) {
        Ok(proto::ResultOrder::ByCompletion) => stepflow_domain::ResultOrder::ByCompletion,
        _ => stepflow_domain::ResultOrder::ByIndex,
    }
}

/// Convert a domain `ItemStatistics` to proto.
pub fn item_stats_to_proto(s: &stepflow_domain::ItemStatistics) -> proto::ItemStatistics {
    proto::ItemStatistics {
        total: s.total as u32,
        completed: s.completed as u32,
        running: s.running as u32,
        failed: s.failed as u32,
        cancelled: s.cancelled as u32,
    }
}

/// Convert a [`prost_wkt_types::Value`] to [`serde_json::Value`], preserving
/// integer types for whole-number floats.
///
/// Protobuf `Value.number_value` is always f64. A naïve `serde_json::to_value`
/// round-trip turns `42` into `42.0`. This function recovers integer
/// representation when the float has no fractional part, matching the
/// behaviour of JSON-based transports.
pub fn proto_value_to_json(value: &prost_wkt_types::Value) -> serde_json::Value {
    use prost_wkt_types::value::Kind;
    match &value.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::NumberValue(n)) => {
            let n = *n;
            if n.is_finite() && n.fract() == 0.0 {
                let i = n as i64;
                if i as f64 == n {
                    return serde_json::Value::Number(i.into());
                }
            }
            serde_json::Number::from_f64(n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::StructValue(s)) => {
            // Sort keys for deterministic serialization. Proto Struct uses
            // HashMap internally, so iteration order is non-deterministic.
            // Sorted keys ensure consistent blob hashes across runs.
            let mut entries: Vec<_> = s
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), proto_value_to_json(v)))
                .collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            let map: serde_json::Map<String, serde_json::Value> = entries.into_iter().collect();
            serde_json::Value::Object(map)
        }
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(proto_value_to_json).collect())
        }
        None => serde_json::Value::Null,
    }
}

/// Convert a [`serde_json::Value`] to [`prost_wkt_types::Value`].
///
/// This is the inverse of [`proto_value_to_json`]. The protobuf `number_value`
/// is always f64, so integer type information is lost during this conversion.
/// Use [`proto_value_to_json`] on the receiving end to recover integers.
pub fn json_to_proto_value(
    value: &serde_json::Value,
) -> Result<prost_wkt_types::Value, serde_json::Error> {
    // serde_json::Value → prost_wkt_types::Value via serde roundtrip.
    let json = serde_json::to_value(value)?;
    serde_json::from_value(json)
}

/// Convert a domain `ItemResult` to proto.
pub fn item_result_to_proto(r: &stepflow_domain::ItemResult) -> proto::ItemResult {
    let (output, error_message, error_code) = match &r.result {
        Some(stepflow_core::FlowResult::Success(v)) => {
            let val: Option<prost_wkt_types::Value> = serde_json::to_value(v)
                .ok()
                .and_then(|j| serde_json::from_value(j).ok());
            (val, None, None)
        }
        Some(stepflow_core::FlowResult::Failed(error)) => (
            None,
            Some(error.message.to_string()),
            Some(error.code.into()),
        ),
        _ => (None, None, None),
    };

    proto::ItemResult {
        item_index: r.item_index as u32,
        status: execution_status_to_proto(r.status),
        output,
        error_message,
        error_code,
        completed_at: r.completed_at.map(chrono_to_timestamp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Regression test for #866: integers must survive JSON→proto→JSON roundtrip.
    #[test]
    fn test_proto_value_roundtrip_preserves_integers() {
        let original = json!({"duration_ms": 10, "count": 42, "name": "test"});

        // JSON → proto (integers become f64)
        let proto = json_to_proto_value(&original).unwrap();

        // proto → JSON (integers should be recovered)
        let recovered = proto_value_to_json(&proto);

        assert_eq!(recovered, original);
        assert!(
            recovered["duration_ms"].is_u64(),
            "Expected integer, got {:?}",
            recovered["duration_ms"]
        );
        assert_eq!(recovered["duration_ms"].as_u64(), Some(10));
        assert_eq!(recovered["count"].as_u64(), Some(42));
    }

    #[test]
    fn test_proto_value_roundtrip_preserves_floats() {
        let original = json!({"temperature": 0.7, "pi": 3.14159});
        let proto = json_to_proto_value(&original).unwrap();
        let recovered = proto_value_to_json(&proto);

        assert!(recovered["temperature"].is_f64());
        assert_eq!(recovered["temperature"].as_f64(), Some(0.7));
        assert!(recovered["pi"].is_f64());
    }

    #[test]
    fn test_proto_value_roundtrip_negative_integers() {
        let original = json!({"value": -5});
        let proto = json_to_proto_value(&original).unwrap();
        let recovered = proto_value_to_json(&proto);

        assert!(
            recovered["value"].is_i64(),
            "Negative integer should be recovered as i64, got {:?}",
            recovered["value"]
        );
        assert_eq!(recovered["value"].as_i64(), Some(-5));
    }

    #[test]
    fn test_proto_value_roundtrip_nested_integers() {
        let original = json!({
            "config": {"max_retries": 3, "timeout_ms": 5000},
            "items": [1, 2, 3]
        });
        let proto = json_to_proto_value(&original).unwrap();
        let recovered = proto_value_to_json(&proto);

        // Nested integers should also be recovered
        assert!(recovered["config"]["max_retries"].is_u64());
        assert_eq!(recovered["config"]["max_retries"].as_u64(), Some(3));
        assert!(recovered["items"][0].is_i64() || recovered["items"][0].is_u64());
    }

    #[test]
    fn test_proto_value_roundtrip_zero() {
        let original = json!({"value": 0});
        let proto = json_to_proto_value(&original).unwrap();
        let recovered = proto_value_to_json(&proto);

        assert!(
            recovered["value"].is_i64() || recovered["value"].is_u64(),
            "Zero should be recovered as integer, got {:?}",
            recovered["value"]
        );
    }

    #[test]
    fn test_proto_value_roundtrip_all_types() {
        let original = json!({
            "null_val": null,
            "bool_val": true,
            "int_val": 42,
            "float_val": 3.14,
            "string_val": "hello",
            "array_val": [1, "two", null],
            "object_val": {"nested": true}
        });
        let proto = json_to_proto_value(&original).unwrap();
        let recovered = proto_value_to_json(&proto);

        assert!(recovered["null_val"].is_null());
        assert_eq!(recovered["bool_val"].as_bool(), Some(true));
        assert!(recovered["int_val"].is_i64() || recovered["int_val"].is_u64());
        assert!(recovered["float_val"].is_f64());
        assert_eq!(recovered["string_val"].as_str(), Some("hello"));
        assert!(recovered["array_val"].is_array());
        assert!(recovered["object_val"].is_object());
    }
}
