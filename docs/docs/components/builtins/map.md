---
sidebar_position: 9
---

### `map`

Apply a workflow to each item in a list and collect all results. Processes items in parallel for better performance.

#### Input

```yaml
input:
  workflow:
    steps: [ ]
    output: { }
  items:
    - <item 1>
    - <item 2>
    - <item 3>
```

- **`workflow`** (required): Complete workflow definition to apply to each item
- **`items`** (required): Array of items to process

#### Output

```yaml
output:
  results:
    - <result for item 1>
    - <result for item 2> 
    - <result for item 3>
  successful: 2
  failed: 1
  skipped: 0
```

- **`results`**: Array of FlowResult objects (one per input item)
- **`successful`**: Count of successful executions
- **`failed`**: Count of failed executions  
- **`skipped`**: Count of skipped executions

#### Example

```yaml
steps:
  - id: process_numbers
    component: /builtin/map
    input:
      workflow:
        schema:
          type: object
          properties:
            value: { type: number }
        steps:
          - id: calculate
            component: /python/math_operations
            input:
              operation: "square"
              value: { $input: "value" }
        output:
          squared: { $step: calculate, path: "result" }
      items:
        - { value: 1 }
        - { value: 2 }
        - { value: 3 }
        - { value: 4 }
```

#### Batch Processing Example

```yaml
steps:
  - id: load_user_list
    component: /builtin/load_file
    input:
      path: "users.json"

  - id: process_users
    component: /builtin/map
    input:
      workflow:
        steps:
          - id: get_profile
            component: /api/fetch_user_profile
            input:
              user_id: { $input: "id" }
          - id: analyze_activity
            component: /python/analyze_user_activity
            input:
              profile: { $step: get_profile }
        output:
          user_analysis: { $step: analyze_activity }
      items: { $step: load_user_list, path: "data.users" }
```

#### Use Cases

- **Batch Processing**: Apply the same logic to multiple data items
- **Parallel Execution**: Process items concurrently for better performance  
- **Data Transformation**: Transform each item in a dataset
- **API Batch Operations**: Make multiple API calls with different parameters