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
async with StepflowOrchestrator.start() as client:
    # client is a StepflowClient connected to the subprocess
    flow_id = client.store_flow(workflow_dict).flow_id
    result = client.run(flow_id, {"input": "value"})
    print(f"Result: {result}")

# Start with custom config
config = OrchestratorConfig(port=8080, log_level="debug")
async with StepflowOrchestrator.start(config) as client:
    # Use client.store_flow(), client.run(), etc.
    pass
```

## Development Mode

Set `STEPFLOW_DEV_BINARY` to use a local development build:

```bash
export STEPFLOW_DEV_BINARY=/path/to/stepflow-server
```
