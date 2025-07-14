---
sidebar_position: 10
---

# Best Practices

This guide covers best practices for designing, implementing, and maintaining StepFlow workflows that are reliable, maintainable, and performant.

## Workflow Design Principles

### Minimize Dependencies

Design workflows with minimal coupling between steps to maximize parallelism and maintainability:

```yaml
# ✅ Good - minimal dependencies
steps:
  - id: load_user
    component: user/load
    input:
      user_id: { $from: { workflow: input }, path: "user_id" }

  # These can run in parallel
  - id: load_permissions
    component: auth/permissions
    input:
      user_id: { $from: { step: load_user }, path: "id" }

  - id: load_preferences
    component: user/preferences
    input:
      user_id: { $from: { step: load_user }, path: "id" }

# ❌ Avoid - unnecessary dependencies
steps:
  - id: load_user
    component: user/load
    input:
      user_id: { $from: { workflow: input }, path: "user_id" }

  - id: load_permissions
    component: auth/permissions
    input:
      user_id: { $from: { step: load_user }, path: "id" }

  - id: load_preferences
    component: user/preferences
    input:
      user_id: { $from: { step: load_user }, path: "id" }
      # Unnecessary dependency creates false sequence
      permissions: { $from: { step: load_permissions } }
```

### Use Appropriate Granularity

Balance between too many small steps and too few large steps:

```yaml
# ✅ Good - appropriate granularity
steps:
  - id: validate_and_parse_input
    component: data/validate_parse
    input:
      raw_data: { $from: { workflow: input } }

  - id: enrich_data
    component: data/enrich
    input:
      parsed_data: { $from: { step: validate_and_parse_input } }

  - id: process_and_format
    component: data/process_format
    input:
      enriched_data: { $from: { step: enrich_data } }

# ❌ Avoid - too granular
steps:
  - id: validate_input
    component: validation/check
  - id: parse_input
    component: parsing/parse
  - id: extract_field_1
    component: data/extract
  - id: extract_field_2
    component: data/extract
  # ... many tiny steps
```

### Handle Errors Gracefully

Design workflows to handle failures elegantly:

```yaml
steps:
  # Critical operation - must succeed
  - id: authenticate_user
    component: auth/verify
    input:
      token: { $from: { workflow: input }, path: "auth_token" }

  # Optional enhancement - can fail gracefully
  - id: load_user_preferences
    component: user/preferences
    on_error:
      action: use_default
      default_value:
        theme: "default"
        notifications: true
    input:
      user_id: { $from: { step: authenticate_user }, path: "user_id" }

  # Main processing - uses preferences if available
  - id: generate_response
    component: response/create
    input:
      user_data: { $from: { step: authenticate_user } }
      preferences:
        $from: { step: load_user_preferences }
        $on_skip: "use_default"
        $default: { theme: "default", notifications: true }
```

## Component Usage Best Practices

### Choose the Right Components

Select components based on your specific needs:

```yaml
# For simple operations, use builtin components
- id: store_data
  component: builtin://put_blob
  input:
    data: { $from: { step: process_data } }

# For AI operations, use OpenAI components
- id: generate_summary
  component: builtin://openai
  input:
    messages: { $from: { step: create_messages } }

# For complex business logic, use custom components
- id: complex_analysis
  component: custom://business_analyzer
  input:
    data: { $from: { step: load_data } }
    rules: { $from: { step: load_rules } }
```

### Optimize Component Configuration

Configure components appropriately for your use case:

```yaml
steps:
  # Fast, deterministic AI responses
  - id: quick_classification
    component: builtin://openai
    input:
      messages: { $from: { step: create_simple_prompt } }
      model: "gpt-3.5-turbo"      # Faster model
      temperature: 0.1            # Low temperature for consistency
      max_tokens: 50              # Short responses

  # Creative AI responses
  - id: creative_writing
    component: builtin://openai
    input:
      messages: { $from: { step: create_creative_prompt } }
      model: "gpt-4"             # Better model for creativity
      temperature: 0.8           # Higher temperature for variety
      max_tokens: 500            # Longer responses
```

### Validate Inputs Early

Catch errors before expensive operations:

```yaml
steps:
  # Fast validation first
  - id: validate_request
    component: validation/request
    input:
      request: { $from: { workflow: input } }

  # Expensive operations only run on valid input
  - id: process_with_ai
    component: builtin://openai
    input:
      messages: { $from: { step: create_messages_from_valid_request } }
      validated_request: { $from: { step: validate_request } }
```

## Data Management Best Practices

### Use Blob Storage Effectively

Store large or reusable data in blobs:

```yaml
steps:
  # Store large dataset once
  - id: store_dataset
    component: builtin://put_blob
    input:
      data: { $from: { step: load_large_dataset } }

  # Multiple analyses reference the same blob
  - id: statistical_analysis
    component: analytics/statistics
    input:
      data_blob: { $from: { step: store_dataset }, path: "blob_id" }

  - id: ml_analysis
    component: analytics/machine_learning
    input:
      data_blob: { $from: { step: store_dataset }, path: "blob_id" }
```

### Reference Specific Fields

Avoid copying entire large objects:

```yaml
# ✅ Good - reference specific fields
- id: create_user_summary
  component: user/summarize
  input:
    user_id: { $from: { step: load_user }, path: "id" }
    user_name: { $from: { step: load_user }, path: "profile.name" }
    user_email: { $from: { step: load_user }, path: "contact.email" }

# ❌ Avoid - copying entire object
- id: create_user_summary
  component: user/summarize
  input:
    user_data: { $from: { step: load_user } }  # Copies entire user object
```

### Batch Operations When Possible

Process multiple items efficiently:

```yaml
# ✅ Good - batch processing
- id: process_all_users
  component: user/batch_process
  input:
    users: { $from: { step: load_users } }
    batch_size: 50

# ❌ Avoid - individual processing (unless parallelism is needed)
- id: process_user_1
  component: user/process
  input:
    user: { $from: { step: load_users }, path: "users[0]" }
# ... repeated for each user
```

## Schema and Validation Best Practices

### Define Clear Schemas

Use comprehensive input and output schemas:

```yaml
name: "User Data Processor"

input_schema:
  type: object
  properties:
    user_id:
      type: string
      pattern: "^[a-zA-Z0-9]{8,}$"
      description: "Unique user identifier"
    processing_options:
      type: object
      properties:
        include_analytics:
          type: boolean
          default: true
        output_format:
          type: string
          enum: ["json", "xml", "csv"]
          default: "json"
      additionalProperties: false
  required: ["user_id"]
  additionalProperties: false

output_schema:
  type: object
  properties:
    processed_data:
      type: object
      description: "Processed user data"
    metadata:
      type: object
      properties:
        processing_time_ms:
          type: integer
          minimum: 0
        version:
          type: string
      required: ["processing_time_ms", "version"]
  required: ["processed_data", "metadata"]
```

### Validate at Step Level

Add validation to individual steps when needed:

```yaml
steps:
  - id: process_user_data
    component: user/process
    input_schema:
      type: object
      properties:
        user_data:
          type: object
          properties:
            id: { type: string, minLength: 1 }
            email: { type: string, format: email }
          required: ["id", "email"]
      required: ["user_data"]
    input:
      user_data: { $from: { step: load_user } }
```

## Testing Best Practices

### Comprehensive Test Coverage

Test different scenarios and edge cases:

```yaml
test:
  cases:
    # Happy path
    - name: successful_processing
      description: "Test normal operation with valid input"
      input:
        user_id: "user123"
        processing_options:
          include_analytics: true
          output_format: "json"
      output:
        outcome: success
        result:
          processed_data: "*"
          metadata:
            processing_time_ms: "*"
            version: "1.0"

    # Error cases
    - name: invalid_user_id
      description: "Test handling of invalid user ID"
      input:
        user_id: ""  # Invalid empty ID
      output:
        outcome: failed
        error:
          code: "VALIDATION_ERROR"

    # Edge cases
    - name: minimal_input
      description: "Test with minimal required input"
      input:
        user_id: "user123"
      output:
        outcome: success
        result:
          processed_data: "*"
```

### Use Test-Specific Configuration

Create dedicated test configurations:

```yaml
test:
  stepflow_config: "test/test-config.yml"
  cases:
    # Test cases using mocked components
```

```yaml
# test/test-config.yml
plugins:
  - name: builtin
    type: builtin
  - name: mock_external_apis
    type: stepflow
    command: "test/mock-server.py"

state_store:
  type: in_memory
```

## Documentation Best Practices

### Descriptive Names and Documentation

Use clear, self-documenting names:

```yaml
name: "Customer Order Processing Pipeline"
description: |
  Processes customer orders through validation, inventory checking,
  payment processing, and fulfillment scheduling. Handles both
  standard and priority orders with appropriate error handling.

steps:
  - id: validate_order_details
    description: "Validate order format, customer info, and product availability"
    component: order/validate
    input:
      order: { $from: { workflow: input }, path: "order" }

  - id: check_inventory_availability
    description: "Verify all ordered items are in stock"
    component: inventory/check
    input:
      items: { $from: { step: validate_order_details }, path: "validated_items" }
```

### Include Examples

Provide examples in your workflow documentation:

```yaml
examples:
  - name: standard_order
    description: "Example of a standard order processing"
    input:
      order:
        customer_id: "cust_12345"
        items:
          - product_id: "prod_001"
            quantity: 2
          - product_id: "prod_002"
            quantity: 1
        shipping_address:
          street: "123 Main St"
          city: "Anytown"
          state: "CA"
          zip: "12345"

  - name: priority_order
    description: "Example of a priority order with expedited processing"
    input:
      order:
        customer_id: "cust_vip"
        priority: true
        items:
          - product_id: "prod_premium"
            quantity: 1
```

## Security Best Practices

### Handle Sensitive Data Carefully

Never include sensitive data in workflow definitions:

```yaml
# ❌ Avoid - hardcoded secrets
steps:
  - id: api_call
    component: http/request
    input:
      url: "https://api.example.com/data"
      headers:
        Authorization: "Bearer sk-1234567890abcdef"  # Don't do this!

# ✅ Good - use environment variables
steps:
  - id: api_call
    component: http/request
    input:
      url: "https://api.example.com/data"
      headers:
        Authorization: { $env: "API_TOKEN" }
```

### Validate External Input

Always validate data from external sources:

```yaml
steps:
  - id: validate_external_data
    component: validation/external
    input:
      data: { $from: { step: fetch_external_data } }
      schema: { $from: { step: load_validation_schema } }

  - id: sanitize_data
    component: security/sanitize
    input:
      validated_data: { $from: { step: validate_external_data } }
```

## Performance Best Practices

### Monitor and Optimize

Track performance metrics and optimize bottlenecks:

```yaml
steps:
  - id: performance_critical_step
    component: analytics/heavy_computation
    input:
      data: { $from: { step: load_data } }
    # Consider adding performance monitoring
    metadata:
      performance_critical: true
      expected_duration_ms: 5000
```

### Use Caching When Appropriate

Cache expensive computations:

```yaml
steps:
  - id: expensive_computation
    component: analytics/complex
    input:
      data: { $from: { step: prepare_data } }
      cache_key: { $from: { step: generate_cache_key } }
      use_cache: true
```

## Maintenance Best Practices

### Version Your Workflows

Use semantic versioning for workflows:

```yaml
name: "User Processing Pipeline"
version: "2.1.0"
description: |
  Version 2.1.0: Added optional analytics processing
  Version 2.0.0: Redesigned with new component architecture
  Version 1.x.x: Legacy processing pipeline
```

### Keep Dependencies Updated

Regularly review and update component dependencies:

```yaml
# Document component versions or requirements
metadata:
  component_requirements:
    openai_component: ">=1.2.0"
    data_processor: "^2.0.0"
  last_updated: "2024-01-15"
  updated_by: "dev-team"
```

### Refactor When Needed

Regularly review and refactor workflows:

1. **Remove unused steps**: Clean up workflows periodically
2. **Combine related operations**: Merge steps that always run together
3. **Split complex steps**: Break down overly complex operations
4. **Update deprecated patterns**: Migrate to newer best practices

## Common Anti-Patterns to Avoid

### Don't Create Monolithic Workflows

```yaml
# ❌ Avoid - one giant workflow doing everything
name: "Everything Processor"
steps:
  # 50+ steps doing unrelated things
```

```yaml
# ✅ Good - focused, composable workflows
name: "User Data Processor"
steps:
  # 5-10 related steps for user processing
```

### Don't Ignore Error Handling

```yaml
# ❌ Avoid - no error handling
steps:
  - id: critical_operation
    component: external/api
    input:
      data: { $from: { workflow: input } }
    # What happens if the API is down?
```

```yaml
# ✅ Good - comprehensive error handling
steps:
  - id: critical_operation
    component: external/api
    on_error:
      action: retry
      max_attempts: 3
      fallback:
        action: use_default
        default_value: { status: "unavailable" }
    input:
      data: { $from: { workflow: input } }
```

### Don't Overcomplicate Simple Operations

```yaml
# ❌ Avoid - overengineering simple tasks
steps:
  - id: extract_field_setup
    component: config/setup
  - id: extract_field_validate
    component: validation/check
  - id: extract_field_execute
    component: data/extract
  - id: extract_field_cleanup
    component: cleanup/finalize

# ✅ Good - simple extraction
steps:
  - id: extract_field
    component: builtin://extract
    input:
      data: { $from: { step: load_data } }
      path: "user.email"
```

Following these best practices will help you create robust, maintainable, and efficient StepFlow workflows that scale well and are easy to debug and modify.