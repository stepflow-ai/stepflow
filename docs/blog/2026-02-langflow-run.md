---
date: 2026-02-20
title: "Langflow on Stepflow with a Single Command"
description: "The stepflow-langflow CLI now supports running Langflow flows locally or against a deployed Stepflow server — no manual setup required."
slug: langflow-run-command
authors:
  - benchambers
tags: [announcements, langflow]
draft: false
---

# Running Langflow Pipelines with a Single Command

In our [first Langflow on Stepflow post](/blog/langflow-poc), we showed how to convert and execute Langflow flows on Stepflow. That demo required building the Stepflow binary from source, generating configuration files, and wiring everything together manually. It worked, but it wasn't exactly a one-liner.

Today, the `stepflow-langflow` CLI makes this dramatically simpler, thanks partly to the new [`stepflow-orchestrator`](https://pypi.org/project/stepflow-orchestrator/) package which includes the necessary binary to run locally. A single `run` command handles conversion, orchestrator startup, and execution — all in one step.

<!-- truncate -->

## Installation

```sh
pip install stepflow-langflow-integration
```

This pulls in the [`stepflow-orchestrator`](https://pypi.org/project/stepflow-orchestrator/) package automatically, which bundles the Stepflow server binary for your platform.

## The `run` command

The new `run` command supports two modes:

- **`--local`**: Starts a local Stepflow orchestrator automatically using the [`stepflow-orchestrator`](https://pypi.org/project/stepflow-orchestrator/) package. No binary to build, no config to write.
- **`--url`**: Connects to an already-running Stepflow server (local or remote).

### Running locally

To demonstrate, let's use the same Blog Content generation [flow.json](https://github.com/stepflow-ai/stepflow/blob/main/docs/static/files/2025-09-langflow-poc-flow.json) from our original POC post. Set your OpenAI API key and run:

```sh
export OPENAI_API_KEY="sk-..."

stepflow-langflow run flow.json '{}' --local
```

That's it — no tweaks, no config files, no binary to build. Langflow components that use `load_from_db` (like the Language Model's API key field) automatically resolve from environment variables at execution time, just as they do in Langflow itself.

The CLI:

1. Converts the Langflow JSON to a Stepflow workflow
2. Starts a local orchestrator with a Langflow component server
3. Submits the workflow and waits for results
4. Prints the output and shuts down

Compare this to the original POC, which required cloning the repo, building Rust from source, passing API keys via `--tweaks`, and running from a specific directory.

### Running against a deployed server

If you have a Stepflow server running (perhaps deployed to [Kubernetes](/blog/langflow-stepflow-k8s-production)), point `--url` at it instead:

```sh
stepflow-langflow run flow.json '{"message": "Hello"}' --url http://stepflow.example.com:7840
```

The conversion happens client-side; only the final Stepflow workflow is submitted to the server. This means you can iterate on your Langflow flow locally and submit it to a production cluster when ready.

### Batch execution

To run the same flow across many inputs, put one JSON object per line in a JSONL file and use `--batch`:

```sh
stepflow-langflow run flow.json --batch inputs.jsonl --local
```

Where `inputs.jsonl` looks like:

```jsonl
{"message": "Write a haiku about coding"}
{"message": "Write a haiku about coffee"}
{"message": "Write a haiku about open source"}
```

The flow is converted and uploaded once, then all inputs are executed in a single run. Stepflow handles scheduling and parallelization automatically.

### Dry run

To inspect the converted Stepflow workflow without executing it:

```sh
stepflow-langflow run flow.json '{}' --local --dry-run
```

This prints the generated YAML so you can see exactly how Langflow components map to Stepflow steps.

## How it works

Under the hood, `--local` uses the [`stepflow-orchestrator`](https://pypi.org/project/stepflow-orchestrator/) Python package, which bundles the Stepflow server binary. The CLI:

1. **Converts** the Langflow JSON to a Stepflow workflow using the built-in translator
2. **Starts** a local `StepflowOrchestrator` with a config that includes the Langflow component server as a sub-process plugin
3. **Stores** the workflow via the HTTP API (`POST /api/v1/flows`)
4. **Submits** a run and waits for completion (`POST /api/v1/runs` with `wait=true`)
5. **Displays** the results
6. **Stops** the local orchestrator

For `--url` mode, steps 2 and 6 are skipped — the CLI talks directly to the remote server.

## What's next

We're continuing to improve the experience:

- **Streaming output**: Follow step execution in real time
- **Flow caching**: Skip re-uploading unchanged flows

If you're using Langflow and want to run your flows with distributed execution, parallel step processing, or Kubernetes-scale deployment, give `stepflow-langflow run --local` a try.
