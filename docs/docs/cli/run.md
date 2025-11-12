---
sidebar_position: 2
---

# Command-Line Help for `run`

This document contains the help content for the `run` command-line program.

**Command Overview:**

* [`run`↴](#run)

## `run`

Run a workflow directly.

Execute a workflow directly and return the result.

# Examples

```bash

# Run with input file

stepflow run --flow=examples/basic/workflow.yaml --input=examples/basic/input1.json

# Run with inline JSON input

stepflow run --flow=workflow.yaml --input-json='{"m": 3, "n": 4}'

# Run with inline YAML input

stepflow run --flow=workflow.yaml --input-yaml='m: 2\nn: 7'

# Run with stdin input

echo '{"m": 1, "n": 2}' | stepflow run --flow=workflow.yaml --stdin-format=json

# Run with custom config and output to file

stepflow run --flow=workflow.yaml --input=input.json --config=my-config.yml --output=result.json

```

**Usage:** `run [OPTIONS] --flow <FILE>`

###### **Options:**

* `--flow <FILE>` — Path to the workflow file to execute
* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.
* `--input <FILE>` — The path to the input file to execute the workflow with.

   Should be JSON or YAML. Format is inferred from file extension.
* `--input-json <JSON>` — The input value as a JSON string
* `--input-yaml <YAML>` — The input value as a YAML string
* `--stdin-format <FORMAT>` — The format for stdin input (json or yaml).

   Only used when reading from stdin (no other input options specified).

  Default value: `json`

  Possible values: `json`, `yaml`

* `--output <FILE>` — Path to write the output to.

   If not set, will write to stdout.
* `--overrides <FILE>` — Path to a file containing workflow overrides (JSON or YAML format).

   Overrides allow you to modify step properties at runtime without changing the original workflow file. Format is inferred from file extension.
* `--overrides-json <JSON>` — Workflow overrides as a JSON string.

   Specify overrides inline as JSON. Example: `--overrides-json '{"step1": {"value": {"input": {"temperature": 0.8}}}}'`
* `--overrides-yaml <YAML>` — Workflow overrides as a YAML string.

   Specify overrides inline as YAML. Example: `--overrides-yaml 'step1: {value: {input: {temperature: 0.8}}}'`



