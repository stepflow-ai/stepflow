---
sidebar_position: 2
---

# Basic Operations

These fundamental examples demonstrate core StepFlow concepts including data flow, blob storage, file operations, and error handling. Perfect for getting started with workflow development.

## Simple Data Transformation

A basic workflow that processes input data through multiple transformation steps.

```yaml
name: "Simple Data Transformation"
description: "Transform user data through multiple steps with validation"

input_schema:
  type: object
  properties:
    user_id:
      type: string
      description: "User identifier"
    raw_data:
      type: object
      description: "Raw user data to process"
  required: ["user_id", "raw_data"]

output_schema:
  type: object
  properties:
    processed_data:
      type: object
      description: "Fully processed user data"
    validation_report:
      type: object
      description: "Data validation results"

steps:
  # Step 1: Validate input data
  - id: validate_input
    component: builtin://put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            data: { type: object }
        code: |
          # Simple validation logic
          required_fields = ['name', 'email']
          missing_fields = [f for f in required_fields if f not in input['data']]
          
          {
            'is_valid': len(missing_fields) == 0,
            'missing_fields': missing_fields,
            'field_count': len(input['data'].keys())
          }

  # Step 2: Run validation
  - id: run_validation
    component: python://udf
    input:
      blob_id: { $from: { step: validate_input }, path: "blob_id" }
      input:
        data: { $from: { workflow: input }, path: "raw_data" }

  # Step 3: Transform data (only if validation passes)
  - id: transform_data
    component: builtin://put_blob
    skip: 
      $from: { step: run_validation }
      path: "is_valid"
      # Skip if NOT valid (invert the boolean logic in your condition)
    input:
      data:
        input_schema:
          type: object
          properties:
            data: { type: object }
            user_id: { type: string }
        code: |
          # Transform and enrich the data
          transformed = {
            'user_id': input['user_id'],
            'name': input['data'].get('name', '').title(),
            'email': input['data'].get('email', '').lower(),
            'created_at': '2024-01-01T00:00:00Z',
            'status': 'active'
          }
          transformed

  # Step 4: Execute transformation
  - id: execute_transform
    component: python://udf
    input:
      blob_id: { $from: { step: transform_data }, path: "blob_id" }
      input:
        data: { $from: { workflow: input }, path: "raw_data" }
        user_id: { $from: { workflow: input }, path: "user_id" }

output:
  processed_data:
    $from: { step: execute_transform }
    $on_skip: "use_default"
    $default: {}
  validation_report: { $from: { step: run_validation } }

test:
  cases:
  - name: valid_user_data
    description: Process valid user data
    input:
      user_id: "user123"
      raw_data:
        name: "john doe"
        email: "JOHN@EXAMPLE.COM"
        age: 30
    output:
      outcome: success
      result:
        processed_data:
          user_id: "user123"
          name: "John Doe"
          email: "john@example.com"
          created_at: "2024-01-01T00:00:00Z"
          status: "active"
        validation_report:
          is_valid: true
          missing_fields: []
          field_count: 3
  
  - name: invalid_user_data
    description: Handle invalid user data gracefully
    input:
      user_id: "user456"
      raw_data:
        age: 25
        # Missing required name and email
    output:
      outcome: success
      result:
        processed_data: {}
        validation_report:
          is_valid: false
          missing_fields: ["name", "email"]
          field_count: 1
```

### Key Concepts Demonstrated

1. **Data Validation**: Using Python UDFs to validate input data
2. **Conditional Execution**: Skipping transformation if validation fails
3. **Skip Handling**: Using `$on_skip` to provide default values
4. **Blob Storage**: Storing reusable Python code as blobs
5. **Test Cases**: Comprehensive testing for both success and failure scenarios

## File Processing Workflow

Load, validate, and process data from files with comprehensive error handling.

```yaml
name: "File Processing Pipeline"
description: "Load configuration and data files, then process them safely"

input_schema:
  type: object
  properties:
    config_file:
      type: string
      description: "Path to configuration file"
    data_file:
      type: string
      description: "Path to data file to process"
    output_format:
      type: string
      enum: ["json", "yaml"]
      default: "json"
  required: ["config_file", "data_file"]

steps:
  # Load configuration with error handling
  - id: load_config
    component: builtin://load_file
    on_error:
      action: continue
      default_output:
        data:
          processing_rules: {}
          output_settings: {}
        metadata:
          resolved_path: ""
          size_bytes: 0
          format: "json"
    input:
      path: { $from: { workflow: input }, path: "config_file" }

  # Load data file
  - id: load_data
    component: builtin://load_file
    input:
      path: { $from: { workflow: input }, path: "data_file" }

  # Validate that we have required configuration
  - id: validate_config
    component: builtin://put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            config: { type: object }
        code: |
          config = input['config']
          has_rules = 'processing_rules' in config and config['processing_rules']
          has_output = 'output_settings' in config
          
          {
            'is_valid': has_rules and has_output,
            'has_processing_rules': has_rules,
            'has_output_settings': has_output,
            'rule_count': len(config.get('processing_rules', {}))
          }

  # Run configuration validation
  - id: check_config
    component: python://udf
    input:
      blob_id: { $from: { step: validate_config }, path: "blob_id" }
      input:
        config: { $from: { step: load_config }, path: "data" }

  # Process data if configuration is valid
  - id: process_data
    component: builtin://put_blob
    skip:
      $from: { step: check_config }
      path: "is_valid"
      # Note: You'd implement the skip logic to skip if NOT valid
    input:
      data:
        input_schema:
          type: object
          properties:
            data: { type: object }
            rules: { type: object }
        code: |
          # Apply processing rules to data
          data = input['data']
          rules = input['rules']
          
          processed = {}
          for key, value in data.items():
            if key in rules:
              rule = rules[key]
              if rule.get('uppercase', False):
                processed[key] = str(value).upper()
              elif rule.get('lowercase', False):
                processed[key] = str(value).lower()
              else:
                processed[key] = value
            else:
              processed[key] = value
          
          processed

  # Execute processing
  - id: execute_processing
    component: python://udf
    input:
      blob_id: { $from: { step: process_data }, path: "blob_id" }
      input:
        data: { $from: { step: load_data }, path: "data" }
        rules: { $from: { step: load_config }, path: "data.processing_rules" }

  # Format output according to requested format
  - id: format_output
    component: builtin://put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            data: { type: object }
            format: { type: string }
        code: |
          import json
          import yaml
          
          data = input['data']
          format_type = input['format']
          
          if format_type == 'yaml':
            formatted = yaml.dump(data, default_flow_style=False)
          else:
            formatted = json.dumps(data, indent=2)
          
          {
            'formatted_data': formatted,
            'format': format_type,
            'size': len(formatted)
          }

  # Execute formatting
  - id: execute_formatting
    component: python://udf
    input:
      blob_id: { $from: { step: format_output }, path: "blob_id" }
      input:
        data:
          $from: { step: execute_processing }
          $on_skip: "use_default"
          $default: {}
        format: { $from: { workflow: input }, path: "output_format" }

output:
  processed_data: { $from: { step: execute_formatting } }
  config_validation: { $from: { step: check_config } }
  file_metadata:
    config_file: { $from: { step: load_config }, path: "metadata" }
    data_file: { $from: { step: load_data }, path: "metadata" }

test:
  cases:
  - name: successful_processing
    description: Process files with valid configuration
    input:
      config_file: "test-config.json"
      data_file: "test-data.json"
      output_format: "json"
    # Note: This would require actual test files in the examples directory
```

:::info Required Test Files
This example requires creating test files:
- `test-config.json`: `{"processing_rules": {"name": {"uppercase": true}}, "output_settings": {}}`
- `test-data.json`: `{"name": "john", "age": 30, "city": "New York"}`
:::

## Parallel Processing Pattern

Demonstrate how StepFlow automatically parallelizes independent operations.

```yaml
name: "Parallel Data Processing"
description: "Process multiple data sources in parallel and combine results"

input_schema:
  type: object
  properties:
    data_sources:
      type: array
      items:
        type: object
        properties:
          name: { type: string }
          url: { type: string }
          type: { type: string, enum: ["api", "file", "database"] }
      description: "List of data sources to process"

steps:
  # These steps run in parallel - one for each data source type
  
  # Process API sources
  - id: process_api_sources
    component: builtin://put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            sources: { type: array }
        code: |
          # Filter and process API sources
          api_sources = [s for s in input['sources'] if s['type'] == 'api']
          results = []
          for source in api_sources:
            # Simulate API processing
            results.append({
              'name': source['name'],
              'type': 'api',
              'status': 'processed',
              'record_count': 100,  # Simulated
              'url': source['url']
            })
          results

  # Process file sources  
  - id: process_file_sources
    component: builtin://put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            sources: { type: array }
        code: |
          # Filter and process file sources
          file_sources = [s for s in input['sources'] if s['type'] == 'file']
          results = []
          for source in file_sources:
            # Simulate file processing
            results.append({
              'name': source['name'],
              'type': 'file',
              'status': 'processed',
              'record_count': 50,  # Simulated
              'path': source['url']
            })
          results

  # Process database sources
  - id: process_database_sources
    component: builtin://put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            sources: { type: array }
        code: |
          # Filter and process database sources
          db_sources = [s for s in input['sources'] if s['type'] == 'database']
          results = []
          for source in db_sources:
            # Simulate database processing
            results.append({
              'name': source['name'],
              'type': 'database',
              'status': 'processed',
              'record_count': 200,  # Simulated
              'connection': source['url']
            })
          results

  # Execute all processing in parallel
  - id: execute_api_processing
    component: python://udf
    input:
      blob_id: { $from: { step: process_api_sources }, path: "blob_id" }
      input:
        sources: { $from: { workflow: input }, path: "data_sources" }

  - id: execute_file_processing
    component: python://udf
    input:
      blob_id: { $from: { step: process_file_sources }, path: "blob_id" }
      input:
        sources: { $from: { workflow: input }, path: "data_sources" }

  - id: execute_database_processing
    component: python://udf
    input:
      blob_id: { $from: { step: process_database_sources }, path: "blob_id" }
      input:
        sources: { $from: { workflow: input }, path: "data_sources" }

  # Combine all results (waits for all parallel processing to complete)
  - id: combine_results
    component: builtin://put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            api_results: { type: array }
            file_results: { type: array }
            database_results: { type: array }
        code: |
          # Combine all processing results
          all_results = []
          all_results.extend(input['api_results'])
          all_results.extend(input['file_results'])
          all_results.extend(input['database_results'])
          
          # Generate summary
          total_records = sum(r['record_count'] for r in all_results)
          by_type = {}
          for result in all_results:
            result_type = result['type']
            if result_type not in by_type:
              by_type[result_type] = {'count': 0, 'records': 0}
            by_type[result_type]['count'] += 1
            by_type[result_type]['records'] += result['record_count']
          
          {
            'all_results': all_results,
            'summary': {
              'total_sources': len(all_results),
              'total_records': total_records,
              'by_type': by_type
            }
          }

  # Execute combination
  - id: execute_combination
    component: python://udf
    input:
      blob_id: { $from: { step: combine_results }, path: "blob_id" }
      input:
        api_results: { $from: { step: execute_api_processing } }
        file_results: { $from: { step: execute_file_processing } }
        database_results: { $from: { step: execute_database_processing } }

output:
  results: { $from: { step: execute_combination } }

test:
  cases:
  - name: mixed_data_sources
    description: Process multiple types of data sources in parallel
    input:
      data_sources:
        - name: "user_api"
          url: "https://api.example.com/users"
          type: "api"
        - name: "config_file"
          url: "/data/config.json"
          type: "file"
        - name: "main_database"
          url: "postgresql://localhost/main"
          type: "database"
        - name: "logs_file"
          url: "/logs/app.log"
          type: "file"
    output:
      outcome: success
      result:
        summary:
          total_sources: 4
          total_records: 450
          by_type:
            api:
              count: 1
              records: 100
            file:
              count: 2
              records: 100
            database:
              count: 1
              records: 200
```

### Parallel Execution Benefits

This workflow demonstrates how StepFlow automatically:
1. **Identifies independent steps**: The three processing steps have no dependencies on each other
2. **Executes in parallel**: All three processing steps run simultaneously
3. **Synchronizes results**: The combination step waits for all parallel steps to complete
4. **Maximizes throughput**: No unnecessary waiting or sequential bottlenecks

## Error Recovery Patterns

:::info Future Component
The `http://get` component shown below is a planned future component. Currently, you can achieve similar functionality using custom Python components with the `requests` library.
:::

```yaml
name: "Robust Error Recovery"
description: "Demonstrate different error recovery strategies"

input_schema:
  type: object
  properties:
    api_endpoints:
      type: array
      items:
        type: string
      description: "List of API endpoints to try"
    fallback_data:
      type: object
      description: "Data to use if all APIs fail"

steps:
  # Try primary API with retry logic
  - id: try_primary_api
    component: http://get  # Future component
    on_error:
      action: continue
      default_output:
        success: false
        data: null
        error: "Primary API failed"
    input:
      url: { $from: { workflow: input }, path: "api_endpoints[0]" }
      timeout: 5
      retries: 3

  # Try secondary API if primary fails
  - id: try_secondary_api
    component: http://get  # Future component
    skip: { $from: { step: try_primary_api }, path: "success" }
    on_error:
      action: continue
      default_output:
        success: false
        data: null
        error: "Secondary API failed"
    input:
      url: { $from: { workflow: input }, path: "api_endpoints[1]" }
      timeout: 10
      retries: 2

  # Use fallback data if all APIs fail
  - id: use_fallback_data
    component: builtin://put_blob
    skip:
      # Skip if either API succeeded
      # This would need custom logic to check if ANY API succeeded
    input:
      data:
        success: true
        data: { $from: { workflow: input }, path: "fallback_data" }
        source: "fallback"

  # Combine results from whichever source succeeded
  - id: combine_results
    component: builtin://put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            primary: { type: object }
            secondary: { type: object }
            fallback: { type: object }
        code: |
          # Use the first successful result
          if input['primary']['success']:
            result = {
              'data': input['primary']['data'],
              'source': 'primary_api',
              'backup_used': False
            }
          elif input['secondary']['success']:
            result = {
              'data': input['secondary']['data'],
              'source': 'secondary_api',
              'backup_used': True
            }
          else:
            result = {
              'data': input['fallback']['data'],
              'source': 'fallback_data',
              'backup_used': True
            }
          result

  # Execute result combination
  - id: execute_combination
    component: python://udf
    input:
      blob_id: { $from: { step: combine_results }, path: "blob_id" }
      input:
        primary:
          $from: { step: try_primary_api }
          $on_skip: "use_default"
          $default: { success: false, data: null }
        secondary:
          $from: { step: try_secondary_api }
          $on_skip: "use_default"
          $default: { success: false, data: null }
        fallback:
          $from: { step: use_fallback_data }
          $on_skip: "use_default"
          $default: { success: false, data: null }

output:
  result: { $from: { step: execute_combination } }
```

:::tip Community Opportunity
Building robust HTTP client components with retry logic, circuit breakers, and timeout handling would be valuable community contributions. Consider implementing these using the Python SDK with libraries like `httpx` or `requests`.
:::

## Next Steps

Now that you understand the basics, explore more advanced patterns:

- **[AI Workflows](./ai-workflows.md)** - Integrate AI capabilities into your workflows
- **[Data Processing](./data-processing.md)** - Handle complex data transformation pipelines
- **[Custom Components](./custom-components.md)** - Build your own reusable components

These basic patterns form the foundation for all StepFlow workflows. Master these concepts and you'll be ready to build sophisticated, reliable workflow applications.