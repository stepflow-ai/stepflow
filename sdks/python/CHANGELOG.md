# Changelog

All notable changes to this project will be documented in this file.

## <a id="0.2.1"></a> [StepFlow Python SDK 0.2.1](https://github.com/riptano/stepflow/releases/tag/stepflow-py-0.2.1) - 2025-07-29
### Bug Fixes

- Input search path and remove unneeded flow_dir variable ([#74](https://github.com/riptano/stepflow/pull/74))
- Relax requires-python ([#180](https://github.com/riptano/stepflow/pull/180))

### Documentation

- Update contributing with link to conventional commits ([#121](https://github.com/riptano/stepflow/pull/121))
- Use generated schemas in documentation ([#127](https://github.com/riptano/stepflow/pull/127))
- Added details on error handling with examples to claude and contrib docs ([#146](https://github.com/riptano/stepflow/pull/146))
- Updates to paths, routes, and invocations to ensure all examples are working ([#161](https://github.com/riptano/stepflow/pull/161))

### Features

- Generate protocol schema from Rust code ([#120](https://github.com/riptano/stepflow/pull/120))
- Generate and use protocol types ([#122](https://github.com/riptano/stepflow/pull/122))
- Add FlowBuilder and Value API ([#135](https://github.com/riptano/stepflow/pull/135))
- Support `.` and `[...]` in paths ([#138](https://github.com/riptano/stepflow/pull/138))
- Change component names URLs to paths ([#144](https://github.com/riptano/stepflow/pull/144))
- Switch to path-based routing for components ([#145](https://github.com/riptano/stepflow/pull/145))
- Implement protocol over HTTP+SSE ([#147](https://github.com/riptano/stepflow/pull/147))
- Allow substitutions in environment variables ([#148](https://github.com/riptano/stepflow/pull/148))
- Add eval method and demonstrate loop/map ([#153](https://github.com/riptano/stepflow/pull/153))
- Change to trie-based routing ([#157](https://github.com/riptano/stepflow/pull/157))
- Replace HTTP+SSE with Streamable HTTP ([#168](https://github.com/riptano/stepflow/pull/168))
- Added validate to CLI args. Supports both flow and config ([#175](https://github.com/riptano/stepflow/pull/175))

### Miscellaneous Tasks

- Setup release scripts and documentation ([#95](https://github.com/riptano/stepflow/pull/95))
- Update names of release workflows ([#97](https://github.com/riptano/stepflow/pull/97))
- Configure license headers ([#115](https://github.com/riptano/stepflow/pull/115))
- Setup CI for python ([#139](https://github.com/riptano/stepflow/pull/139))
- Fix python lints & mypy ([#140](https://github.com/riptano/stepflow/pull/140))
- Add scripts for the CI checks ([#156](https://github.com/riptano/stepflow/pull/156))
- Update release process for python ([#173](https://github.com/riptano/stepflow/pull/173))
- Change action label to stepflow-py
- Use gh token for checkout
- Set GITHUB_TOUKEN
- Fix cliff.toml
- Release stepflow-py v0.2.0 ([#179](https://github.com/riptano/stepflow/pull/179))

### Refactoring

- Standardize on `input` naming ([#77](https://github.com/riptano/stepflow/pull/77))
- Remove unused components and update examples ([#111](https://github.com/riptano/stepflow/pull/111))
- Use custom exceptions in python SDK ([#114](https://github.com/riptano/stepflow/pull/114))
- Some doc fixes, load test scripts ([#169](https://github.com/riptano/stepflow/pull/169))
- Update config to camel case consistently ([#170](https://github.com/riptano/stepflow/pull/170))
- Rename stepflow-sdk to stepflow-py ([#172](https://github.com/riptano/stepflow/pull/172))

## <a id="0.2.0"></a> [StepFlow Python SDK 0.2.0](https://github.com/riptano/stepflow/releases/tag/stepflow-py-0.2.0) - 2025-07-29
### Bug Fixes

- Input search path and remove unneeded flow_dir variable ([#74](https://github.com/riptano/stepflow/pull/74))

### Documentation

- Update contributing with link to conventional commits ([#121](https://github.com/riptano/stepflow/pull/121))
- Use generated schemas in documentation ([#127](https://github.com/riptano/stepflow/pull/127))
- Added details on error handling with examples to claude and contrib docs ([#146](https://github.com/riptano/stepflow/pull/146))
- Updates to paths, routes, and invocations to ensure all examples are working ([#161](https://github.com/riptano/stepflow/pull/161))

### Features

- Generate protocol schema from Rust code ([#120](https://github.com/riptano/stepflow/pull/120))
- Generate and use protocol types ([#122](https://github.com/riptano/stepflow/pull/122))
- Add FlowBuilder and Value API ([#135](https://github.com/riptano/stepflow/pull/135))
- Support `.` and `[...]` in paths ([#138](https://github.com/riptano/stepflow/pull/138))
- Change component names URLs to paths ([#144](https://github.com/riptano/stepflow/pull/144))
- Switch to path-based routing for components ([#145](https://github.com/riptano/stepflow/pull/145))
- Implement protocol over HTTP+SSE ([#147](https://github.com/riptano/stepflow/pull/147))
- Allow substitutions in environment variables ([#148](https://github.com/riptano/stepflow/pull/148))
- Add eval method and demonstrate loop/map ([#153](https://github.com/riptano/stepflow/pull/153))
- Change to trie-based routing ([#157](https://github.com/riptano/stepflow/pull/157))
- Replace HTTP+SSE with Streamable HTTP ([#168](https://github.com/riptano/stepflow/pull/168))
- Added validate to CLI args. Supports both flow and config ([#175](https://github.com/riptano/stepflow/pull/175))

### Miscellaneous Tasks

- Setup release scripts and documentation ([#95](https://github.com/riptano/stepflow/pull/95))
- Update names of release workflows ([#97](https://github.com/riptano/stepflow/pull/97))
- Configure license headers ([#115](https://github.com/riptano/stepflow/pull/115))
- Setup CI for python ([#139](https://github.com/riptano/stepflow/pull/139))
- Fix python lints & mypy ([#140](https://github.com/riptano/stepflow/pull/140))
- Add scripts for the CI checks ([#156](https://github.com/riptano/stepflow/pull/156))
- Update release process for python ([#173](https://github.com/riptano/stepflow/pull/173))
- Change action label to stepflow-py
- Use gh token for checkout
- Set GITHUB_TOUKEN
- Fix cliff.toml

### Refactoring

- Standardize on `input` naming ([#77](https://github.com/riptano/stepflow/pull/77))
- Remove unused components and update examples ([#111](https://github.com/riptano/stepflow/pull/111))
- Use custom exceptions in python SDK ([#114](https://github.com/riptano/stepflow/pull/114))
- Some doc fixes, load test scripts ([#169](https://github.com/riptano/stepflow/pull/169))
- Update config to camel case consistently ([#170](https://github.com/riptano/stepflow/pull/170))
- Rename stepflow-sdk to stepflow-py ([#172](https://github.com/riptano/stepflow/pull/172))

## [Unreleased]

