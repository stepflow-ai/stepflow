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
pub fn proto_to_result_order(value: i32) -> stepflow_dtos::ResultOrder {
    match proto::ResultOrder::try_from(value) {
        Ok(proto::ResultOrder::ByCompletion) => stepflow_dtos::ResultOrder::ByCompletion,
        _ => stepflow_dtos::ResultOrder::ByIndex,
    }
}

/// Convert a domain `ItemStatistics` to proto.
pub fn item_stats_to_proto(s: &stepflow_dtos::ItemStatistics) -> proto::ItemStatistics {
    proto::ItemStatistics {
        total: s.total as u32,
        completed: s.completed as u32,
        running: s.running as u32,
        failed: s.failed as u32,
        cancelled: s.cancelled as u32,
    }
}

/// Convert a domain `ItemResult` to proto.
pub fn item_result_to_proto(r: &stepflow_dtos::ItemResult) -> proto::ItemResult {
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
            Some(error.code as i32),
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
