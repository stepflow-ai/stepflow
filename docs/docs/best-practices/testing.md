---
sidebar_position: 3
---

# Testing Workflows

Stepflow provides comprehensive testing capabilities built directly into workflow files. This guide covers everything from basic test cases to advanced testing patterns and CI/CD integration.

## Test Configuration Overview

Tests are defined in the `test` section of your workflow file:

```yaml
name: "My Workflow"
# ... workflow definition ...

test:
  # Optional: Override stepflow config for tests
  stepflow_config: "test-config.yml"

  # Global test settings
  defaults:
    timeout: 30
    log_level: info

  # Test cases
  cases:
    - name: test_case_1
      # Test case definition
```

## Basic Test Cases

### Simple Success Test

```yaml
test:
  cases:
    - name: successful_processing
      description: "Test normal operation with valid input"
      input:
        user_id: "12345"
        action: "process"
      output:
        outcome: success
        result:
          status: "completed"
          user_id: "12345"
```

### Testing Error Conditions

```yaml
test:
  cases:
    - name: invalid_input
      description: "Test handling of invalid input"
      input:
        user_id: ""  # Invalid empty user ID
        action: "process"
      output:
        outcome: failed
        error:
          code: "VALIDATION_ERROR"
          message: "*user_id*"  # Error should mention user_id
```

### Testing Skipped Steps

```yaml
test:
  cases:
    - name: conditional_skip
      description: "Test step skipping based on conditions"
      input:
        skip_processing: true
        data: {"test": "value"}
      output:
        outcome: skipped
        reason: "*skip_processing*"
```

## Advanced Test Patterns

### Partial Result Matching

Use wildcards and partial matching for flexible test assertions:

```yaml
test:
  cases:
    - name: flexible_matching
      input:
        question: "What is AI?"
      output:
        outcome: success
        result:
          # Exact match
          status: "completed"

          # Wildcard match - any value is acceptable
          timestamp: "*"

          # Partial string match - answer should contain "artificial"
          answer: "*artificial*"

          # Pattern match for blob IDs
          blob_id: "sha256:*"

          # Array length check
          results:
            $length: 3  # Array should have 3 items

          # Nested object partial matching
          metadata:
            model: "gpt-3.5-turbo"
            tokens: "*"  # Any token count
```

### Testing AI Workflows

Special patterns for testing non-deterministic AI responses:

```yaml
test:
  cases:
    - name: ai_response_quality
      description: "Test AI response contains expected elements"
      input:
        question: "Explain machine learning"
        context: "Machine learning is a subset of AI..."
      output:
        outcome: success
        result:
          answer:
            # Multiple partial matches (all must be present)
            $contains: ["machine learning", "algorithm", "data"]
          confidence:
            # Numeric range checks
            $range: [0.7, 1.0]
          sources:
            # Array should not be empty
            $not_empty: true
```

### Testing with Mock Data

Override external dependencies with mock responses:

```yaml
test:
  # Use test configuration with mocked components
  stepflow_config: "test/mock-config.yml"

  cases:
    - name: mocked_api_call
      description: "Test with mocked external API"
      input:
        api_endpoint: "https://api.example.com/data"
      # Mock configuration will return predetermined responses
      output:
        outcome: success
        result:
          data: "mocked_response"
```

## Test Configuration Files

### Creating Test-Specific Config

Create `test-config.yml` for test environments:

```yaml
# test-config.yml
plugins:
  - name: builtin
    type: builtin

  # Mock external services for testing
  - name: http_mock
    type: stepflow
    command: "mock-server"
    args: ["--config", "test/http-mocks.json"]

stateStore:
  type: inMemory  # Use in-memory store for tests

# Test-specific settings
settings:
  timeout: 5  # Shorter timeouts for faster tests
  log_level: debug
  enable_tracing: true
```

### Environment-Specific Testing

```yaml
test:
  # Different configs for different test environments
  environments:
    unit:
      stepflow_config: "test/unit-config.yml"
      defaults:
        timeout: 5

    integration:
      stepflow_config: "test/integration-config.yml"
      defaults:
        timeout: 30

    e2e:
      stepflow_config: "stepflow-config.yml"  # Use production config
      defaults:
        timeout: 120

  cases:
    - name: unit_test
      environment: unit
      input: {}
      output: {}

    - name: integration_test
      environment: integration
      input: {}
      output: {}
```

## Test Data Management

### Inline Test Data

```yaml
test:
  cases:
    - name: small_dataset
      input:
        records:
          - id: 1, name: "Alice"
          - id: 2, name: "Bob"
        rules:
          validation: strict
          output_format: json
      output:
        outcome: success
```

### External Test Data Files

```yaml
test:
  cases:
    - name: large_dataset
      input_file: "test/data/large-input.json"
      output_file: "test/expected/large-output.json"

    - name: multiple_formats
      input_file: "test/data/input.yaml"
      output:
        outcome: success
        result_file: "test/expected/output.json"
```

### Generated Test Data

```yaml
test:
  cases:
    - name: generated_data
      description: "Test with dynamically generated data"
      generate_input:
        type: "user_records"
        count: 100
        seed: 12345  # For reproducible generation
      output:
        outcome: success
        result:
          processed_count: 100
```

## Running Tests

### Basic Test Execution

```bash
# Run all tests in a workflow
stepflow test workflow.yaml

# Run specific test case
stepflow test workflow.yaml --case="successful_processing"

# Run tests with specific environment
stepflow test workflow.yaml --environment=integration

# Run tests with custom config
stepflow test workflow.yaml --config=custom-test-config.yml
```

### Test Output Formats

```bash
# JSON output for CI/CD integration
stepflow test workflow.yaml --format=json

# JUnit XML for test reporting tools
stepflow test workflow.yaml --format=junit --output=test-results.xml

# Detailed output with step-by-step execution
stepflow test workflow.yaml --verbose

# Quiet mode - only show failures
stepflow test workflow.yaml --quiet
```

### Parallel Test Execution

```bash
# Run test cases in parallel
stepflow test workflow.yaml --parallel

# Limit concurrent test cases
stepflow test workflow.yaml --parallel --max-workers=4
```

## Test Organization

### Multiple Test Files

Organize tests across multiple files:

```yaml
# main-workflow.yaml
name: "Main Workflow"
# ... workflow definition ...

test:
  includes:
    - "test/unit-tests.yaml"
    - "test/integration-tests.yaml"
    - "test/performance-tests.yaml"
```

```yaml
# test/unit-tests.yaml
test_suite: "Unit Tests"
cases:
  - name: test_validation
    # ... test definition ...

  - name: test_transformation
    # ... test definition ...
```

### Test Categories and Tags

```yaml
test:
  cases:
    - name: smoke_test
      tags: ["smoke", "critical", "fast"]
      input: {}
      output: {}

    - name: performance_test
      tags: ["performance", "slow"]
      timeout: 300
      input: {}
      output: {}

    - name: edge_case_test
      tags: ["edge-case", "regression"]
      input: {}
      output: {}
```

Run tests by category:

```bash
# Run only smoke tests
stepflow test workflow.yaml --tags="smoke"

# Run fast tests (exclude slow ones)
stepflow test workflow.yaml --exclude-tags="slow"

# Run critical tests only
stepflow test workflow.yaml --tags="critical"
```

## CI/CD Integration

### GitHub Actions

```yaml
# .github/workflows/test.yml
name: Stepflow Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v3

    - name: Install Stepflow
      run: |
        curl -L https://github.com/stepflow-ai/stepflow/releases/latest/download/stepflow-linux.tar.gz | tar xz
        sudo mv stepflow /usr/local/bin/

    - name: Run Workflow Tests
      env:
        OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
      run: |
        stepflow test workflows/main.yaml --format=junit --output=test-results.xml

    - name: Upload test results
      uses: actions/upload-artifact@v3
      with:
        name: test-results
        path: test-results.xml
```

### Test Matrix

```yaml
strategy:
  matrix:
    environment: [unit, integration, e2e]

steps:
  - name: Run Tests - ${{ matrix.environment }}
    run: stepflow test workflow.yaml --environment=${{ matrix.environment }}
```

## Test Debugging

### Debug Mode

```bash
# Run tests with detailed debugging
stepflow test workflow.yaml --debug

# Enable step-by-step execution tracing
stepflow test workflow.yaml --trace

# Save intermediate results for inspection
stepflow test workflow.yaml --save-intermediate --output-dir=debug/
```