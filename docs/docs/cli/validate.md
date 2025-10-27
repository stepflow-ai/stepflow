---
sidebar_position: 9
---

# Command-Line Help for `validate`

This document contains the help content for the `validate` command-line program.

**Command Overview:**

* [`validate`↴](#validate)

## `validate`

Validate workflow files and configuration.

Validate workflow files and configuration without executing them. This performs workflow validation (structure, step dependencies, value references), configuration validation (plugin definitions, routing rules), component routing validation, and schema validation. Returns 0 for success, 1+ for validation failures (suitable for CI/CD pipelines).

# Examples

```bash

# Validate workflow with auto-detected config stepflow validate --flow=examples/basic/workflow.yaml

# Validate with specific config stepflow validate --flow=workflow.yaml --config=my-config.yml

```

**Usage:** `validate [OPTIONS] --flow <FILE>`

###### **Options:**

* `--flow <FILE>` — Path to the workflow file to validate
* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.



