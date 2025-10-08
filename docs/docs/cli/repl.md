---
sidebar_position: 7
---

# Command-Line Help for `repl`

This document contains the help content for the `repl` command-line program.

**Command Overview:**

* [`repl`↴](#repl)

## `repl`

Start an interactive REPL for workflow development and debugging.

Start an interactive REPL (Read-Eval-Print Loop) for workflow development and debugging. The REPL provides an interactive environment for testing workflow fragments, debugging step execution, exploring component capabilities, and iterative workflow development.

# Examples

```bash

# Start REPL with default config

stepflow repl

# Start REPL with custom config

stepflow repl --config=development-config.yml

```

**Usage:** `repl [OPTIONS]`

###### **Options:**

* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.



