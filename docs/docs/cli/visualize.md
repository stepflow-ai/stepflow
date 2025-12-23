---
sidebar_position: 11
---

# Command-Line Help for `visualize`

This document contains the help content for the `visualize` command-line program.

**Command Overview:**

* [`visualize`↴](#visualize)

## `visualize`

Visualize workflow structure as a graph.

Generate a visual representation of workflow structure showing steps, dependencies, and component routing. Supports multiple output formats (DOT, SVG, PNG) with optional features like component server coloring and detailed tooltips.

For SVG and PNG formats, output defaults to a file next to the workflow with matching extension (e.g., workflow.yaml → workflow.svg). Use --output to override. DOT format outputs to stdout by default.

# Examples

```bash # Generate SVG visualization (writes to workflow.svg) stepflow visualize workflow.yaml

# Generate PNG (writes to workflow.png) stepflow visualize workflow.yaml --format=png

# Specify custom output path stepflow visualize workflow.yaml --output=diagrams/flow.svg

# Output DOT to stdout for piping stepflow visualize workflow.yaml --format=dot

# Minimal visualization without server details stepflow visualize workflow.yaml --no-servers ```

**Usage:** `visualize [OPTIONS] <FLOW>`

###### **Arguments:**

* `<FLOW>` — Path to the workflow file to visualize

###### **Options:**

* `-o`, `--output <FILE>` — Path to write the visualization output.

   For SVG/PNG formats, defaults to a file next to the workflow (e.g., workflow.svg). For DOT format, defaults to stdout.
* `--format <FORMAT>` — Output format for the visualization

  Default value: `svg`

  Possible values:
  - `dot`:
    DOT graph format
  - `svg`:
    SVG image format
  - `png`:
    PNG image format

* `--no-servers` — Hide component server information from nodes
* `--no-details` — Hide detailed tooltips and metadata
* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.



