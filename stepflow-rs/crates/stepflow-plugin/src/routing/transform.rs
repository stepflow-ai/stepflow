// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use error_stack::{Result, ResultExt};
use globset::Glob;
use stepflow_core::workflow::Component;
use crate::error::PluginError;
use crate::routing::rules::RoutingTarget;

/// Component filter for include/exclude pattern matching
#[derive(Debug, Clone)]
pub struct ComponentFilter {
    include_patterns: Vec<Glob>,
    exclude_patterns: Vec<Glob>,
}

impl ComponentFilter {
    /// Create a new component filter from a routing target
    pub fn from_target(target: &RoutingTarget) -> Result<Self, PluginError> {
        let mut include_patterns = Vec::new();
        let mut exclude_patterns = Vec::new();
        
        // Parse include patterns
        if let Some(components) = &target.components {
            for pattern_str in components {
                let pattern = Glob::new(pattern_str)
                    .change_context(PluginError::Configuration)?;
                include_patterns.push(pattern);
            }
        }
        
        // Parse exclude patterns
        if let Some(exclude_components) = &target.exclude_components {
            for pattern_str in exclude_components {
                let pattern = Glob::new(pattern_str)
                    .change_context(PluginError::Configuration)?;
                exclude_patterns.push(pattern);
            }
        }
        
        Ok(ComponentFilter {
            include_patterns,
            exclude_patterns,
        })
    }
    
    /// Create a filter that matches all components
    pub fn allow_all() -> Self {
        ComponentFilter {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
        }
    }
    
    /// Check if a component name matches this filter
    pub fn matches(&self, component_name: &str) -> bool {
        // If include patterns are specified, component must match at least one
        if !self.include_patterns.is_empty() {
            let included = self.include_patterns.iter().any(|p| p.compile_matcher().is_match(component_name));
            if !included {
                return false;
            }
        }
        
        // Component must not match any exclude patterns
        !self.exclude_patterns.iter().any(|p| p.compile_matcher().is_match(component_name))
    }
    
    /// Check if a component matches this filter
    pub fn matches_component(&self, component: &Component) -> bool {
        // For filtering, we want to match against the component name part
        // For builtin components, use the builtin name
        // For path-based components, the component name is the final path segment
        let component_name = if component.is_builtin() {
            component.builtin_name().unwrap_or("")
        } else {
            // Extract component name from path like "/udf" -> "udf" or "/nested/path" -> "path"
            let path = component.path_string();
            let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if let Some(last_part) = parts.last() {
                last_part
            } else {
                ""
            }
        };
        self.matches(component_name)
    }
}

/// Path transformer for bidirectional path transformation
#[derive(Debug, Clone)]
pub struct PathTransformer {
    strip_segments: Vec<String>,
}

impl PathTransformer {
    /// Create a new path transformer
    pub fn new(strip_segments: Vec<String>) -> Self {
        PathTransformer { strip_segments }
    }
    
    /// Create a path transformer from a routing target
    pub fn from_target(target: &RoutingTarget) -> Self {
        PathTransformer::new(target.strip_segments.clone())
    }
    
    /// Transform a path from routing format to plugin format (forward transformation)
    /// Example: "/python/udf" with strip_segments ["python"] -> "/udf"
    pub fn transform_to_plugin(&self, original_path: &str) -> Result<String, PluginError> {
        let mut segments = original_path.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>();
        
        // Validate that the path starts with the expected segments
        if segments.len() < self.strip_segments.len() {
            return Err(error_stack::report!(PluginError::Configuration)
                .attach_printable(format!(
                    "Path '{}' has fewer segments than expected strip_segments {:?}",
                    original_path, self.strip_segments
                )));
        }
        
        // Validate each segment matches
        for (i, expected) in self.strip_segments.iter().enumerate() {
            if segments.get(i) != Some(&expected.as_str()) {
                return Err(error_stack::report!(PluginError::Configuration)
                    .attach_printable(format!(
                        "Path '{}' segment {} is '{}' but expected '{}' based on strip_segments {:?}",
                        original_path, i, segments.get(i).map_or("", |v| v), expected, self.strip_segments
                    )));
            }
        }
        
        // Remove the stripped segments
        segments.drain(0..self.strip_segments.len());
        
        // Return the transformed path
        if segments.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", segments.join("/")))
        }
    }
    
    /// Transform a path from plugin format to routing format (reverse transformation)
    /// Example: "/udf" with strip_segments ["python"] -> "/python/udf"
    pub fn transform_from_plugin(&self, plugin_path: &str) -> String {
        let plugin_segments = plugin_path.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>();
        let mut full_path = self.strip_segments.clone();
        full_path.extend(plugin_segments.iter().map(|s| s.to_string()));
        
        if full_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", full_path.join("/"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::rules::RoutingTarget;

    #[test]
    fn test_component_filter_allow_all() {
        let filter = ComponentFilter::allow_all();
        assert!(filter.matches("udf"));
        assert!(filter.matches("analyzer"));
        assert!(filter.matches("debug_tool"));
    }

    #[test]
    fn test_component_filter_include_patterns() {
        let target = RoutingTarget {
            plugin: "python".to_string(),
            strip_segments: vec!["python".to_string()],
            components: Some(vec!["udf".to_string(), "analyzer".to_string()]),
            exclude_components: None,
        };
        
        let filter = ComponentFilter::from_target(&target).unwrap();
        assert!(filter.matches("udf"));
        assert!(filter.matches("analyzer"));
        assert!(!filter.matches("debug_tool"));
    }

    #[test]
    fn test_component_filter_exclude_patterns() {
        let target = RoutingTarget {
            plugin: "python".to_string(),
            strip_segments: vec!["python".to_string()],
            components: None,
            exclude_components: Some(vec!["debug_*".to_string()]),
        };
        
        let filter = ComponentFilter::from_target(&target).unwrap();
        assert!(filter.matches("udf"));
        assert!(filter.matches("analyzer"));
        assert!(!filter.matches("debug_tool"));
        assert!(!filter.matches("debug_analyzer"));
    }

    #[test]
    fn test_component_filter_include_and_exclude() {
        let target = RoutingTarget {
            plugin: "python".to_string(),
            strip_segments: vec!["python".to_string()],
            components: Some(vec!["ml_*".to_string(), "data_*".to_string()]),
            exclude_components: Some(vec!["*_debug".to_string()]),
        };
        
        let filter = ComponentFilter::from_target(&target).unwrap();
        assert!(filter.matches("ml_classifier"));
        assert!(filter.matches("data_processor"));
        assert!(!filter.matches("ml_classifier_debug"));
        assert!(!filter.matches("data_processor_debug"));
        assert!(!filter.matches("other_tool"));
    }

    #[test]
    fn test_path_transformer_single_segment() {
        let transformer = PathTransformer::new(vec!["python".to_string()]);
        
        // Forward transformation
        assert_eq!(transformer.transform_to_plugin("/python/udf").unwrap(), "/udf");
        assert_eq!(transformer.transform_to_plugin("/python/analyzer").unwrap(), "/analyzer");
        assert_eq!(transformer.transform_to_plugin("/python/deep/nested/path").unwrap(), "/deep/nested/path");
        
        // Reverse transformation
        assert_eq!(transformer.transform_from_plugin("/udf"), "/python/udf");
        assert_eq!(transformer.transform_from_plugin("/analyzer"), "/python/analyzer");
        assert_eq!(transformer.transform_from_plugin("/deep/nested/path"), "/python/deep/nested/path");
    }

    #[test]
    fn test_path_transformer_multiple_segments() {
        let transformer = PathTransformer::new(vec!["ai".to_string(), "python".to_string()]);
        
        // Forward transformation
        assert_eq!(transformer.transform_to_plugin("/ai/python/udf").unwrap(), "/udf");
        assert_eq!(transformer.transform_to_plugin("/ai/python/analyzer").unwrap(), "/analyzer");
        
        // Reverse transformation
        assert_eq!(transformer.transform_from_plugin("/udf"), "/ai/python/udf");
        assert_eq!(transformer.transform_from_plugin("/analyzer"), "/ai/python/analyzer");
    }

    #[test]
    fn test_path_transformer_validation_error() {
        let transformer = PathTransformer::new(vec!["python".to_string()]);
        
        // Wrong prefix should fail
        assert!(transformer.transform_to_plugin("/openai/udf").is_err());
        
        // Path with only plugin name should result in root path
        assert_eq!(transformer.transform_to_plugin("/python").unwrap(), "/");
        
        // Empty path should fail
        assert!(transformer.transform_to_plugin("").is_err());
    }

    #[test]
    fn test_path_transformer_from_target() {
        let target = RoutingTarget {
            plugin: "python".to_string(),
            strip_segments: vec!["ai".to_string(), "python".to_string()],
            components: None,
            exclude_components: None,
        };
        
        let transformer = PathTransformer::from_target(&target);
        assert_eq!(transformer.transform_to_plugin("/ai/python/udf").unwrap(), "/udf");
        assert_eq!(transformer.transform_from_plugin("/udf"), "/ai/python/udf");
    }

    #[test]
    fn test_path_transformer_root_path() {
        let transformer = PathTransformer::new(vec!["python".to_string()]);
        
        // When plugin returns root path, should still work
        assert_eq!(transformer.transform_from_plugin("/"), "/python");
        
        // When stripping results in empty path, should return root
        assert_eq!(transformer.transform_to_plugin("/python").unwrap(), "/");
    }

    #[test]
    fn test_component_filter_with_component_struct() {
        let target = RoutingTarget {
            plugin: "python".to_string(),
            strip_segments: vec!["python".to_string()],
            components: Some(vec!["udf".to_string()]),
            exclude_components: None,
        };
        
        let filter = ComponentFilter::from_target(&target).unwrap();
        
        // Test with plugin-stripped paths (what the plugin would return)
        let component_udf = Component::from_string("/udf");
        let component_analyzer = Component::from_string("/analyzer");
        
        assert!(filter.matches_component(&component_udf));
        assert!(!filter.matches_component(&component_analyzer));
    }
}