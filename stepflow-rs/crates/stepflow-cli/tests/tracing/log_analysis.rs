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

//! Log analysis utilities for test verification

use super::otlp_log_types::{LogRecord, OtlpLogs};
use std::path::Path;

/// Read and parse OTLP logs from a JSONL file
pub fn read_logs(log_file: &Path) -> Vec<OtlpLogs> {
    if !log_file.exists() {
        return vec![];
    }

    let content = std::fs::read_to_string(log_file)
        .unwrap_or_else(|e| panic!("Failed to read log file {}: {}", log_file.display(), e));

    content
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            serde_json::from_str(line).unwrap_or_else(|e| {
                panic!("Failed to parse log line: {}\nLine: {}", e, line);
            })
        })
        .collect()
}

/// Find all log records for a specific trace
pub fn find_logs_by_trace<'a>(logs: &'a [OtlpLogs], trace_id: &str) -> Vec<&'a LogRecord> {
    logs.iter()
        .flat_map(|log| log.all_log_records())
        .filter(|record| record.belongs_to_trace(trace_id))
        .collect()
}

/// Find log records by severity level
pub fn find_logs_by_severity<'a>(
    logs: &'a [OtlpLogs],
    severity: &str,
) -> Vec<&'a LogRecord> {
    logs.iter()
        .flat_map(|log| log.all_log_records())
        .filter(|record| record.severity_text == severity)
        .collect()
}

/// Count total log records
pub fn count_log_records(logs: &[OtlpLogs]) -> usize {
    logs.iter()
        .flat_map(|log| log.all_log_records())
        .count()
}

/// Check if logs have required diagnostic context fields
pub fn verify_log_context(log_record: &LogRecord) -> bool {
    // Check if trace_id and span_id are present
    !log_record.trace_id.is_empty() && !log_record.span_id.is_empty()
}

/// Print log records for debugging
pub fn print_logs(logs: &[OtlpLogs], trace_id: &str) {
    let trace_logs = find_logs_by_trace(logs, trace_id);
    println!("📋 Log records for trace {}:", trace_id);
    for log in trace_logs {
        println!(
            "  [{:5}] {} (trace: {:.8}, span: {:.8})",
            log.severity_text,
            log.message(),
            log.trace_id(),
            log.span_id()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_parsing() {
        // This is a minimal test - actual log parsing is tested in integration tests
        let logs: Vec<OtlpLogs> = vec![];
        assert_eq!(count_log_records(&logs), 0);
    }
}
