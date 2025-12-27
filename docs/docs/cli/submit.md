---
sidebar_position: 3
---

# Command-Line Help for `submit`

This document contains the help content for the `submit` command-line program.

**Command Overview:**

* [`submit`↴](#submit)

## `submit`

Submit a workflow to a Stepflow server.

Submit a workflow to a running Stepflow server.

# Examples

```bash

# Submit to local server

stepflow submit --flow=workflow.yaml --input=input.json

# Submit to remote server

stepflow submit --url=http://production-server:7840 --flow=workflow.yaml --input-json='{"key": "value"}'

# Submit with inline YAML input

stepflow submit --flow=workflow.yaml --input-yaml='param: value'

# Submit batch with multiple inputs from JSONL file

stepflow submit --flow=workflow.yaml --inputs=inputs.jsonl --output=results.jsonl

```

**Usage:** `submit [OPTIONS] --flow <FILE>`

###### **Options:**

* `--url <URL>` — The URL of the Stepflow service to submit the workflow to

  Default value: `http://localhost:7837`
* `--flow <FILE>` — Path to the workflow file to submit
* `--input <FILE>` — The path to the input file to execute the workflow with.

   Should be JSON or YAML. Format is inferred from file extension.
* `--input-json <JSON>` — The input value as a JSON string
* `--input-yaml <YAML>` — The input value as a YAML string
* `--stdin-format <FORMAT>` — The format for stdin input (json or yaml).

   Only used when reading from stdin (no other input options specified).

  Default value: `json`

  Possible values: `json`, `yaml`

* `--inputs <FILE>` — Path to JSONL file containing multiple inputs (one JSON object per line).

   When specified, the workflow is executed once per line in the file. Results are written in JSONL format (one result per line).
* `--max-concurrent <N>` — Maximum number of concurrent executions on the server (only used with --inputs).

   Defaults to number of inputs if not specified.
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
* `--output <FILE>` — Path to write the output to.

   If not set, will write to stdout.



