---
sidebar_position: 7
---

# Reference

This document provides the complete specification for StepFlow workflow files, including all fields, formats, and advanced features supported by the workflow engine.

## Complete Workflow Structure

```yaml
# Optional metadata
name: "My Workflow"
description: "A complete workflow demonstrating all features"
version: "1.2.0"

# Input validation schema
input_schema:
  type: object
  properties:
    # Input field definitions
  required: []

# Output validation schema
output_schema:
  type: object
  properties:
    # Output field definitions
  required: []

# Workflow execution steps
steps:
  - id: step_identifier
    component: component://name
    # Step configuration...

# Output mapping
output:
  result_field: { $from: { step: step_name } }

# Test configuration
test:
  stepflow_config: "path/to/test-config.yml"
  cases:
    - name: test_case_name
      # Test case definition...

# Example inputs (for documentation and tooling)
examples:
  - name: example_name
    description: "Description of this example"
    input:
      # Example input data
```

## Metadata Fields

### Core Metadata

```yaml
name: "Workflow Name"
description: "Detailed description of what this workflow does"
version: "1.0.0"
```

**Field Descriptions:**
- **`name`** (optional): Human-readable workflow name for documentation and UIs
- **`description`** (optional): Detailed description of workflow purpose and behavior
- **`version`** (optional): Semantic version for workflow tracking and compatibility

**Usage Guidelines:**
- Use descriptive names that clearly indicate the workflow's purpose
- Include version numbers for workflows that evolve over time
- Write descriptions that help other developers understand the workflow's use case

### Version Management

```yaml
# Version with metadata
version: "2.1.0"

# Complex versioning (future feature)
version_info:
  version: "2.1.0"
  compatibility: ">=2.0.0"
  deprecated_features: ["old_syntax"]
  migration_guide: "docs/migration-v2.md"
```

## Schema Definitions

### Input Schema

Defines the structure and validation rules for workflow input:

```yaml
input_schema:
  type: object
  properties:
    user_data:
      type: object
      properties:
        name:
          type: string
          minLength: 1
          maxLength: 100
        email:
          type: string
          format: email
        age:
          type: integer
          minimum: 0
          maximum: 150
      required: ["name", "email"]

    processing_options:
      type: object
      properties:
        format:
          type: string
          enum: ["json", "xml", "csv"]
          default: "json"
        include_metadata:
          type: boolean
          default: true
      additionalProperties: false

    file_paths:
      type: array
      items:
        type: string
        pattern: "^[a-zA-Z0-9._/-]+$"
      minItems: 1
      maxItems: 10

  required: ["user_data"]
  additionalProperties: false
```

### Output Schema

Defines the structure of workflow output:

```yaml
output_schema:
  type: object
  properties:
    processed_data:
      type: object
      description: "The main processed result"

    metadata:
      type: object
      properties:
        processing_time_ms:
          type: integer
          minimum: 0
        records_processed:
          type: integer
          minimum: 0
        warnings:
          type: array
          items:
            type: string
      required: ["processing_time_ms", "records_processed"]

    status:
      type: string
      enum: ["success", "partial", "failed"]

  required: ["processed_data", "metadata", "status"]
```

### Schema Features

**Supported JSON Schema Features:**
- Type validation (`string`, `number`, `integer`, `boolean`, `array`, `object`, `null`)
- String constraints (`minLength`, `maxLength`, `pattern`, `format`)
- Number constraints (`minimum`, `maximum`, `multipleOf`)
- Array constraints (`minItems`, `maxItems`, `uniqueItems`)
- Object constraints (`required`, `additionalProperties`, `patternProperties`)
- Composition (`allOf`, `anyOf`, `oneOf`, `not`)
- Conditional schemas (`if`/`then`/`else`)
- References (`$ref`)
- Enumerations (`enum`)
- Constants (`const`)

## Step Configuration

### Basic Step Structure

```yaml
steps:
  - id: unique_step_identifier
    component: builtin://component_name
    input:
      param1: { $from: { workflow: input }, path: "field" }
      param2: { $literal: "static_value" }

    # Optional fields
    input_schema:
      type: object
      # Schema for this step's input

    output_schema:
      type: object
      # Schema for this step's output

    skip_if: { $from: { workflow: input }, path: "skip_condition" }

    on_error:
      action: continue
      default_output:
        result: "fallback_value"
```

### Step Schemas

Individual steps can define their own input/output schemas:

```yaml
steps:
  - id: data_processor
    component: custom://processor

    input_schema:
      type: object
      properties:
        data:
          type: array
          items:
            type: object
            properties:
              id: { type: string }
              value: { type: number }
            required: ["id", "value"]
        rules:
          type: object
          additionalProperties:
            type: string
      required: ["data"]

    output_schema:
      type: object
      properties:
        processed_data:
          type: array
          items:
            type: object
        summary:
          type: object
          properties:
            total_records: { type: integer }
            processing_time_ms: { type: integer }
      required: ["processed_data", "summary"]

    input:
      data: { $from: { workflow: input }, path: "raw_data" }
      rules: { $from: { step: load_rules } }
```

**Step Schema Benefits:**
- **Validation**: Ensures data integrity at each step
- **Documentation**: Self-documenting step interfaces
- **Tooling**: Enables IDE autocompletion and validation
- **Debugging**: Clear error messages for data mismatches

## Test Configuration

### Test Structure

```yaml
test:
  # Optional: Override stepflow config for tests
  stepflow_config: "test-config.yml"

  # Test cases
  cases:
    - name: test_case_identifier
      description: "Human-readable description of test case"
      input:
        # Input data matching workflow input_schema
      output:
        outcome: success  # success | skipped | failed
        result:
          # Expected output data matching workflow output_schema

      # Optional: Override specific configuration
      config_overrides:
        timeout: 60
        log_level: debug
```

### Complete Test Example

```yaml
test:
  stepflow_config: "test/test-config.yml"

  cases:
    - name: successful_processing
      description: "Process valid user data successfully"
      input:
        user_data:
          name: "Alice Johnson"
          email: "alice@example.com"
          age: 30
        processing_options:
          format: "json"
          include_metadata: true
        file_paths: ["data/input1.csv", "data/input2.csv"]

      output:
        outcome: success
        result:
          processed_data:
            user_id: "alice_johnson_30"
            formatted_name: "Alice Johnson"
            email_domain: "example.com"
          metadata:
            processing_time_ms: 250
            records_processed: 2
            warnings: []
          status: "success"

    - name: missing_required_field
      description: "Handle missing required input gracefully"
      input:
        user_data:
          name: "Bob Smith"
          # Missing required email field
        processing_options:
          format: "json"
        file_paths: ["data/input1.csv"]

      output:
        outcome: failed
        error:
          code: "VALIDATION_ERROR"
          message: "Required field 'email' missing from user_data"

    - name: empty_file_list
      description: "Handle empty file list"
      input:
        user_data:
          name: "Charlie Brown"
          email: "charlie@example.com"
        file_paths: []

      output:
        outcome: failed
        error:
          code: "VALIDATION_ERROR"
          message: "file_paths must contain at least 1 item"

    - name: skip_condition_triggered
      description: "Test skip condition behavior"
      input:
        user_data:
          name: "David Wilson"
          email: "david@example.com"
        skip_processing: true  # This triggers skip condition
        processing_options:
          format: "json"
        file_paths: ["data/input1.csv"]

      output:
        outcome: skipped
        reason: "Processing skipped due to skip_processing flag"
```

### Test Configuration Overrides

```yaml
test:
  # Global test configuration
  stepflow_config: "test/base-config.yml"

  # Global test settings
  defaults:
    timeout: 30
    log_level: info

  cases:
    - name: quick_test
      # Inherits global settings
      input: { }
      output: { }

    - name: long_running_test
      # Override timeout for this specific test
      config_overrides:
        timeout: 300
        log_level: debug
      input: { }
      output: { }

    - name: custom_config_test
      # Use completely different config
      stepflow_config: "test/special-config.yml"
      input: { }
      output: { }
```

## Example Inputs

### Basic Examples

```yaml
examples:
  - name: basic_usage
    description: "Simple example with minimal required fields"
    input:
      user_data:
        name: "John Doe"
        email: "john@example.com"
        age: 25
      file_paths: ["sample.csv"]

  - name: advanced_usage
    description: "Example showing all optional features"
    input:
      user_data:
        name: "Jane Smith"
        email: "jane@company.com"
        age: 35
      processing_options:
        format: "xml"
        include_metadata: true
      file_paths: ["data1.csv", "data2.csv", "data3.csv"]

  - name: edge_case
    description: "Edge case with maximum values"
    input:
      user_data:
        name: "Very Long Name That Tests Maximum Length Constraints"
        email: "test@subdomain.example-domain.com"
        age: 150
      processing_options:
        format: "csv"
        include_metadata: false
      file_paths: [
        "file1.csv", "file2.csv", "file3.csv", "file4.csv", "file5.csv",
        "file6.csv", "file7.csv", "file8.csv", "file9.csv", "file10.csv"
      ]
```

### Example Categories

```yaml
examples:
  # Production examples
  - name: production_scenario_1
    description: "Typical production use case"
    tags: ["production", "common"]
    input: { }

  # Development examples
  - name: development_test
    description: "Quick test for development"
    tags: ["development", "quick"]
    input: { }

  # Edge cases
  - name: maximum_load
    description: "Test with maximum input size"
    tags: ["edge-case", "stress-test"]
    input: { }

  # Error scenarios
  - name: invalid_input_example
    description: "Example that should fail validation"
    tags: ["error", "validation"]
    expected_outcome: "validation_error"
    input: { }
```

### Example Usage

Examples serve multiple purposes:

1. **Documentation**: Show users how to use the workflow
2. **Testing**: Provide test data for automated testing
3. **Tooling**: Enable UI dropdowns and quick-start templates
4. **Validation**: Verify workflow works with realistic data

**Accessing Examples:**
- Workflow editors can provide example selection
- CLI tools can run workflows with example inputs
- Documentation generators can include examples
- Test frameworks can use examples as test cases

## Advanced Features

### Conditional Workflows

```yaml
steps:
  - id: check_environment
    component: builtin://environment_check
    input:
      environment: { $from: { workflow: input }, path: "env" }

  # Different processing based on environment
  - id: production_processing
    component: production://processor
    skip_if:
      $from: { step: check_environment }
      path: "is_development"
    input:
      data: { $from: { workflow: input }, path: "data" }

  - id: development_processing
    component: development://processor
    skip_if:
      $from: { step: check_environment }
      path: "is_production"
    input:
      data: { $from: { workflow: input }, path: "data" }
```

### Multi-Environment Support

```yaml
# Environment-specific configurations
environments:
  development:
    default_timeout: 10
    enable_debug: true
    data_source: "dev_db"

  staging:
    default_timeout: 30
    enable_debug: false
    data_source: "staging_db"

  production:
    default_timeout: 60
    enable_debug: false
    data_source: "prod_db"

# Use environment variables in steps
steps:
  - id: connect_database
    component: database://connect
    input:
      connection_string:
        $from: { environment: current }
        path: "data_source"
      timeout:
        $from: { environment: current }
        path: "default_timeout"
```

### Workflow Composition

```yaml
# Import and compose other workflows
imports:
  - path: "common/data-validation.yml"
    alias: "validate"
  - path: "processors/text-processing.yml"
    alias: "text_proc"

steps:
  # Use imported workflow as a step
  - id: validate_input
    component: builtin://eval
    input:
      workflow: { $import: "validate" }
      input: { $from: { workflow: input } }

  # Chain workflows
  - id: process_text
    component: builtin://eval
    input:
      workflow: { $import: "text_proc" }
      input: { $from: { step: validate_input } }
```

### Metadata and Annotations

```yaml
# Workflow metadata
metadata:
  author: "Development Team"
  created: "2024-01-15"
  last_modified: "2024-01-20"
  tags: ["data-processing", "production"]
  category: "data-pipelines"
  complexity: "medium"
  estimated_runtime: "5-10 minutes"

# Documentation links
documentation:
  readme: "docs/workflow-guide.md"
  api_docs: "https://api.example.com/docs"
  runbook: "https://runbooks.example.com/data-pipeline"

# Resource requirements
resources:
  memory: "512Mi"
  cpu: "0.5"
  timeout: "10m"
  max_retries: 3

# Security annotations
security:
  required_permissions: ["data:read", "storage:write"]
  sensitive_data: ["user_data.email", "api_keys"]
  compliance: ["GDPR", "SOX"]
```

## Validation and Best Practices

### Schema Validation

```yaml
# Enable strict validation
validation:
  strict_mode: true
  fail_on_extra_properties: true
  validate_step_schemas: true

# Custom validation rules
validation_rules:
  - rule: "no_empty_strings"
    description: "String fields cannot be empty"
    pattern: "^.+$"
    applies_to: ["string"]

  - rule: "valid_email_domains"
    description: "Email must be from allowed domains"
    pattern: "@(company|partner)\\.com$"
    applies_to: ["email"]
```

### Performance Optimization

```yaml
# Performance hints
performance:
  parallel_steps: ["step1", "step2", "step3"]
  cache_results: ["expensive_computation"]
  memory_efficient: true
  streaming_mode: false

# Resource limits
limits:
  max_execution_time: "1h"
  max_memory: "2Gi"
  max_blob_size: "100Mi"
  max_steps: 50
```

### Documentation Standards

```yaml
# Rich documentation
name: "User Data Processing Pipeline"
description: |
  This workflow processes user data through multiple validation and
  transformation steps. It supports multiple output formats and includes
  comprehensive error handling.

  Key features:
  - Multi-format output (JSON, XML, CSV)
  - Email validation and domain checking
  - Age-based processing rules
  - Detailed error reporting

  Usage:
  - Development: Use with test data for feature development
  - Staging: Full integration testing with realistic data
  - Production: Live user data processing

# Step documentation
steps:
  - id: validate_user_data
    description: |
      Validates user data against business rules:
      - Name must be 1-100 characters
      - Email must be valid format from allowed domains
      - Age must be realistic (0-150)
    component: validation://user_validator
    # ... rest of step configuration
```

This specification provides the complete reference for StepFlow workflow authoring, covering all supported features and advanced patterns for building robust, maintainable workflows.