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
/// Provides well-known error code constants organized by range. Error code fields
/// (e.g. `FlowError.code`, protocol `Error.code`) accept any integer — these
/// constants are convenience values, not an exhaustive constraint.
///
/// ## Ranges
///
/// | Range | Category |
/// |-------|----------|
/// | -32700 to -32600 | JSON-RPC standard errors |
/// | -32099 to -32000 | Stepflow protocol errors |
/// | 1 to 4999 | Orchestrator and component errors |
/// | 5000+ | Transport / infrastructure errors |
///
/// **Retry rule:** codes >= 5000 are transport errors (retried up to
/// `retry.transportMaxRetries`). All other positive codes are component errors
/// (retried only with `onError: { action: retry }`).
pub struct ErrorCode;

impl ErrorCode {
    // ── JSON-RPC Standard Errors ─────────────────────────────────

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

    // ── Stepflow Protocol Errors ─────────────────────────────────

    /// Generic server error.
    pub const SERVER_ERROR: i64 = -32000;
    /// Requested component does not exist.
    pub const COMPONENT_NOT_FOUND: i64 = -32001;
    /// Server has not been initialized.
    pub const SERVER_NOT_INITIALIZED: i64 = -32002;
    /// Input does not match component schema.
    pub const INVALID_INPUT_SCHEMA: i64 = -32003;
    /// Component failed during execution.
    pub const COMPONENT_EXECUTION_FAILED: i64 = -32004;
    /// Required resource is not available.
    pub const RESOURCE_UNAVAILABLE: i64 = -32005;
    /// Operation timed out.
    pub const TIMEOUT: i64 = -32006;
    /// Insufficient permissions.
    pub const PERMISSION_DENIED: i64 = -32007;
    /// Requested blob does not exist.
    pub const BLOB_NOT_FOUND: i64 = -32008;
    /// Flow expression could not be evaluated.
    pub const EXPRESSION_EVAL_FAILED: i64 = -32009;
    /// HTTP session has expired.
    pub const SESSION_EXPIRED: i64 = -32010;
    /// Invalid value for a field.
    pub const INVALID_VALUE: i64 = -32011;
    /// Referenced value is not found.
    pub const NOT_FOUND: i64 = -32012;

    // ── Orchestrator / Component Errors (1–4999) ─────────────────

    /// A referenced field does not exist in the step output, input, or variable.
    pub const UNDEFINED_FIELD: i64 = 1;
    /// Invalid input or structure.
    pub const BAD_REQUEST: i64 = 400;
    /// A referenced entity was not found (e.g., unknown step name).
    pub const ENTITY_NOT_FOUND: i64 = 404;
    /// The component ran and returned an error, or an internal error occurred.
    /// Default code for component-reported failures.
    pub const INTERNAL_ERROR: i64 = 500;

    // ── Transport Errors (5000+) ─────────────────────────────────

    /// Infrastructure failure — subprocess crash, network timeout, connection refused.
    /// The component never ran or didn't complete.
    pub const TRANSPORT_ERROR: i64 = 5000;

    /// Returns true if the given code represents a transport/infrastructure error.
    pub const fn is_transport(code: i64) -> bool {
        code >= 5000
    }
}

impl schemars::JsonSchema for ErrorCode {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ErrorCode".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Well-known error codes for the Stepflow platform.\n\nRanges:\n  -32700 to -32600: JSON-RPC standard errors\n  -32099 to -32000: Stepflow protocol errors\n  1 to 4999: Orchestrator and component errors\n  5000+: Transport / infrastructure errors\n\nRetry rule: codes >= 5000 are transport errors (retried up to transportMaxRetries). All other positive codes are component errors (retried only with onError: { action: retry }).",
            "type": "integer",
            "enum": [
                -32700, -32600, -32601, -32602, -32603,
                -32000, -32001, -32002, -32003, -32004, -32005,
                -32006, -32007, -32008, -32009, -32010, -32011, -32012,
                1, 400, 404, 500,
                5000
            ],
            "x-enum-varnames": [
                "PARSE_ERROR", "INVALID_REQUEST", "METHOD_NOT_FOUND", "INVALID_PARAMS", "JSON_RPC_INTERNAL_ERROR",
                "SERVER_ERROR", "COMPONENT_NOT_FOUND", "SERVER_NOT_INITIALIZED", "INVALID_INPUT_SCHEMA",
                "COMPONENT_EXECUTION_FAILED", "RESOURCE_UNAVAILABLE", "TIMEOUT", "PERMISSION_DENIED",
                "BLOB_NOT_FOUND", "EXPRESSION_EVAL_FAILED", "SESSION_EXPIRED", "INVALID_VALUE", "NOT_FOUND",
                "UNDEFINED_FIELD", "BAD_REQUEST", "ENTITY_NOT_FOUND", "INTERNAL_ERROR",
                "TRANSPORT_ERROR"
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transport() {
        assert!(ErrorCode::is_transport(5000));
        assert!(ErrorCode::is_transport(5001));
        assert!(ErrorCode::is_transport(9999));
        assert!(!ErrorCode::is_transport(500));
        assert!(!ErrorCode::is_transport(503));
        assert!(!ErrorCode::is_transport(0));
        assert!(!ErrorCode::is_transport(-32000));
    }

    #[test]
    fn test_error_code_constants_match_ranges() {
        // JSON-RPC standard errors: -32700 to -32600
        assert!(ErrorCode::PARSE_ERROR <= -32600);
        assert!(ErrorCode::INVALID_REQUEST >= -32700 && ErrorCode::INVALID_REQUEST <= -32600);

        // Stepflow protocol errors: -32099 to -32000
        assert!(ErrorCode::SERVER_ERROR >= -32099 && ErrorCode::SERVER_ERROR <= -32000);
        assert!(ErrorCode::NOT_FOUND >= -32099 && ErrorCode::NOT_FOUND <= -32000);

        // Orchestrator/component errors: 1 to 4999
        assert!(ErrorCode::UNDEFINED_FIELD >= 1 && ErrorCode::UNDEFINED_FIELD < 5000);
        assert!(ErrorCode::INTERNAL_ERROR >= 1 && ErrorCode::INTERNAL_ERROR < 5000);

        // Transport errors: 5000+
        assert!(ErrorCode::TRANSPORT_ERROR >= 5000);
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
    }
}
