# stepflow-runtime

Embedded Stepflow runtime with bundled server binary.

## Installation

```bash
pip install stepflow-runtime
```

## Overview

This package provides an embedded Stepflow server that manages a subprocess lifecycle, enabling local workflow execution without requiring a separate server installation. The server binary is bundled with the package, so no additional downloads are needed.

## Usage

### Basic Usage

```python
from stepflow_runtime import StepflowRuntime

# Start with builtin plugin only (no config needed)
async with StepflowRuntime.start() as runtime:
    result = await runtime.run("workflow.yaml", {"x": 1, "y": 2})

    if result.is_success:
        print(f"Output: {result.output}")
    elif result.is_failed:
        print(f"Error: {result.error.message}")
```

### With Configuration

```python
# Start with a configuration file for additional plugins
async with StepflowRuntime.start("stepflow-config.yml") as runtime:
    result = await runtime.run("workflow.yaml", {"prompt": "Hello"})
```

### Advanced Configuration

```python
from stepflow_runtime import StepflowRuntime, LogConfig, RestartPolicy
import os

runtime = StepflowRuntime.start(
    "stepflow-config.yml",
    port=8080,  # Use specific port
    env={"OPENAI_API_KEY": os.environ["OPENAI_API_KEY"]},
    log_config=LogConfig(
        level="debug",
        capture=True,  # Capture logs to Python
        python_logger="myapp.stepflow",
    ),
    restart_policy=RestartPolicy.ON_FAILURE,
    max_restarts=3,
    startup_timeout=30.0,
    on_crash=lambda code: print(f"Server crashed with code {code}"),
)

try:
    result = await runtime.run("workflow.yaml", {"x": 1})
finally:
    runtime.stop()
```

### Accessing Logs

```python
# When capture=True, logs are available programmatically
for entry in runtime.get_recent_logs(limit=50):
    print(f"{entry.timestamp} [{entry.level}] {entry.message}")
```

### Synchronous and Async Context Managers

```python
# Async context manager
async with StepflowRuntime.start("config.yml") as runtime:
    result = await runtime.run("workflow.yaml", {"x": 1})

# Sync context manager (for simple scripts)
with StepflowRuntime.start("config.yml") as runtime:
    import asyncio
    result = asyncio.run(runtime.run("workflow.yaml", {"x": 1}))
```

### Batch Execution

```python
async with StepflowRuntime.start() as runtime:
    # Submit multiple runs
    inputs = [{"x": 1}, {"x": 2}, {"x": 3}]
    batch_id = await runtime.submit_batch("workflow.yaml", inputs, max_concurrent=2)

    # Get results
    details, results = await runtime.get_batch(batch_id)
    for result in results:
        print(result.output)
```

## Interchangeable with StepflowClient

The runtime implements the same `StepflowExecutor` protocol as `StepflowClient`, making them interchangeable:

```python
from stepflow import StepflowExecutor

def get_executor(local: bool = True) -> StepflowExecutor:
    if local:
        from stepflow_runtime import StepflowRuntime
        return StepflowRuntime.start("config.yml")
    from stepflow_client import StepflowClient
    return StepflowClient("http://production:7837")

# Use either local or remote execution
async with get_executor(local=True) as executor:
    result = await executor.run("workflow.yaml", input_data)
```

## Configuration Options

### LogConfig

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `level` | str | "info" | Log level (trace, debug, info, warn, error) |
| `format` | str | "json" | Log format (json or text) |
| `capture` | bool | True | Capture logs to Python logging |
| `python_logger` | str | "stepflow.server" | Python logger name for forwarded logs |
| `file_path` | Path | None | Optional file path for log output |
| `otlp_endpoint` | str | None | Optional OTLP endpoint for log export |

### RestartPolicy

| Value | Description |
|-------|-------------|
| `NEVER` | Never restart the subprocess |
| `ON_FAILURE` | Restart only on non-zero exit code |
| `ALWAYS` | Always restart unless explicitly stopped |

### StepflowRuntime.start() Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config_path` | str/Path/None | None | Path to stepflow-config.yml |
| `port` | int/None | None | Server port (auto-selected if None) |
| `env` | dict | None | Additional environment variables |
| `inherit_env` | bool | True | Inherit parent process environment |
| `log_config` | LogConfig | LogConfig() | Logging configuration |
| `restart_policy` | RestartPolicy | NEVER | When to restart the subprocess |
| `max_restarts` | int | 3 | Maximum restart attempts |
| `startup_timeout` | float | 30.0 | Seconds to wait for server ready |
| `on_crash` | Callable | None | Callback on subprocess crash |

## Signal Handling

The runtime automatically handles signals for graceful shutdown:

- **SIGTERM/SIGINT**: Gracefully stops the subprocess and cleans up
- **atexit**: Ensures cleanup on Python interpreter exit

Signal handlers are automatically restored when the runtime stops.

## License

Apache-2.0
