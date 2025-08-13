---
sidebar_position: 3
---

# Command-Line Help for `serve`

This document contains the help content for the `serve` command-line program.

**Command Overview:**

* [`serve`↴](#serve)

## `serve`

Start a Stepflow service.

Start a Stepflow service that can accept workflow submissions via HTTP API.

# Examples

```bash

# Start server on default port (7837)

stepflow serve

# Start server on custom port with config

stepflow serve --port=8080 --config=production-config.yml

```

**Usage:** `serve [OPTIONS]`

###### **Options:**

* `--port <PORT>` — Port to run the service on

  Default value: `7837`
* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.



