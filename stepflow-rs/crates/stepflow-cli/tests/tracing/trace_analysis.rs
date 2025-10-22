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

#![allow(clippy::print_stderr)]

//! Trace analysis utilities for test verification

use super::otlp_types::{OtlpTrace, Span};
use std::path::Path;

/// Read and parse OTLP traces from a JSONL file
pub fn read_traces(trace_file: &Path) -> Vec<OtlpTrace> {
    if !trace_file.exists() {
        return vec![];
    }

    let content = std::fs::read_to_string(trace_file)
        .unwrap_or_else(|e| panic!("Failed to read trace file {}: {}", trace_file.display(), e));

    content
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            serde_json::from_str(line).unwrap_or_else(|e| {
                panic!("Failed to parse trace line: {}\nLine: {}", e, line);
            })
        })
        .collect()
}

/// Find all spans with the given name across all traces
pub fn find_spans_by_name<'a>(traces: &'a [OtlpTrace], name: &str) -> Vec<&'a Span> {
    traces
        .iter()
        .flat_map(|trace| trace.all_spans())
        .filter(|span| span.name == name)
        .collect()
}

/// Find the root span for a given trace ID
pub fn find_root_span<'a>(traces: &'a [OtlpTrace], trace_id: &str) -> Option<&'a Span> {
    traces
        .iter()
        .flat_map(|trace| trace.all_spans())
        .find(|span| span.belongs_to_trace(trace_id) && span.is_root())
}

/// Find all child spans of a given parent span ID
pub fn find_child_spans<'a>(traces: &'a [OtlpTrace], parent_span_id: &str) -> Vec<&'a Span> {
    traces
        .iter()
        .flat_map(|trace| trace.all_spans())
        .filter(|span| span.has_parent(parent_span_id))
        .collect()
}

/// Filter traces to only include spans from a specific trace (run)
pub fn filter_traces_by_trace_id<'a>(traces: &'a [OtlpTrace], trace_id: &str) -> Vec<&'a Span> {
    traces
        .iter()
        .flat_map(|trace| trace.all_spans())
        .filter(|span| span.belongs_to_trace(trace_id))
        .collect()
}

/// Count total spans in a trace
pub fn count_spans_in_trace(traces: &[OtlpTrace], trace_id: &str) -> usize {
    filter_traces_by_trace_id(traces, trace_id).len()
}

/// Helper to print a trace tree for debugging
pub fn print_trace_tree(traces: &[OtlpTrace], trace_id: &str) {
    let root = find_root_span(traces, trace_id);
    if root.is_none() {
        eprintln!("No root span found for trace {}", trace_id);
        return;
    }

    let root = root.unwrap();
    eprintln!("Trace tree for {}:", trace_id);
    print_span_tree(traces, root, 0);
}

fn print_span_tree(traces: &[OtlpTrace], span: &Span, indent: usize) {
    let prefix = "  ".repeat(indent);
    eprintln!("{}{}", prefix, span);

    let children = find_child_spans(traces, &span.span_id());
    for child in children {
        print_span_tree(traces, child, indent + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracing::otlp_types::*;

    fn create_test_trace() -> Vec<OtlpTrace> {
        // Create a simple trace hierarchy:
        // root (trace1, span1)
        //   ├─ child1 (trace1, span2, parent=span1)
        //   └─ child2 (trace1, span3, parent=span1)
        //       └─ grandchild (trace1, span4, parent=span3)
        vec![OtlpTrace {
            resource_spans: vec![ResourceSpans {
                scope_spans: vec![ScopeSpans {
                    spans: vec![
                        Span {
                            trace_id: "trace1".to_string(),
                            span_id: "span1".to_string(),
                            parent_span_id: "".to_string(),
                            name: "root".to_string(),
                            kind: 0,
                        },
                        Span {
                            trace_id: "trace1".to_string(),
                            span_id: "span2".to_string(),
                            parent_span_id: "span1".to_string(),
                            name: "child1".to_string(),
                            kind: 0,
                        },
                        Span {
                            trace_id: "trace1".to_string(),
                            span_id: "span3".to_string(),
                            parent_span_id: "span1".to_string(),
                            name: "child2".to_string(),
                            kind: 0,
                        },
                        Span {
                            trace_id: "trace1".to_string(),
                            span_id: "span4".to_string(),
                            parent_span_id: "span3".to_string(),
                            name: "grandchild".to_string(),
                            kind: 0,
                        },
                    ],
                }],
            }],
        }]
    }

    #[test]
    fn test_find_spans_by_name() {
        let traces = create_test_trace();
        let roots = find_spans_by_name(&traces, "root");
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].name, "root");

        let children = find_spans_by_name(&traces, "child1");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "child1");
    }

    #[test]
    fn test_find_root_span() {
        let traces = create_test_trace();
        let root = find_root_span(&traces, "trace1");
        assert!(root.is_some());
        assert_eq!(root.unwrap().name, "root");
        assert!(root.unwrap().is_root());
    }

    #[test]
    fn test_find_child_spans() {
        let traces = create_test_trace();
        let children = find_child_spans(&traces, "span1");
        assert_eq!(children.len(), 2);
        assert!(children.iter().any(|s| s.name == "child1"));
        assert!(children.iter().any(|s| s.name == "child2"));

        let grandchildren = find_child_spans(&traces, "span3");
        assert_eq!(grandchildren.len(), 1);
        assert_eq!(grandchildren[0].name, "grandchild");
    }

    #[test]
    fn test_count_spans() {
        let traces = create_test_trace();
        assert_eq!(count_spans_in_trace(&traces, "trace1"), 4);
    }
}
