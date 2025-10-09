# Changelog

All notable changes to this project will be documented in this file.

## <a id="0.5.0"></a> [Stepflow Python SDK 0.5.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-py-0.5.0) - 2025-10-09
### Features

- Protocol support for batch execution ([#348](https://github.com/stepflow-ai/stepflow/pull/348))
- Kubernetes + load balancer demo ([#349](https://github.com/stepflow-ai/stepflow/pull/349))

## <a id="0.4.0"></a> [Stepflow Python SDK 0.4.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-py-0.4.0) - 2025-10-02
### Bug Fixes

- Better future management for process messages ([#325](https://github.com/stepflow-ai/stepflow/pull/325))

### Features

- Allow Python components/UDFs to signal skip ([#239](https://github.com/stepflow-ai/stepflow/pull/239))
- Remove duplicate types ([#240](https://github.com/stepflow-ai/stepflow/pull/240))
- Get Vector Store Rag flow working ([#309](https://github.com/stepflow-ai/stepflow/pull/309))
- Add HTTP protocol support for Langflow component server ([#319](https://github.com/stepflow-ai/stepflow/pull/319))
- Restart stdio subprocess; retry components ([#336](https://github.com/stepflow-ai/stepflow/pull/336))

### Miscellaneous Tasks

- Use workflow rather than repository dispatch ([#237](https://github.com/stepflow-ai/stepflow/pull/237))
- Add Langflow to CI ([#243](https://github.com/stepflow-ai/stepflow/pull/243))

## <a id="0.3.0"></a> [Stepflow Python SDK 0.3.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-py-0.3.0) - 2025-08-25
### Bug Fixes

- Decorator should preserve iscoroutine property ([#190](https://github.com/stepflow-ai/stepflow/pull/190))

### Documentation

- Demonstrate operations concerns ([#171](https://github.com/stepflow-ai/stepflow/pull/171))
- Flesh out roadmap a little ([#211](https://github.com/stepflow-ai/stepflow/pull/211))
- Update docs to use stepflow-ai GitHub org ([#212](https://github.com/stepflow-ai/stepflow/pull/212))

### Features

- Add schema field to flow ([#194](https://github.com/stepflow-ai/stepflow/pull/194))
- Use blobs for flows ([#195](https://github.com/stepflow-ai/stepflow/pull/195))
- Add comprehensive LangChain integration for StepFlow Python SDK ([#174](https://github.com/stepflow-ai/stepflow/pull/174))
- Simplify UDFs using named functions ([#199](https://github.com/stepflow-ai/stepflow/pull/199))
- Add extensible metadata to flows and steps ([#210](https://github.com/stepflow-ai/stepflow/pull/210))
- Initial Stepflow-Langflow integration ([#216](https://github.com/stepflow-ai/stepflow/pull/216))

### Miscellaneous Tasks

- Add response logging ([#191](https://github.com/stepflow-ai/stepflow/pull/191))
- Standardize on Stepflow capitalization ([#205](https://github.com/stepflow-ai/stepflow/pull/205))
- All the plumbing, files, and scripts for ICLA setup and maintenance. ([#218](https://github.com/stepflow-ai/stepflow/pull/218))
- License check revamp using correct license headers, configure licensure for such ([#223](https://github.com/stepflow-ai/stepflow/pull/223))
- Fix stragglers for license update ([#225](https://github.com/stepflow-ai/stepflow/pull/225))
- Update release scripts ([#228](https://github.com/stepflow-ai/stepflow/pull/228))
- Dispatch permission ([#230](https://github.com/stepflow-ai/stepflow/pull/230))
- Dispatch on the release branch ([#233](https://github.com/stepflow-ai/stepflow/pull/233))
- Undo dispatch changes ([#235](https://github.com/stepflow-ai/stepflow/pull/235))

## <a id="0.2.4"></a> [Stepflow Python SDK 0.2.4](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-py-0.2.4) - 2025-07-30
### Bug Fixes

- Enable attribute access in UDFs ([#188](https://github.com/stepflow-ai/stepflow/pull/188))

## <a id="0.2.3"></a> [Stepflow Python SDK 0.2.3](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-py-0.2.3) - 2025-07-29
### Bug Fixes

- Don't send notification responses in stdio transport ([#184](https://github.com/stepflow-ai/stepflow/pull/184))
- Register UDF component in sub-servers (and HTTP transport) ([#185](https://github.com/stepflow-ai/stepflow/pull/185))

## <a id="0.2.2"></a> [Stepflow Python SDK 0.2.2](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-py-0.2.2) - 2025-07-29
### Bug Fixes

- Input search path and remove unneeded flow_dir variable ([#74](https://github.com/stepflow-ai/stepflow/pull/74))
- Relax requires-python ([#180](https://github.com/stepflow-ai/stepflow/pull/180))
- Python version mismatch in release workflow
- Fix issues with python <3.13 ([#182](https://github.com/stepflow-ai/stepflow/pull/182))

### Documentation

- Update contributing with link to conventional commits ([#121](https://github.com/stepflow-ai/stepflow/pull/121))
- Use generated schemas in documentation ([#127](https://github.com/stepflow-ai/stepflow/pull/127))
- Added details on error handling with examples to claude and contrib docs ([#146](https://github.com/stepflow-ai/stepflow/pull/146))
- Updates to paths, routes, and invocations to ensure all examples are working ([#161](https://github.com/stepflow-ai/stepflow/pull/161))

### Features

- Generate protocol schema from Rust code ([#120](https://github.com/stepflow-ai/stepflow/pull/120))
- Generate and use protocol types ([#122](https://github.com/stepflow-ai/stepflow/pull/122))
- Add FlowBuilder and Value API ([#135](https://github.com/stepflow-ai/stepflow/pull/135))
- Support `.` and `[...]` in paths ([#138](https://github.com/stepflow-ai/stepflow/pull/138))
- Change component names URLs to paths ([#144](https://github.com/stepflow-ai/stepflow/pull/144))
- Switch to path-based routing for components ([#145](https://github.com/stepflow-ai/stepflow/pull/145))
- Implement protocol over HTTP+SSE ([#147](https://github.com/stepflow-ai/stepflow/pull/147))
- Allow substitutions in environment variables ([#148](https://github.com/stepflow-ai/stepflow/pull/148))
- Add eval method and demonstrate loop/map ([#153](https://github.com/stepflow-ai/stepflow/pull/153))
- Change to trie-based routing ([#157](https://github.com/stepflow-ai/stepflow/pull/157))
- Replace HTTP+SSE with Streamable HTTP ([#168](https://github.com/stepflow-ai/stepflow/pull/168))
- Added validate to CLI args. Supports both flow and config ([#175](https://github.com/stepflow-ai/stepflow/pull/175))

### Miscellaneous Tasks

- Setup release scripts and documentation ([#95](https://github.com/stepflow-ai/stepflow/pull/95))
- Update names of release workflows ([#97](https://github.com/stepflow-ai/stepflow/pull/97))
- Configure license headers ([#115](https://github.com/stepflow-ai/stepflow/pull/115))
- Setup CI for python ([#139](https://github.com/stepflow-ai/stepflow/pull/139))
- Fix python lints & mypy ([#140](https://github.com/stepflow-ai/stepflow/pull/140))
- Add scripts for the CI checks ([#156](https://github.com/stepflow-ai/stepflow/pull/156))
- Update release process for python ([#173](https://github.com/stepflow-ai/stepflow/pull/173))
- Change action label to stepflow-py
- Use gh token for checkout
- Set GITHUB_TOUKEN
- Fix cliff.toml
- Release stepflow-py v0.2.0 ([#179](https://github.com/stepflow-ai/stepflow/pull/179))
- Release stepflow-py v0.2.1 ([#181](https://github.com/stepflow-ai/stepflow/pull/181))
- Debug python version in release
- Iterate on python release

### Refactoring

- Standardize on `input` naming ([#77](https://github.com/stepflow-ai/stepflow/pull/77))
- Remove unused components and update examples ([#111](https://github.com/stepflow-ai/stepflow/pull/111))
- Use custom exceptions in python SDK ([#114](https://github.com/stepflow-ai/stepflow/pull/114))
- Some doc fixes, load test scripts ([#169](https://github.com/stepflow-ai/stepflow/pull/169))
- Update config to camel case consistently ([#170](https://github.com/stepflow-ai/stepflow/pull/170))
- Rename stepflow-sdk to stepflow-py ([#172](https://github.com/stepflow-ai/stepflow/pull/172))
