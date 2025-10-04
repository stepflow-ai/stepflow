---
sidebar_position: 7
---

# Command-Line Help for `list-components`

This document contains the help content for the `list-components` command-line program.

**Command Overview:**

* [`list-components`↴](#list-components)

## `list-components`

List all available components from a stepflow config.

List all available components from the configured plugins.

# Examples

```bash

# List components in pretty format

stepflow list-components

# List components with JSON output including schemas

stepflow list-components --format=json

# List components from specific config without schemas

stepflow list-components --config=my-config.yml --format=yaml --schemas=false

# Show all components including unreachable ones

stepflow list-components --hide-unreachable=false

```

**Usage:** `list-components [OPTIONS]`

###### **Options:**

* `--config <FILE>` — The path to the stepflow config file.

   If not specified, will look for `stepflow-config.yml` in the directory containing the workflow file. If that isn't found, will also look in the current directory.
* `--format <FORMAT>` — Output format for the component list

  Default value: `pretty`

  Possible values: `pretty`, `json`, `yaml`

* `--schemas <SCHEMAS>` — Include component schemas in output.

   Defaults to false for pretty format, true for json/yaml formats.

  Possible values: `true`, `false`

* `--hide-unreachable <HIDE_UNREACHABLE>` — Hide components that are not reachable through any routing rule.

   Use --no-hide-unreachable to show all components regardless of routing.

  Default value: `true`

  Possible values: `true`, `false`




