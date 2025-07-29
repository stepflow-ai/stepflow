# Changelog

All notable changes to this project will be documented in this file.

## <a id="0.2.1"></a> [StepFlow 0.2.1](https://github.com/riptano/stepflow/releases/tag/stepflow-rs-0.2.1) - 2025-07-29
### Bug Fixes

- Fix issues with python <3.13 ([#182](https://github.com/riptano/stepflow/pull/182))
- Notification responses ([#184](https://github.com/riptano/stepflow/pull/184))

### Miscellaneous Tasks

- Fix docker image version names

## <a id="0.2.0"></a> [StepFlow 0.2.0](https://github.com/riptano/stepflow/releases/tag/stepflow-rs-0.2.0) - 2025-07-29
### Bug Fixes

- Verify status reported during execution ([#117](https://github.com/riptano/stepflow/pull/117))
- Upgrade to schemars 1.0 ([#119](https://github.com/riptano/stepflow/pull/119))
- Cleanup Component ([#126](https://github.com/riptano/stepflow/pull/126))

### Documentation

- Update contributing with link to conventional commits ([#121](https://github.com/riptano/stepflow/pull/121))
- Use generated schemas in documentation ([#127](https://github.com/riptano/stepflow/pull/127))
- Added details on error handling with examples to claude and contrib docs ([#146](https://github.com/riptano/stepflow/pull/146))
- Updates to paths, routes, and invocations to ensure all examples are working ([#161](https://github.com/riptano/stepflow/pull/161))

### Features

- Standardize naming ([#112](https://github.com/riptano/stepflow/pull/112))
- Initial basic implementation of MCP server working with… ([#113](https://github.com/riptano/stepflow/pull/113))
- Plugin config loading and proper URL handling, tool discovery w… ([#116](https://github.com/riptano/stepflow/pull/116))
- Generate protocol schema from Rust code ([#120](https://github.com/riptano/stepflow/pull/120))
- Generate and use protocol types ([#122](https://github.com/riptano/stepflow/pull/122))
- Initial checked-in workflow Schema ([#123](https://github.com/riptano/stepflow/pull/123))
- Introduce ValueTemplate ([#125](https://github.com/riptano/stepflow/pull/125))
- Mcp components 3 ([#124](https://github.com/riptano/stepflow/pull/124))
- Add FlowBuilder and Value API ([#135](https://github.com/riptano/stepflow/pull/135))
- Support `.` and `[...]` in paths ([#138](https://github.com/riptano/stepflow/pull/138))
- Add routing rules and router ([#142](https://github.com/riptano/stepflow/pull/142))
- Change component names URLs to paths ([#144](https://github.com/riptano/stepflow/pull/144))
- Switch to path-based routing for components ([#145](https://github.com/riptano/stepflow/pull/145))
- Implement protocol over HTTP+SSE ([#147](https://github.com/riptano/stepflow/pull/147))
- Allow substitutions in environment variables ([#148](https://github.com/riptano/stepflow/pull/148))
- Add eval method and demonstrate loop/map ([#153](https://github.com/riptano/stepflow/pull/153))
- Change to trie-based routing ([#157](https://github.com/riptano/stepflow/pull/157))
- Replace HTTP+SSE with Streamable HTTP ([#168](https://github.com/riptano/stepflow/pull/168))
- Added validate to CLI args. Supports both flow and config ([#175](https://github.com/riptano/stepflow/pull/175))

### Miscellaneous Tasks

- Configure license headers ([#115](https://github.com/riptano/stepflow/pull/115))
- Debug tracing for blob and value resolver ([#137](https://github.com/riptano/stepflow/pull/137))
- Setup CI for python ([#139](https://github.com/riptano/stepflow/pull/139))
- Fix python lints & mypy ([#140](https://github.com/riptano/stepflow/pull/140))
- Add scripts for the CI checks ([#156](https://github.com/riptano/stepflow/pull/156))
- Fix cliff.toml

### Refactoring

- Remove unnecessary get_step_result_by_id ([#109](https://github.com/riptano/stepflow/pull/109))
- Move OutputFormat to list_components.rs ([#110](https://github.com/riptano/stepflow/pull/110))
- Remove unused components and update examples ([#111](https://github.com/riptano/stepflow/pull/111))
- Replace PluginError with strongly-typed McpError variant ([#143](https://github.com/riptano/stepflow/pull/143))
- Move tests from stepflow-rs to top-level ([#150](https://github.com/riptano/stepflow/pull/150))
- Eliminate unsafe blocks for env tests ([#158](https://github.com/riptano/stepflow/pull/158))
- Some doc fixes, load test scripts ([#169](https://github.com/riptano/stepflow/pull/169))
- Update config to camel case consistently ([#170](https://github.com/riptano/stepflow/pull/170))
- Rename stepflow-sdk to stepflow-py ([#172](https://github.com/riptano/stepflow/pull/172))

## <a id="0.1.0"></a> [StepFlow 0.1.0](https://github.com/riptano/stepflow/releases/tag/stepflow-rs-0.1.0) - 2025-06-27
### Bug Fixes

- Input search path and remove unneeded flow_dir variable ([#74](https://github.com/riptano/stepflow/pull/74))

### Documentation

- Quick fix to remove working_directory from function comment ([#93](https://github.com/riptano/stepflow/pull/93))

### Miscellaneous Tasks

- Setup release scripts and documentation ([#95](https://github.com/riptano/stepflow/pull/95))

### Refactoring

- Standardize on `input` naming ([#77](https://github.com/riptano/stepflow/pull/77))

## <a id="0.0.1"></a> StepFlow 0.0.1 - 2025-06-25
Initial, unreleased version of StepFlow.
