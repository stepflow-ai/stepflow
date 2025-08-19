---
sidebar_position: 1
---

# CLI Reference

Stepflow provides several commands for executing workflows in different ways. This reference covers all available commands and their options.

## Commands

- **[`run`](./run.md)** - Run a workflow directly
- **[`serve`](./serve.md)** - Start a Stepflow service
- **[`submit`](./submit.md)** - Submit a workflow to a Stepflow service
- **[`test`](./test.md)** - Run tests defined in workflow files or directories
- **[`list-components`](./list-components.md)** - List all available components from a stepflow config
- **[`repl`](./repl.md)** - Start an interactive REPL for workflow development and debugging
- **[`validate`](./validate.md)** - Validate workflow files and configuration
- **[`visualize`](./visualize.md)** - Visualize workflow structure as a graph

## Global Options

All commands support these global options:

- `--log-level <LEVEL>` - Set the log level for Stepflow
- `--other-log-level <LEVEL>` - Set the log level for other parts of Stepflow
- `--log-file <FILE>` - Write logs to a file instead of stderr
- `--omit-stack-trace <OMIT_STACK_TRACE>` - Omit stack traces (line numbers of errors)
