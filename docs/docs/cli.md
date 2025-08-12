---
sidebar_position: 7
---

# CLI Reference

StepFlow provides several commands for executing workflows in different ways. This reference covers all available commands and their options.

## Global Options

All commands support these global options:

- `--log-level <LEVEL>` - Set log level for StepFlow [default: info] [values: trace, debug, info, warn, error]
- `--other-log-level <LEVEL>` - Set log level for other components [default: warn] [values: trace, debug, info, warn, error]
- `--log-file <FILE>` - Write logs to file instead of stderr
- `--omit-stack-trace` - Omit stack traces (line numbers of errors)

## Running a Workflow {#run}

Execute a workflow directly and return the result.

### Usage

```bash
stepflow run --flow=<workflow.yaml> [OPTIONS]
```

### Options

**Required:**
- `--flow <FILE>` - Path to workflow file to execute

**Input Options (mutually exclusive):**
- `--input <FILE>` - Path to input file (JSON/YAML, format inferred from extension)
- `--input-json <JSON>` - Input value as JSON string
- `--input-yaml <YAML>` - Input value as YAML string
- `--stdin-format <FORMAT>` - Read input from stdin [default: json] [values: json, yaml]

**Other Options:**
- `--config <FILE>` - Path to stepflow config file (auto-detects if not specified)
- `--output <FILE>` - Path to write output (stdout if not set)

### Examples

```bash
# Run with input file
stepflow run --flow=examples/basic/workflow.yaml --input=examples/basic/input1.json

# Run with inline JSON input
stepflow run --flow=workflow.yaml --input-json='{"m": 3, "n": 4}'

# Run with inline YAML input
stepflow run --flow=workflow.yaml --input-yaml='m: 2\nn: 7'

# Run with stdin input
echo '{"m": 1, "n": 2}' | stepflow run --flow=workflow.yaml --stdin-format=json

# Run with custom config and output to file
stepflow run --flow=workflow.yaml --input=input.json --config=my-config.yml --output=result.json
```

## Running a Workflow Server {#serve}

Start a StepFlow service that can accept workflow submissions via HTTP API.

### Usage

```bash
stepflow serve [OPTIONS]
```

### Options

- `--port <PORT>` - Port to run service on [default: 7837]
- `--config <FILE>` - Path to stepflow config file

### Examples

```bash
# Start server on default port (7837)
stepflow serve

# Start server on custom port with config
stepflow serve --port=8080 --config=production-config.yml
```

### API Endpoints

When running as a server, StepFlow exposes these endpoints:

- `POST /api/v1/workflow/run` - Submit a workflow for execution
- `GET /api/v1/workflow/{id}` - Get workflow execution status
- `GET /api/v1/health` - Health check endpoint

## Submitting a Workflow {#submit}

Submit a workflow to a running StepFlow service.

### Usage

```bash
stepflow submit --flow=<workflow.yaml> [OPTIONS]
```

### Options

**Required:**
- `--flow <FILE>` - Path to workflow file to submit

**Input Options (mutually exclusive):**
- `--input <FILE>` - Path to input file
- `--input-json <JSON>` - Input value as JSON string
- `--input-yaml <YAML>` - Input value as YAML string
- `--stdin-format <FORMAT>` - Read input from stdin [default: json]

**Other Options:**
- `--url <URL>` - URL of StepFlow service [default: http://localhost:7837]
- `--output <FILE>` - Path to write output (stdout if not set)

### Examples

```bash
# Submit to local server
stepflow submit --flow=workflow.yaml --input=input.json

# Submit to remote server
stepflow submit --url=http://production-server:7837 --flow=workflow.yaml --input-json='{"key": "value"}'

# Submit with inline YAML input
stepflow submit --flow=workflow.yaml --input-yaml='param: value'
```

## Testing Workflows {#test}

Run test cases defined in workflow files.

### Usage

```bash
stepflow test <PATH> [OPTIONS]
```

### Arguments

- `<PATH>` - Path to workflow file or directory containing tests (required)

### Options

- `--config <FILE>` - Path to stepflow config file
- `--case <NAME>` - Run only specific test case(s) by name (can be repeated)
- `--update` - Update expected outputs with actual outputs from test runs
- `--diff` - Show diff when tests fail

### Examples

```bash
# Run all tests in a workflow file
stepflow test examples/basic/workflow.yaml

# Run tests in a directory
stepflow test examples/

# Run specific test case
stepflow test workflow.yaml --case=calculate_with_8_and_5

# Run multiple specific test cases
stepflow test workflow.yaml --case=test1 --case=test2

# Update expected outputs (snapshot testing)
stepflow test workflow.yaml --update

# Show detailed diff on test failures
stepflow test workflow.yaml --diff
```

### Test Format

Tests are defined in the workflow file under the `test` section:

```yaml
test:
  cases:
  - name: calculate_with_8_and_5
    description: Test calculation with m=8, n=5
    input:
      m: 8
      n: 5
    output:
      outcome: success
      result:
        m_plus_n: 13
        m_times_n: 40
```

## Listing Components {#list-components}

List all available components from the configured plugins.

### Usage

```bash
stepflow list-components [OPTIONS]
```

### Options

- `--config <FILE>` - Path to stepflow config file
- `--format <FORMAT>` - Output format [default: pretty] [values: pretty, json, yaml]
- `--schemas <true|false>` - Include component schemas in output (defaults to false for pretty format, true for json/yaml)

### Examples

```bash
# List components in pretty format
stepflow list-components

# List components with JSON output including schemas
stepflow list-components --format=json

# List components from specific config without schemas
stepflow list-components --config=my-config.yml --format=yaml --schemas=false
```

## Interactive REPL {#repl}

Start an interactive REPL (Read-Eval-Print Loop) for workflow development and debugging.

### Usage

```bash
stepflow repl [OPTIONS]
```

### Options

- `--config <FILE>` - Path to stepflow config file

### Examples

```bash
# Start REPL with default config
stepflow repl

# Start REPL with custom config
stepflow repl --config=development-config.yml
```

### REPL Commands

The REPL provides an interactive environment for:
- Testing workflow fragments
- Debugging step execution
- Exploring component capabilities
- Iterative workflow development

## Configuration File Resolution

StepFlow uses a hierarchical approach to find configuration files:

1. **Explicit path**: Use `--config` option if provided
2. **Workflow directory**: Look for `stepflow-config.yml`, `stepflow-config.yaml`, `stepflow_config.yml`, or `stepflow_config.yaml` in the workflow file's directory
3. **Current directory**: Look for same files in current working directory
4. **Default**: Use built-in configuration with builtin components only

## Input Format Detection

StepFlow automatically detects input formats:

- **File extension**: `.json` files are parsed as JSON, `.yaml`/`.yml` as YAML
- **Content detection**: Falls back to content-based detection if extension is ambiguous
- **Stdin**: Use `--stdin-format` to specify format when reading from stdin