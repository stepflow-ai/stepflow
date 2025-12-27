---
sidebar_position: 8
---

# Command-Line Help for `infer`

This document contains the help content for the `infer` command-line program.

**Command Overview:**

* [`infer`↴](#infer)

## `infer`

Infer types for a workflow and output an annotated flow.

Perform static type checking on a workflow and optionally output an annotated version with inferred output schemas. This helps catch type mismatches before execution and documents the expected data flow through the workflow.

# Examples

```bash # Output annotated flow to stdout stepflow infer --flow=workflow.yaml

# Output to auto-named file (workflow.inferred.yaml) stepflow infer --flow=workflow.yaml --output

# Output to specific file stepflow infer --flow=workflow.yaml --output=annotated.yaml

# Strict mode (untyped outputs are errors) stepflow infer --flow=workflow.yaml --strict ```

**Usage:** `infer [OPTIONS] --flow <FILE>`

###### **Options:**

* `--flow <FILE>` — Path to the workflow file to type check
* `-o`, `--output <FILE>` — Output path for the annotated workflow.

   If specified without a value, writes to `<flow>.inferred.yaml`. If specified with a value, writes to that path. If not specified, outputs to stdout.
* `--show-step-inputs` — Show inferred input types for each step.

   This prints the expected input type for each step based on component schemas. Useful for understanding what data each step receives.
* `--strict` — Treat untyped component outputs as errors instead of warnings
* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.



