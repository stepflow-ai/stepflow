# stepflow-orchestrator

Platform-specific Python wheels bundling the stepflow-server binary for local orchestration.

## Installation

```bash
pip install stepflow-orchestrator
```

## Usage

```python
from stepflow_orchestrator import StepflowOrchestrator, OrchestratorConfig

# Start with default config (auto-assigned port)
async with StepflowOrchestrator.start() as orchestrator:
    print(f"Server running at {orchestrator.url}")
    # Use orchestrator.url with your preferred HTTP client

# Start with custom config
config = OrchestratorConfig(
    port=8080,
    log_level="debug",
    config={"plugins": {"builtin": {"type": "builtin"}}}
)
async with StepflowOrchestrator.start(config) as orchestrator:
    # orchestrator.url - server URL (e.g., "http://127.0.0.1:8080")
    # orchestrator.port - bound port number
    # orchestrator.is_running - check if process is alive
    pass
```

## With stepflow-py Client

For a convenient combined experience, use `stepflow-py[local]`:

```bash
pip install stepflow-py[local]
```

```python
from stepflow_py import StepflowClient
from stepflow_py.config import StepflowConfig

config = StepflowConfig(plugins={...}, routes={...})
async with StepflowClient.local(config) as client:
    # Client owns the orchestrator - both shut down on exit
    response = await client.store_flow(workflow)
    result = await client.run(response.flow_id, input_data)
```

## Development Mode

Set `STEPFLOW_DEV_BINARY` to use a local development build:

```bash
export STEPFLOW_DEV_BINARY=/path/to/stepflow-server
```

## Changelog

This package bundles the Stepflow server binary. For release notes and changelog, see the main [Stepflow Changelog](https://github.com/stepflow-ai/stepflow/blob/main/stepflow-rs/CHANGELOG.md).
