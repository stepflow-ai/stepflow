---
sidebar_position: 2
---

# STDIO Transport

The STDIO transport provides process-based communication between the Stepflow runtime and component servers using standard input and output streams. This transport is ideal for local component servers that can be executed as subprocesses.

## Overview

STDIO transport creates a subprocess for each component server and communicates via:
- **stdin**: Stepflow runtime sends JSON-RPC messages to the component server
- **stdout**: Component server sends JSON-RPC responses and bidirectional requests
- **stderr**: Component server sends logs and diagnostic information (not part of protocol)

:::warning
Relying on stderr for logging is discouraged, since it is not supported by other transports.
It may be useful in development or when accessing the component-server logs is easy, but in general a structrued logging mechanism (e.g., tracing) is preferred.
:::

## Process Lifecycle

The main benefit of the STDIO transport for local development is that the Stepflow runtime manages the component server processes.

### Process Creation
The Stepflow runtime spawns component server processes using the configured command and arguments:

See [Configuration Documentation](../../configuration/) for complete plugin configuration options.

### Process Management
- **Startup**: Runtime spawns process and waits for initialization
- **Health monitoring**: Runtime monitors process status and stdio streams
- **Graceful shutdown**: Runtime closes stdin and waits for process termination
- **Forced termination**: Runtime kills process after timeout if needed

### Process Environment
Process environment, working directory, and resource limits are configured via the runtime configuration. See [Configuration - STDIO Transport](../../configuration.md#stdio-transport) for details.