---
sidebar_position: 5
---

# Command-Line Help for `submit-batch`

This document contains the help content for the `submit-batch` command-line program.

**Command Overview:**

* [`submit-batch`↴](#submit-batch)

## `submit-batch`

Submit a batch workflow to a Stepflow service for execution.

This submits a workflow and multiple inputs from a JSONL file to a remote Stepflow server for batch execution with concurrency control and progress tracking.

# Examples

```bash

# Submit batch with default concurrency

stepflow submit-batch --url=http://localhost:7837/api/v1 --flow=workflow.yaml --inputs=inputs.jsonl

# Submit batch with limited concurrency and output file

stepflow submit-batch --url=http://localhost:7837/api/v1 --flow=workflow.yaml --inputs=inputs.jsonl --max-concurrent=10 --output=results.json

```

**Usage:** `submit-batch [OPTIONS] --url <URL> --flow <FILE> --inputs <FILE>`

###### **Options:**

* `--url <URL>` — The URL of the Stepflow service
* `--flow <FILE>` — Path to the workflow file to execute
* `--inputs <FILE>` — Path to JSONL file containing inputs (one JSON object per line)
* `--max-concurrent <N>` — Maximum number of concurrent executions on the server. Defaults to number of inputs if not specified
* `--output <FILE>` — Path to write batch results (JSONL format - one result per line)
* `--variables <FILE>` — The path to the variables file.

   Should be JSON or YAML. Format is inferred from file extension.
* `--variables-json <JSON>` — The variables as a JSON string
* `--variables-yaml <YAML>` — The variables as a YAML string
* `--env-variables` — Enable environment variable fallback for missing variables.

   When enabled, missing variables will be looked up from environment variables using the pattern `STEPFLOW_VAR_<VARIABLE_NAME>`.
* `--overrides <FILE>` — Path to a file containing workflow overrides (JSON or YAML format).

   Overrides allow you to modify step properties at runtime without changing the original workflow file. Format is inferred from file extension.
* `--overrides-json <JSON>` — Workflow overrides as a JSON string.

   Specify overrides inline as JSON. Example: `--overrides-json '{"step1": {"value": {"input": {"temperature": 0.8}}}}'`
* `--overrides-yaml <YAML>` — Workflow overrides as a YAML string.

   Specify overrides inline as YAML. Example: `--overrides-yaml 'step1: {value: {input: {temperature: 0.8}}}'`



