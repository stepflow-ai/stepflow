---
sidebar_position: 4
---

# Command-Line Help for `test`

This document contains the help content for the `test` command-line program.

**Command Overview:**

* [`test`↴](#test)

## `test`

Run tests defined in workflow files or directories.

Run test cases defined in workflow files.

# Examples

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

**Usage:** `test [OPTIONS] [PATH]...`

###### **Arguments:**

* `<PATH>` — Paths to workflow files or directories containing tests

###### **Options:**

* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.
* `--case <NAME>` — Run only specific test case(s) by name. Can be repeated
* `--update` — Update expected outputs with actual outputs from test runs
* `--diff` — Show diff when tests fail



