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

/// Unified error codes for the Stepflow platform.
///
/// All error codes use JSON-RPC negative ranges, organized into sub-ranges by
/// origin and retryability. Error code fields (e.g. `FlowError.code`, protocol
/// `Error.code`) accept any integer — these constants are convenience values,
/// not an exhaustive constraint.
///
/// ## Ranges
///
/// | Range | Category | Retry Behavior |
/// |-------|----------|----------------|
/// | -32700 to -32600 | JSON-RPC standard | Never |
/// | -32000 to -32010 | Component server (predefined) | Never |
/// | -32011 to -32099 | Component server (user-defined) | Never |
/// | -32100 to -32109 | Component execution (predefined) | With `onError: retry` |
/// | -32110 to -32199 | Component execution (user-defined) | With `onError: retry` |
/// | -32200 to -32209 | Orchestrator (predefined) | Never |
/// | -32210 to -32299 | Orchestrator (reserved) | Never |
/// | -32300 to -32399 | Transport | Always |
///
/// **Retry rule:** transport errors are always retried (up to
/// `retry.transportMaxRetries`). Component execution errors are retried only
/// with `onError: { action: retry }`. All other errors are never retried.
pub struct ErrorCode;

impl ErrorCode {
    // ── JSON-RPC Standard Errors (-32700 to -32600) ──────────────

    /// Invalid JSON was received by the server.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i64 = -32600;
    /// The method does not exist or is not available.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Invalid method parameter(s).
    pub const INVALID_PARAMS: i64 = -32602;
    /// Internal JSON-RPC error.
    pub const JSON_RPC_INTERNAL_ERROR: i64 = -32603;

    // ── Component Server Errors (-32000 to -32099) ───────────────
    //
    // Structural/SDK errors in the component server process itself.
    // Not in user code. Retrying won't fix these.
    // Predefined: -32000 to -32010. User-defined: -32011 to -32099.

    /// Generic server error.
    pub const SERVER_ERROR: i64 = -32000;
    /// Requested component does not exist on the server.
    pub const COMPONENT_NOT_FOUND: i64 = -32001;
    /// Server has not been initialized.
    pub const SERVER_NOT_INITIALIZED: i64 = -32002;
    /// Input does not match the component's declared schema.
    pub const INVALID_INPUT_SCHEMA: i64 = -32003;
    /// Invalid value for a protocol field.
    pub const INVALID_VALUE: i64 = -32004;
    /// Referenced entity not found (flow, blob, etc.).
    pub const NOT_FOUND: i64 = -32005;
    /// Protocol version incompatibility between orchestrator and component server.
    pub const PROTOCOL_VERSION_MISMATCH: i64 = -32006;
    /// Required dependency unavailable (e.g., Python import failure).
    pub const DEPENDENCY_ERROR: i64 = -32007;
    /// Invalid or incompatible configuration.
    pub const CONFIGURATION_ERROR: i64 = -32008;

    // ── Component Execution Errors (-32100 to -32199) ────────────
    //
    // Errors from user component code. The component ran but its
    // logic failed. Retrying may help.
    // Predefined: -32100 to -32109. User-defined: -32110 to -32199.

    /// Unhandled exception in component code.
    pub const COMPONENT_EXECUTION_FAILED: i64 = -32100;
    /// Invalid value in component logic.
    pub const COMPONENT_VALUE_ERROR: i64 = -32101;
    /// Required resource not available during component execution.
    pub const COMPONENT_RESOURCE_UNAVAILABLE: i64 = -32102;
    /// Invalid input structure in component logic.
    pub const COMPONENT_BAD_REQUEST: i64 = -32103;

    // ── Orchestrator Errors (-32200 to -32299) ───────────────────
    //
    // Errors from the orchestrator during expression evaluation or
    // input resolution. Retrying won't fix these.
    // Predefined: -32200 to -32209. Reserved: -32210 to -32299.

    /// A referenced field does not exist in the step output, input, or variable.
    pub const UNDEFINED_FIELD: i64 = -32200;
    /// A referenced entity was not found (e.g., unknown step name).
    pub const ENTITY_NOT_FOUND: i64 = -32201;
    /// Unexpected internal error (default for `FlowError::from_error_stack()`).
    pub const INTERNAL_ERROR: i64 = -32202;

    // ── Transport Errors (-32300 to -32399) ──────────────────────
    //
    // The component never ran or didn't complete. The JSON-RPC
    // communication channel itself failed. Always retried.

    /// Generic transport/infrastructure failure.
    pub const TRANSPORT_ERROR: i64 = -32300;
    /// Subprocess failed to start or crashed on startup.
    pub const TRANSPORT_SPAWN_ERROR: i64 = -32301;
    /// HTTP request/response or SSE stream failure.
    pub const TRANSPORT_CONNECTION_ERROR: i64 = -32302;
    /// Garbled or unparseable JSON-RPC message.
    pub const TRANSPORT_PROTOCOL_ERROR: i64 = -32303;

    /// Returns true if the given code represents a transport/infrastructure error.
    pub const fn is_transport(code: i64) -> bool {
        code >= -32399 && code <= -32300
    }

    /// Returns true if the given code represents a component execution error
    /// (retryable with `onError: { action: retry }`).
    pub const fn is_component_execution(code: i64) -> bool {
        code >= -32199 && code <= -32100
    }
}

impl schemars::JsonSchema for ErrorCode {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ErrorCode".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Well-known error codes for the Stepflow platform.\n\nRanges:\n  -32700 to -32600: JSON-RPC standard errors\n  -32000 to -32099: Component server errors (never retried)\n  -32100 to -32199: Component execution errors (retried with onError: retry)\n  -32200 to -32299: Orchestrator errors (never retried)\n  -32300 to -32399: Transport errors (always retried)\n\nComponent server predefined: -32000 to -32010, user-defined: -32011 to -32099.\nComponent execution predefined: -32100 to -32109, user-defined: -32110 to -32199.",
            "type": "integer",
            "enum": [
                -32700, -32600, -32601, -32602, -32603,
                -32000, -32001, -32002, -32003, -32004, -32005, -32006, -32007, -32008,
                -32100, -32101, -32102, -32103,
                -32200, -32201, -32202,
                -32300, -32301, -32302, -32303
            ],
            "x-enum-varnames": [
                "PARSE_ERROR", "INVALID_REQUEST", "METHOD_NOT_FOUND", "INVALID_PARAMS", "JSON_RPC_INTERNAL_ERROR",
                "SERVER_ERROR", "COMPONENT_NOT_FOUND", "SERVER_NOT_INITIALIZED", "INVALID_INPUT_SCHEMA", "INVALID_VALUE", "NOT_FOUND", "PROTOCOL_VERSION_MISMATCH", "DEPENDENCY_ERROR", "CONFIGURATION_ERROR",
                "COMPONENT_EXECUTION_FAILED", "COMPONENT_VALUE_ERROR", "COMPONENT_RESOURCE_UNAVAILABLE", "COMPONENT_BAD_REQUEST",
                "UNDEFINED_FIELD", "ENTITY_NOT_FOUND", "INTERNAL_ERROR",
                "TRANSPORT_ERROR", "TRANSPORT_SPAWN_ERROR", "TRANSPORT_CONNECTION_ERROR", "TRANSPORT_PROTOCOL_ERROR"
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transport() {
        assert!(ErrorCode::is_transport(-32300));
        assert!(ErrorCode::is_transport(-32301));
        assert!(ErrorCode::is_transport(-32399));
        assert!(!ErrorCode::is_transport(-32400));
        assert!(!ErrorCode::is_transport(-32299));
        assert!(!ErrorCode::is_transport(-32000));
        assert!(!ErrorCode::is_transport(-32100));
        assert!(!ErrorCode::is_transport(0));
    }

    #[test]
    fn test_is_component_execution() {
        assert!(ErrorCode::is_component_execution(-32100));
        assert!(ErrorCode::is_component_execution(-32150));
        assert!(ErrorCode::is_component_execution(-32199));
        assert!(!ErrorCode::is_component_execution(-32200));
        assert!(!ErrorCode::is_component_execution(-32099));
        assert!(!ErrorCode::is_component_execution(-32000));
        assert!(!ErrorCode::is_component_execution(-32300));
        assert!(!ErrorCode::is_component_execution(0));
    }

    #[test]
    fn test_error_code_constants_match_ranges() {
        // JSON-RPC standard errors: -32700 to -32600
        assert!(ErrorCode::PARSE_ERROR <= -32600);
        assert!(ErrorCode::INVALID_REQUEST >= -32700 && ErrorCode::INVALID_REQUEST <= -32600);

        // Component server errors: -32000 to -32099
        assert!(ErrorCode::SERVER_ERROR >= -32099 && ErrorCode::SERVER_ERROR <= -32000);
        assert!(ErrorCode::NOT_FOUND >= -32099 && ErrorCode::NOT_FOUND <= -32000);
        assert!(
            ErrorCode::PROTOCOL_VERSION_MISMATCH >= -32099
                && ErrorCode::PROTOCOL_VERSION_MISMATCH <= -32000
        );
        assert!(ErrorCode::DEPENDENCY_ERROR >= -32099 && ErrorCode::DEPENDENCY_ERROR <= -32000);
        assert!(
            ErrorCode::CONFIGURATION_ERROR >= -32099 && ErrorCode::CONFIGURATION_ERROR <= -32000
        );

        // Component execution errors: -32100 to -32199
        assert!(ErrorCode::is_component_execution(
            ErrorCode::COMPONENT_EXECUTION_FAILED
        ));
        assert!(ErrorCode::is_component_execution(
            ErrorCode::COMPONENT_VALUE_ERROR
        ));
        assert!(ErrorCode::is_component_execution(
            ErrorCode::COMPONENT_RESOURCE_UNAVAILABLE
        ));
        assert!(ErrorCode::is_component_execution(
            ErrorCode::COMPONENT_BAD_REQUEST
        ));

        // Orchestrator errors: -32200 to -32299
        assert!(ErrorCode::UNDEFINED_FIELD >= -32299 && ErrorCode::UNDEFINED_FIELD <= -32200);
        assert!(ErrorCode::ENTITY_NOT_FOUND >= -32299 && ErrorCode::ENTITY_NOT_FOUND <= -32200);
        assert!(ErrorCode::INTERNAL_ERROR >= -32299 && ErrorCode::INTERNAL_ERROR <= -32200);

        // Transport errors: -32300 to -32399
        assert!(ErrorCode::is_transport(ErrorCode::TRANSPORT_ERROR));
        assert!(ErrorCode::is_transport(ErrorCode::TRANSPORT_SPAWN_ERROR));
        assert!(ErrorCode::is_transport(
            ErrorCode::TRANSPORT_CONNECTION_ERROR
        ));
        assert!(ErrorCode::is_transport(ErrorCode::TRANSPORT_PROTOCOL_ERROR));
    }

    #[test]
    fn test_schema_enum_matches_constants() {
        let schema = schemars::schema_for!(ErrorCode);
        let json = serde_json::to_value(&schema).unwrap();

        // schema_for! generates a root schema that *is* the ErrorCode schema
        // (no $defs wrapper when generating for a single type)
        let error_code = &json;
        let enum_values = error_code.get("enum").unwrap().as_array().unwrap();
        let varnames = error_code
            .get("x-enum-varnames")
            .unwrap()
            .as_array()
            .unwrap();

        // enum and x-enum-varnames must have the same length
        assert_eq!(enum_values.len(), varnames.len());

        // All constants must appear in the enum array
        let codes: Vec<i64> = enum_values.iter().map(|v| v.as_i64().unwrap()).collect();
        assert!(codes.contains(&ErrorCode::PARSE_ERROR));
        assert!(codes.contains(&ErrorCode::TRANSPORT_ERROR));
        assert!(codes.contains(&ErrorCode::INTERNAL_ERROR));
        assert!(codes.contains(&ErrorCode::UNDEFINED_FIELD));
        assert!(codes.contains(&ErrorCode::COMPONENT_EXECUTION_FAILED));
        assert!(codes.contains(&ErrorCode::COMPONENT_BAD_REQUEST));
        assert!(codes.contains(&ErrorCode::PROTOCOL_VERSION_MISMATCH));
        assert!(codes.contains(&ErrorCode::DEPENDENCY_ERROR));
        assert!(codes.contains(&ErrorCode::CONFIGURATION_ERROR));
    }
}
