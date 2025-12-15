---
sidebar_position: 4
---

# Performance Optimization

This guide covers strategies for optimizing Stepflow workflow performance, including parallelism patterns, resource management, and component selection.

## Maximizing Parallelism

### Independent Operations

Structure workflows to maximize parallel execution by identifying steps that can run simultaneously:

```yaml
steps:
  # Load base data
  - id: load_user
    component: /user/load
    input:
      user_id: { { $input }, path: "user_id" }

  # All these can run in parallel after load_user completes
  - id: load_user_posts
    component: /content/posts
    input:
      user_id: { { $step: load_user }, path: "id" }

  - id: load_user_followers
    component: /social/followers
    input:
      user_id: { { $step: load_user }, path: "id" }

  - id: load_user_activity
    component: /analytics/activity
    input:
      user_id: { { $step: load_user }, path: "id" }

  - id: calculate_metrics
    component: /analytics/metrics
    input:
      user_data: { { $step: load_user } }
```

### Avoid False Dependencies

Don't create unnecessary dependencies that reduce parallelism:

```yaml
# ❌ Bad - creates false dependency chain
steps:
  - id: step1
    component: /data/process
    input:
      data: { { $input } }

  - id: step2
    component: /data/validate
    input:
      # This creates unnecessary dependency on step1
      original_data: { { $input } }
      processed_data: { { $step: step1 } }

# ✅ Good - remove false dependency
steps:
  - id: step1
    component: /data/process
    input:
      data: { { $input } }

  - id: step2
    component: /data/validate
    input:
      # Can run in parallel with step1
      data: { { $input } }
```

### Parallel Data Fetching

Fetch data from multiple sources simultaneously:

```yaml
steps:
  # All three data sources fetch in parallel
  - id: fetch_data_source_1
    component: /http/get
    input:
      url: "https://api1.example.com/data"

  - id: fetch_data_source_2
    component: /http/get
    input:
      url: "https://api2.example.com/data"

  - id: fetch_data_source_3
    component: /http/get
    input:
      url: "https://api3.example.com/data"

  # Waits for all three fetches to complete
  - id: combine_data
    component: /data/merge
    input:
      source1: { { $step: fetch_data_source_1 } }
      source2: { { $step: fetch_data_source_2 } }
      source3: { { $step: fetch_data_source_3 } }
```

## Resource Management

### Memory Optimization

#### Use Blob Storage for Large Data

Store large datasets in blobs to avoid memory duplication:

```yaml
steps:
  # Store large dataset in blob
  - id: store_large_dataset
    component: /builtin/put_blob
    input:
      data: { { $step: load_massive_dataset } }

  # Multiple steps can reference the same blob efficiently
  - id: analyze_subset_1
    component: /analytics/process
    input:
      data_blob: { { $step: store_large_dataset }, path: "blob_id" }
      filter: "category=A"

  - id: analyze_subset_2
    component: /analytics/process
    input:
      data_blob: { { $step: store_large_dataset }, path: "blob_id" }
      filter: "category=B"
```

#### Reference Specific Fields

Avoid copying entire large objects by referencing specific fields:

```yaml
steps:
  - id: process_user_data
    component: /user/process
    input:
      # ✅ Good - reference specific fields
      user_id: { { $step: load_user }, path: "id" }
      user_name: { { $step: load_user }, path: "profile.name" }

  - id: inefficient_processing
    component: /user/process
    input:
      # ❌ Avoid - copying entire large object
      user_data: { { $step: load_user } }
```

### Component Performance

#### Choose Appropriate Components

Select components based on the complexity of your task:

```yaml
# For simple data transformations, use lightweight components
- id: extract_field
  component: /extract
  input:
    data: { { $step: load_data } }
    path: "metadata.id"

# For complex processing, use specialized components
- id: ai_analysis
  component: /builtin/openai
  input:
    messages: [...]
    model: "gpt-4"
```

#### Batch Operations

Process data in batches when possible to reduce overhead:

```yaml
# ✅ Good - batch processing
- id: process_all_items
  component: /data/batch_process
  input:
    items: { { $step: load_items } }
    batch_size: 100

# ❌ Avoid - individual processing (unless parallelism is needed)
- id: process_item_1
  component: /data/process_single
  input:
    item: { { $step: load_items }, path: "items[0]" }
```

#### Component Configuration

Optimize component settings for your use case:

```yaml
steps:
  - id: ai_generation
    component: /builtin/openai
    input:
      messages: { { $step: create_messages } }
      # Optimize parameters for performance vs quality
      model: "gpt-3.5-turbo"  # Faster than gpt-4
      max_tokens: 150         # Limit output length
      temperature: 0.3        # Lower temperature for faster, more deterministic responses
```

## Data Flow Optimization

### Minimize Data Movement

Structure workflows to minimize data copying and movement:

```yaml
steps:
  # Store shared data once
  - id: store_shared_context
    component: /builtin/put_blob
    input:
      data: { { $step: load_context } }

  # Multiple processing steps reference the same blob
  - id: analysis_1
    component: /analytics/type_a
    input:
      context_blob: { { $step: store_shared_context }, path: "blob_id" }
      specific_data: { { $step: load_specific_1 } }

  - id: analysis_2
    component: /analytics/type_b
    input:
      context_blob: { { $step: store_shared_context }, path: "blob_id" }
      specific_data: { { $step: load_specific_2 } }
```

### Early Validation

Validate inputs early to avoid expensive processing on invalid data:

```yaml
steps:
  # Fast validation step
  - id: validate_input
    component: /validation/fast_check
    input:
      data: { { $input } }

  # Expensive processing only runs on valid data
  - id: expensive_processing
    component: /ai/complex_analysis
    input:
      validated_data: { { $step: validate_input } }
```

## AI Workflow Optimization

### Prompt Optimization

Optimize AI prompts for performance and cost:

```yaml
steps:
  - id: efficient_ai_call
    component: /builtin/openai
    input:
      messages:
        - role: system
          # Concise system prompt
          content: "Answer briefly and directly."
        - role: user
          # Clear, specific user prompt
          content: { { $step: format_prompt }, path: "optimized_prompt" }
      # Performance settings
      max_tokens: 100        # Limit response length
      temperature: 0.1       # Lower temperature for consistency
      top_p: 0.9            # Focus on high-probability tokens
```

### AI Response Caching

Cache AI responses for repeated queries:

```yaml
steps:
  # Check cache first
  - id: check_ai_cache
    component: /cache/check
    input:
      key: { { $step: create_cache_key }, path: "cache_key" }

  # Only call AI if not cached
  - id: generate_ai_response
    component: /builtin/openai
    skipIf: { { $step: check_ai_cache }, path: "cache_hit" }
    input:
      messages: { { $step: create_messages } }

  # Store response in cache
  - id: cache_ai_response
    component: /cache/store
    skipIf: { { $step: check_ai_cache }, path: "cache_hit" }
    input:
      key: { { $step: create_cache_key }, path: "cache_key" }
      value: { { $step: generate_ai_response } }
```

## Workflow Architecture Patterns

### Fan-Out/Fan-In Pattern

Process multiple independent items and combine results:

```yaml
steps:
  # Fan-out: Process multiple items in parallel
  - id: process_item_1
    component: /item/process
    input:
      item: { { $input }, path: "items[0]" }

  - id: process_item_2
    component: /item/process
    input:
      item: { { $input }, path: "items[1]" }

  - id: process_item_3
    component: /item/process
    input:
      item: { { $input }, path: "items[2]" }

  # Fan-in: Combine all results
  - id: combine_results
    component: /data/combine
    input:
      results:
        - { { $step: process_item_1 } }
        - { { $step: process_item_2 } }
        - { { $step: process_item_3 } }
```

### Pipeline Pattern

Create processing pipelines with minimal intermediate storage:

```yaml
steps:
  - id: stage_1
    component: /pipeline/extract
    input:
      source: { { $input } }

  - id: stage_2
    component: /pipeline/transform
    input:
      data: { { $step: stage_1 } }

  - id: stage_3
    component: /pipeline/load
    input:
      transformed_data: { { $step: stage_2 } }
```

## Performance Monitoring

### Key Metrics to Track

Monitor these performance indicators:

1. **Step Execution Time**: How long each step takes
2. **Parallel Efficiency**: How well parallel steps utilize resources
3. **Memory Usage**: Peak memory consumption during execution
4. **Blob Storage Usage**: Size and frequency of blob operations
5. **Component Startup Time**: Time to initialize components

### Performance Testing

Add performance tests to your workflows:

```yaml
test:
  cases:
    - name: performance_test
      description: "Ensure workflow completes within time limit"
      tags: ["performance"]
      input:
        # Large input to test performance
        data_size: "1MB"
        batch_size: 1000
      output:
        outcome: success
        performance_metrics:
          execution_time_ms:
            $less_than: 30000  # Must complete in under 30 seconds
          memory_usage_mb:
            $less_than: 512    # Must use less than 512MB
```

## Common Performance Anti-Patterns

### Avoid These Patterns

#### Sequential Processing When Parallel is Possible

```yaml
# ❌ Bad - sequential processing
steps:
  - id: process_1
    component: /data/process
    input:
      data: { { $input }, path: "data1" }

  - id: process_2
    component: /data/process
    input:
      data: { { $input }, path: "data2" }
      # Unnecessary dependency creates false sequence
      previous: { { $step: process_1 } }

# ✅ Good - parallel processing
steps:
  - id: process_1
    component: /data/process
    input:
      data: { { $input }, path: "data1" }

  - id: process_2
    component: /data/process
    input:
      data: { { $input }, path: "data2" }
      # No dependency - runs in parallel
```

#### Over-Granular Steps

```yaml
# ❌ Bad - too many small steps
steps:
  - id: get_field_1
    component: /extract
    input:
      data: { { $step: load_data } }
      path: "field1"

  - id: get_field_2
    component: /extract
    input:
      data: { { $step: load_data } }
      path: "field2"

# ✅ Good - combined extraction
steps:
  - id: extract_fields
    component: /data/extract_multiple
    input:
      data: { { $step: load_data } }
      fields: ["field1", "field2"]
```

#### Large Data Copying

```yaml
# ❌ Bad - copying large objects repeatedly
steps:
  - id: step1
    component: /process/a
    input:
      large_dataset: { { $step: load_large_data } }

  - id: step2
    component: /process/b
    input:
      large_dataset: { { $step: load_large_data } }

# ✅ Good - use blob storage
steps:
  - id: store_large_data
    component: /builtin/put_blob
    input:
      data: { { $step: load_large_data } }

  - id: step1
    component: /process/a
    input:
      data_blob: { { $step: store_large_data }, path: "blob_id" }

  - id: step2
    component: /process/b
    input:
      data_blob: { { $step: store_large_data }, path: "blob_id" }
```

## Optimization Checklist

When optimizing workflow performance, check:

### ✅ Parallelism
- [ ] Steps without dependencies run in parallel
- [ ] No false dependencies between independent operations
- [ ] Data fetching operations run concurrently

### ✅ Resource Usage
- [ ] Large datasets stored in blobs
- [ ] Specific fields referenced instead of entire objects
- [ ] Appropriate component selection for task complexity

### ✅ Data Flow
- [ ] Minimal data copying and movement
- [ ] Early validation to catch errors before expensive operations
- [ ] Efficient use of intermediate results

### ✅ AI Optimization
- [ ] Concise, optimized prompts
- [ ] Appropriate model selection for task
- [ ] Response caching for repeated queries

### ✅ Architecture
- [ ] Appropriate granularity of steps
- [ ] Efficient workflow patterns (fan-out/fan-in, pipeline)
- [ ] Performance monitoring and testing

Following these optimization strategies will help you build high-performance Stepflow workflows that scale efficiently and provide fast response times.