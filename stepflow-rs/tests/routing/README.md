# Routing Tests

This directory contains comprehensive tests and examples for the StepFlow routing system, demonstrating path transformation and component filtering capabilities.

## Configuration Overview

The `stepflow-config.yml` file demonstrates advanced routing features:

### Complex Routing with Component Filtering
```yaml
- match: "/python/*"
  target:
    plugin: python
    strip_segments: []  # No transformation needed
    components: ["udf"]  # Only allow udf component
    exclude_components: ["debug_*"]  # Exclude debug tools
```

This configuration:
- **Matches** components that start with `/python/`
- **No path transformation** since the Python plugin returns correctly prefixed paths
- **Filters components** to only allow `udf` component
- **Excludes** any components matching the `debug_*` pattern

### Simple Routing
```yaml
- match: "/builtin/*"
  target: builtin
```

This provides direct access to builtin components without transformation.

## Test Files

### `path_transformation.yaml`
Demonstrates how routing works with component filtering:

- **User writes**: `/python/udf` in the workflow
- **System routes**: Directly to the Python plugin (no transformation needed)
- **Plugin receives**: `/udf` (the component it actually implements)

The workflow shows:
1. Data processing using `/python/udf`
2. Analysis using `/python/udf` (reused for demo purposes)
3. Final message generation using builtin `create_messages`

### `component_filtering.yaml`
Shows how component filtering works:

- **Allowed components**: Only `udf` is accessible through the `/python/*` route
- **Blocked components**: Any component matching `debug_*` pattern would be filtered out
- **Test case**: Uses `/python/udf` for text processing

## Key Features Demonstrated

### 1. Component Filtering
- Include specific components: `components: ["udf"]`
- Exclude patterns: `exclude_components: ["debug_*"]`
- Provides security and organization benefits

### 2. Multiple Route Support
- Same plugin can have multiple routing rules
- Different filtering for different routes
- Flexible access control

### 3. Path Transformation (Advanced)
- Strip segments for plugins that return unprefixed paths
- Configurable with `strip_segments: ["segment1", "segment2"]`
- Useful for organizing component namespaces

### 4. Backward Compatibility
- Simple string targets still work: `target: builtin`
- Complex targets provide advanced features
- Gradual migration path

## Running the Tests

```bash
# Run the path transformation test
cargo run -- run --flow=tests/routing/path_transformation.yaml --input='{"numbers": [1,2,3,4,5], "message": "test"}' --config=tests/routing/stepflow-config.yml

# Run the component filtering test  
cargo run -- run --flow=tests/routing/component_filtering.yaml --input='{"text": "Hello World", "operation": "uppercase"}' --config=tests/routing/stepflow-config.yml

# List available components to see the routing in action
cargo run -- list-components --config=tests/routing/stepflow-config.yml
```

## Expected Component Listing

With the routing configuration, `list-components` should show:

```
Available Components:
====================

Component: /python/udf
  Description: Execute user-defined function (UDF) using cached compiled functions from blobs

Component: /builtin/create_messages
  Description: Create a chat message list from system instructions and user prompt

Component: /builtin/eval
  Description: Execute a nested workflow with given input and return the result

Component: /builtin/get_blob
  Description: Retrieve JSON data from a blob using its ID

Component: /builtin/load_file
  Description: Load and parse a file (JSON, YAML, or text) from the filesystem

Component: /builtin/openai
  Description: Send messages to OpenAI's chat completion API and get a response

Component: /builtin/put_blob
  Description: Store JSON data as a blob and return its content-addressable ID
```

Note how:
- Python components appear as `/python/udf` (only the allowed component)
- No debug components are shown (filtered out by `exclude_components`)
- Builtin components are available with `/builtin/` prefix
- The routing system provides a unified view of all accessible components