# stepflow-api

Generated API client models for communicating with Stepflow servers.

> **Note:** This package contains auto-generated code. Do not edit generated files directly.
> See [Code Generation](#code-generation) for how to update.

## Installation

```bash
pip install stepflow-api
```

## Overview

This package provides:
- **Pydantic models** for all Stepflow API request/response types
- **HTTP client utilities** for making API calls
- **Type-safe interfaces** for workflow execution, batch operations, and component discovery

It is used internally by `stepflow-client` and `stepflow-runtime`. Most users should install those packages instead of using `stepflow-api` directly.

## Usage

```python
from stepflow_api import Client
from stepflow_api.models import CreateRunRequest, CreateRunResponse
from stepflow_api.api.run import create_run

client = Client(base_url="http://localhost:7837")

# Make API calls
response = await create_run.asyncio_detailed(
    client=client,
    body=CreateRunRequest(flow_id="...", input={...})
)
```

## Code Generation

This package is generated from the Stepflow server's OpenAPI specification.

### File Structure

```
stepflow-api/
├── src/stepflow_api/
│   ├── models/
│   │   ├── generated.py      # Auto-generated Pydantic models
│   │   ├── __init__.py       # Auto-generated exports + compatibility methods
│   │   ├── *.py              # Re-export stubs OR custom overrides
│   │   └── ...
│   ├── api/                  # Hand-written API endpoint modules
│   ├── client.py             # Auto-generated HTTP client
│   ├── types.py              # Auto-generated utility types
│   └── errors.py             # Auto-generated error types
└── ...
```

### Generated vs Custom Files

- **`models/generated.py`**: Fully auto-generated from OpenAPI spec. Never edit.
- **`models/__init__.py`**: Auto-generated with compatibility methods. Never edit.
- **`models/*.py` (stubs)**: Simple re-exports, auto-generated. Can be replaced with custom overrides.
- **`api/`**: Hand-written endpoint modules. Preserved during regeneration.

Custom model overrides (files that don't start with `"""Re-export`) are automatically preserved when regenerating.

### Regenerating Models

When the Rust API changes:

```bash
# 1. Update the stored OpenAPI spec (requires building Rust server)
uv run poe api-gen --update-spec

# 2. Regenerate Python models
uv run poe api-gen

# 3. Run tests to verify
uv run poe test
```

To just regenerate from the existing spec:

```bash
uv run poe api-gen
```

To check if models are up-to-date (used in CI):

```bash
uv run poe api-check
```

### Generation Tools

The generation script (`scripts/generate_api_client.py`) uses:
- **datamodel-code-generator**: Generates Pydantic v2 models from OpenAPI
- **ruff**: Formats generated code

### Custom Model Overrides

Some API responses don't match the generated types exactly (e.g., flexible union types, RootModel unwrapping). Custom overrides in `models/` handle these cases:

- `create_run_request.py` - Flexible overrides format
- `create_run_response.py` - Flexible result format
- `run_details.py` - Flexible result format
- `batch_output_info.py` - Flexible result format
- `list_batch_outputs_response.py` - Custom list type
- `workflow_overrides.py` - Custom serialization

These files are preserved during regeneration.

## License

Apache-2.0
