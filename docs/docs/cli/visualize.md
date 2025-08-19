---
sidebar_position: 9
---

# Command-Line Help for `visualize`

This document contains the help content for the `visualize` command-line program.

**Command Overview:**

* [`visualize`↴](#visualize)

## `visualize`

Visualize workflow structure as a graph.

Generate a visual representation of workflow structure showing steps, dependencies, and component routing. Supports multiple output formats (DOT, SVG, PNG) with optional features like component server coloring and detailed tooltips.

# Examples

```bash

# Generate SVG visualization (default) stepflow visualize --flow=workflow.yaml --output=workflow.svg

# Generate PNG with component server info stepflow visualize --flow=workflow.yaml --output=workflow.png --format=png

# Generate DOT file for custom processing stepflow visualize --flow=workflow.yaml --output=workflow.dot --format=dot

# Output DOT to stdout stepflow visualize --flow=workflow.yaml --format=dot

# Minimal visualization without server details stepflow visualize --flow=workflow.yaml --output=workflow.svg --no-servers

```

**Usage:** `visualize [OPTIONS] --flow <FILE>`

###### **Options:**

* `--flow <FILE>` — Path to the workflow file to visualize
* `-o`, `--output <FILE>` — Path to write the visualization output. If not specified, outputs DOT format to stdout
* `--format <FORMAT>` — Output format for the visualization

  Default value: `svg`
* `--no-servers` — Hide component server information from nodes
* `--no-details` — Hide detailed tooltips and metadata
* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.



