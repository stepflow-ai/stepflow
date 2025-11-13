---
sidebar_position: 3
---

# Command-Line Help for `run-batch`

This document contains the help content for the `run-batch` command-line program.

**Command Overview:**

* [`run-batch`↴](#run-batch)

## `run-batch`

Run a batch of workflows directly.

Execute multiple workflow runs locally with concurrency control.

# Examples

```bash

# Run batch with inputs from JSONL file

stepflow run-batch --flow=workflow.yaml --inputs=inputs.jsonl --config=stepflow-config.yml

# Run batch with limited concurrency

stepflow run-batch --flow=workflow.yaml --inputs=inputs.jsonl --max-concurrent=5

# Run batch with output to file

stepflow run-batch --flow=workflow.yaml --inputs=inputs.jsonl --output=results.jsonl

```

**Usage:** `run-batch [OPTIONS] --flow <FILE> --inputs <FILE>`

###### **Options:**

* `--flow <FILE>` — Path to the workflow file to execute
* `--inputs <FILE>` — Path to JSONL file containing inputs (one JSON object per line)
* `--max-concurrent <N>` — Maximum number of concurrent executions. Defaults to number of inputs if not specified
* `--output <FILE>` — Path to write batch results (JSONL format - one result per line)
* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.
* `--overrides <FILE>` — Path to a file containing workflow overrides (JSON or YAML format).

   Overrides allow you to modify step properties at runtime without changing the original workflow file. Format is inferred from file extension.
* `--overrides-json <JSON>` — Workflow overrides as a JSON string.

   Specify overrides inline as JSON. Example: `--overrides-json '{"step1": {"value": {"input": {"temperature": 0.8}}}}'`
* `--overrides-yaml <YAML>` — Workflow overrides as a YAML string.

   Specify overrides inline as YAML. Example: `--overrides-yaml 'step1: {value: {input: {temperature: 0.8}}}'`



