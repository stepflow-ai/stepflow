# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## About Stepflow

Stepflow is an orchestration engine for AI workflows with a flexible plugin architecture. It provides an execution runtime that coordinates parallel step execution, manages state, and routes component requests to appropriate backend services. Workflows are defined declaratively in YAML/JSON, allowing dynamic configuration through routing rules and runtime overrides. The system supports multiple component backends including built-in components, Python SDK servers, and Model Context Protocol (MCP) integrations.

## Repository Organization

- **`stepflow-rs/`**: Core orchestrator and load balancer written in Rust
  - Workflow execution engine with parallel processing
  - Plugin system and component routing
  - JSON-RPC protocol definitions for component communication
  - Built-in components (OpenAI, eval, etc.) and MCP integration
  - CLI and HTTP service
  - See `stepflow-rs/CLAUDE.md` for Rust development, building, testing, and conventions

- **`sdks/`**: Language-specific SDKs for building component servers
  - `sdks/python/`: Python SDK with stdio and HTTP transport mode.
     See `sdks/python/CLAUDE.md` for Python SDK development

- **`integrations/`**: Third-party integrations
  - `integrations/langflow/`: Langflow translator and component server

- **`examples/`**: Complete workflow examples demonstrating features and patterns

- **`tests/`**: End-to-end test workflows used for integration testing

- **`schemas/`**: JSON schemas for flows and protocol
  - `flow.json`: Workflow definition schema
  - `protocol.json`: Configuration schema
  - Generated from Rust types, used by SDKs and documentation

- **`docs/`**: Documentation site (see https://stepflow.org or build locally)
  - User guides, API references, and tutorials

## Specialized Guides

- **Rust Development**: See `stepflow-rs/CLAUDE.md` for Rust conventions, testing, error handling, and coding standards
- **Python SDK**: See `sdks/python/CLAUDE.md` for Python development, version compatibility, and HTTP/stdio modes

## Key Concepts

- **Flow**: Declarative workflow definition with steps, inputs, and outputs
- **Step**: Single operation within a workflow that invokes a component
- **Component**: Executable implementation provided by a plugin (e.g., `/python/my_func`, `/builtin/openai`)
- **Plugin**: Service providing one or more components (builtin, Python SDK, MCP server)
- **Routing**: Configuration rules mapping component paths to plugin backends
- **Value Expressions**: Data flow between steps using `$step`, `$input`, and `$variable` expressions
- **Blob Storage**: Persistent JSON data with content-based IDs (SHA-256 hashes)
- **Run**: An execution of a flow for specific inputs.

## Configuration

Configuration file (`stepflow-config.yml`) defines plugins, routing rules, and state storage.

**Schema Reference**: See `stepflow-rs/crates/stepflow-config/src/types.rs` for definitive types and `schema/stepflow-config.schema.json` for JSON schema.

### Plugin Types

**builtin**: Built-in components (OpenAI, eval, create_messages)
**stepflow**: Component servers with stdio or http transport
**mcp**: Model Context Protocol servers

### Example Configuration

```yaml
plugins:
  builtin:
    type: builtin

  python_stdio:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "../sdks/python/stepflow-server", "run", "stepflow_server"]
    env:  # Optional, supports ${VAR:-default} substitution
      PYTHONPATH: "${HOME}/custom/path"
      USER_CONFIG: "${USER:-anonymous}"

  python_http:
    type: stepflow
    transport: http
    url: "http://localhost:8080"

  filesystem:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "${HOME}/workspace"]
    env:
      MCP_LOG_LEVEL: "${LOG_LEVEL:-info}"

routes:
  "/python/{*component}":
    - plugin: python_stdio
  "/python_http/{*component}":
    - plugin: python_http
  "/filesystem/{*component}":
    - plugin: filesystem
  "/{*component}":
    - plugin: builtin

stateStore:
  type: sqlite
  databaseUrl: "sqlite:workflow_state.db"
  autoMigrate: true
  maxConnections: 10
```

### Environment Variable Substitution

Both `stepflow` and `mcp` plugins support environment variable substitution in `args` and `env` fields:
- `${VAR}`: Substitute variable (error if not found)
- `${VAR:-default}`: Substitute with default if not found
- Nested substitution: `${HOME}/projects/${USER}`

### Routes

Routes map component paths to plugins. Rules are evaluated in order, first match wins.

**Path Patterns**:
- `{component}`: Single path segment
- `{*component}`: Wildcard matching remaining path

**Advanced**: Routes support input-based conditions. See schema for details.

### State Stores

**In-Memory** (default):
```yaml
stateStore:
  type: inMemory
```

**SQLite**:
```yaml
stateStore:
  type: sqlite
  databaseUrl: "sqlite:workflow_state.db"  # or ":memory:"
  autoMigrate: true
  maxConnections: 10
```

## Workflow Syntax

**Schema Reference**: See `stepflow-rs/crates/stepflow-core/src/flow.rs` for definitive types and `schema/flow.schema.json` for JSON schema.

### Value References

```yaml
steps:
  step1:
    component: /builtin/openai
    input:
      # Reference workflow input
      message: { $input: "message" }

      # Reference step output (use JSONPath $. for nested fields)
      context: { $step: "previous_step", path: "$.data.context" }

      # Simple field access (no dots)
      value: { $step: "math_step", path: "result" }
```

**Path Resolution Rules**:
1. Simple paths (no dots): `path: "field"` - Direct field access
2. JSONPath (with $.): `path: "$.nested.field"` - Nested field access
3. No path: Returns entire object/result

### Workflow Overrides

Runtime modifications to step properties without changing workflow files.

**Use Cases**:
- Parameter tuning (temperatures, token limits, prompts)
- Component switching (different models/implementations)
- Environment-specific configuration
- A/B testing

**Override Sources**:
```bash
# From file
cargo run -- run --flow=workflow.yaml --input=input.json --overrides=overrides.yaml

# Inline JSON
cargo run -- run --flow=workflow.yaml --input=input.json \
  --overrides-json='{"step1": {"value": {"input": {"temperature": 0.9}}}}'

# Inline YAML
cargo run -- run --flow=workflow.yaml --input=input.json \
  --overrides-yaml='step1: {value: {input: {temperature: 0.9}}}'
```

**Override Format**:
```yaml
step_id:
  $type: merge_patch  # Optional, default is merge_patch (RFC 7396)
  value:
    input:
      temperature: 0.8
      api_key: "override-key"
    component: "/different/component"
```

**Features**:
- JSON Merge Patch (RFC 7396) semantics
- Step-level targeting
- Full type safety and validation
- Works with both `run` and `submit` commands

## Workflow Validation

```bash
# Validate workflow and configuration
cargo run -- validate --flow=workflow.yaml --config=stepflow-config.yml

# Auto-discover config (searches workflow dir → current dir → builtin only)
cargo run -- validate --flow=workflow.yaml

# Use in CI/CD (exit code 0 = success, 1+ = failure)
cargo run -- validate --flow=production.yaml --config=prod-config.yml
```

**Validation Features**:
- Workflow structure, step dependencies, value references
- Plugin definitions and routing rules
- Component routing (ensures all components route to configured plugins)
- Schema validation for component inputs/outputs
- User-friendly output with clear error messages

**Why both flow and config?** Workflows reference component paths like `/python/udf` or `/builtin/openai` which require routing rules to map to actual plugins.

**When to validate**:
- Before committing workflow/configuration changes
- In CI/CD pipelines
- When debugging execution issues
- After modifying plugin configurations

## Bidirectional Communication & Blob Support

The protocol supports bidirectional communication allowing components to make calls back to the stepflow runtime.

**From stepflow to components**:
- `initialize`: Initialize component server
- `component_info`: Get component schema
- `component_execute`: Execute component

**From components to stepflow**:
- `blob_store`: Store JSON data, returns content-based ID
- `blob_get`: Retrieve JSON data by blob ID

### Python SDK Example

```python
from stepflow_server import StepflowStdioServer, StepflowContext
import msgspec

class MyInput(msgspec.Struct):
    data: dict

class MyOutput(msgspec.Struct):
    blob_id: str

server = StepflowStdioServer()

@server.component
async def my_component(input: MyInput, context: StepflowContext) -> MyOutput:
    # Store data as a blob
    blob_id = await context.put_blob(input.data)
    return MyOutput(blob_id=blob_id)

server.run()
```

See `sdks/python/CLAUDE.md` for more Python SDK patterns.

## ICLA (Individual Contributor License Agreement)

All contributors must sign an ICLA before pull requests can be merged. GitHub Actions enforces this automatically.

```bash
# Check ICLA status
python scripts/check_icla.py

# Sign the ICLA (first-time contributors)
python scripts/sign_icla.py

# Commit and push the signature file
git add .github/cla-signatures.json
git commit -m "Add ICLA signature"
git push
```

**Note**: Email in signature must match git config. If mismatch occurs, update git config and re-sign.

See `ICLA.md` and `scripts/` for complete details.

## Git Commit Messages

Use conventional commit prefixes:
- `feat:` for new features
- `fix:` for bug fixes
- `docs:` for documentation changes
- `style:` for formatting changes
- `refactor:` for code refactoring
- `test:` for adding or modifying tests
- `chore:` for maintenance tasks

Guidelines:
- Use present tense ("Add feature" not "Added feature")
- Start with a capital letter
- Keep first line under 50 characters

## Code References

When referencing specific functions or code, include the pattern `file_path:line_number` to allow easy navigation:

```
Clients are marked as failed in the `connectToServer` function in src/services/process.ts:712.
```
