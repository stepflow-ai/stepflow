---
sidebar_position: 4
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



