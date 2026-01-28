# Changelog

All notable changes to this project will be documented in this file.

## <a id="0.9.0"></a> [Stepflow 0.9.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-0.9.0) - 2026-01-28

This release contains significant enhancements to:

- Batch and Subflow Execution: All items in the batch share a single executor, as do subflows submitted during execution. This should allow better throttling and lower resource usage for many applications.
- Local execution via Python: New Python package (`stepflow-orchestrator`) packages the `stepflow-server` binary into platform-specific wheels for easy installation and execution via `pip`.

### Breaking Changes

- The STDIO transport was removed from the protocol. All Stepflow protocol interactions happen via Streamable HTTP transport.
- Reference schema reworked to support oneOf and easier manipulation of references.
- API and JSON schema heavily reworked to support and simplify generated OpenAPI clients.

### Bug Fixes

- Handle non-flow YAML files gracefully in test command ([#490](https://github.com/stepflow-ai/stepflow/pull/490))

### Features

- Introduce new edge syntax ([#451](https://github.com/stepflow-ai/stepflow/pull/451))
- De-clutter stderr from component servers ([#453](https://github.com/stepflow-ai/stepflow/pull/453))
- Add queue/eval debug primitives with lazy step evaluation ([#459](https://github.com/stepflow-ai/stepflow/pull/459))
- Add static type checking for workflows ([#461](https://github.com/stepflow-ai/stepflow/pull/461))
- Unify single and multi-item run model ([#467](https://github.com/stepflow-ai/stepflow/pull/467))
- Redesign batch execution with pluggable schedulers ([#473](https://github.com/stepflow-ai/stepflow/pull/473))
- Add RunContext for run-scoped bidirectional communication ([#494](https://github.com/stepflow-ai/stepflow/pull/494))
- Subflow execution support in FlowExecutor ([#502](https://github.com/stepflow-ai/stepflow/pull/502))
- Run test cases in batches ([#513](https://github.com/stepflow-ai/stepflow/pull/513))
- Return flow_id in dry_run mode for store_flow endpoint ([#543](https://github.com/stepflow-ai/stepflow/pull/543))
- Add StepflowConfig JSON schema and stdin config input ([#545](https://github.com/stepflow-ai/stepflow/pull/545))
- Restructure orchestrator release to include Python wheels ([#547](https://github.com/stepflow-ai/stepflow/pull/547))

### Refactoring

- Simplify execution to use lazy step evaluation ([#454](https://github.com/stepflow-ai/stepflow/pull/454))
- Eliminate duplicate JsonSchema crate ([#460](https://github.com/stepflow-ai/stepflow/pull/460))
- Deduplicate StepflowConfig definitions ([#471](https://github.com/stepflow-ai/stepflow/pull/471))
- Remove debugging API / REPL ([#480](https://github.com/stepflow-ai/stepflow/pull/480))
- Replace STDIO transport with unified HTTP transport ([#486](https://github.com/stepflow-ai/stepflow/pull/486))
- Rename stepflow-py package to stepflow-server ([#481](https://github.com/stepflow-ai/stepflow/pull/481))
- Convert StepflowEnvironment to type map pattern ([#514](https://github.com/stepflow-ai/stepflow/pull/514))
- Remove schema versioning from Flow type ([#517](https://github.com/stepflow-ai/stepflow/pull/517))
- Standardize check scripts with consistent output and -v flag ([#518](https://github.com/stepflow-ai/stepflow/pull/518))
- Use StepId and DTOs uniformly ([#519](https://github.com/stepflow-ai/stepflow/pull/519))
- Reorganize Python SDK "primary" and orchestrator packages ([#523](https://github.com/stepflow-ai/stepflow/pull/523))
- Simplify Diagnostic and Path types for cleaner OpenAPI schemas ([#527](https://github.com/stepflow-ai/stepflow/pull/527))
- Unify test server management with subprocess plugin ([#538](https://github.com/stepflow-ai/stepflow/pull/538))
- Standardize step identification API to use step_id only ([#542](https://github.com/stepflow-ai/stepflow/pull/542))
- Reverse dependency between stepflow-py and stepflow-orchestrator ([#546](https://github.com/stepflow-ai/stepflow/pull/546))

### Style

- Fix formatting issues in validation and codegen files ([#536](https://github.com/stepflow-ai/stepflow/pull/536))

## <a id="0.8.0"></a> [Stepflow 0.8.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.8.0) - 2025-12-15
### Features

- 404 dashboard updates and add initial set of metrics, support for such in crates and config ([#405](https://github.com/stepflow-ai/stepflow/pull/405))
- Http support for langflow integration ([#408](https://github.com/stepflow-ai/stepflow/pull/408))
- Add must_execute field for steps with side effects ([#412](https://github.com/stepflow-ai/stepflow/pull/412))
- Implement dynamic workflow overrides system ([#417](https://github.com/stepflow-ai/stepflow/pull/417))
- Refactor override system to use late binding and unified API ([#418](https://github.com/stepflow-ai/stepflow/pull/418))
- Implement variables and secrets system ([#421](https://github.com/stepflow-ai/stepflow/pull/421))
- Use variables for load_from_db inputs ([#424](https://github.com/stepflow-ai/stepflow/pull/424))
- Switch to uuid v7 ([#425](https://github.com/stepflow-ai/stepflow/pull/425))
- Lb tracing improvement ([#426](https://github.com/stepflow-ai/stepflow/pull/426))
- Standardize validation ([#432](https://github.com/stepflow-ai/stepflow/pull/432))
- Validate variable references and subflows ([#435](https://github.com/stepflow-ai/stepflow/pull/435))
- Defines initial set of six basic request metrics for export from pingora LB into grafana. Closes #427 ([#437](https://github.com/stepflow-ai/stepflow/pull/437))

### Miscellaneous Tasks

- Split CLAUDE.md ([#439](https://github.com/stepflow-ai/stepflow/pull/439))

## <a id="0.7.0"></a> [Stepflow 0.7.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.7.0) - 2025-10-27
### Features

- Use released containers; fix k8s demo ([#379](https://github.com/stepflow-ai/stepflow/pull/379))
- Add stepflow-observability crate ([#386](https://github.com/stepflow-ai/stepflow/pull/386))
- Migrate from tracing to log + fastrace observability ([#387](https://github.com/stepflow-ai/stepflow/pull/387))
- Observability stack added as docker-compose, exporter for OTLP … ([#390](https://github.com/stepflow-ai/stepflow/pull/390))
- OTLP logging support and observability improvements ([#391](https://github.com/stepflow-ai/stepflow/pull/391))
- End-to-end distributed tracing for component servers ([#393](https://github.com/stepflow-ai/stepflow/pull/393))
- Add flow_id to the diagnostic context ([#394](https://github.com/stepflow-ai/stepflow/pull/394))
- Comprehensive distributed tracing integration tests ([#397](https://github.com/stepflow-ai/stepflow/pull/397))
- Use standard OTLP environment variables ([#400](https://github.com/stepflow-ai/stepflow/pull/400))
- Add distributed tracing for batch execution ([#401](https://github.com/stepflow-ai/stepflow/pull/401))

## <a id="0.6.0"></a> [Stepflow 0.6.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.6.0) - 2025-10-15
### Features

- Upgrade langflow integration to 1.6.4 with lfx support ([#370](https://github.com/stepflow-ai/stepflow/pull/370))

### Miscellaneous Tasks

- Update release_rust for multiple images ([#374](https://github.com/stepflow-ai/stepflow/pull/374))

## <a id="0.5.0"></a> [Stepflow 0.5.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.5.0) - 2025-10-09
### Bug Fixes

- Update tracing-subscriber ([#242](https://github.com/stepflow-ai/stepflow/pull/242))

### Documentation

- Demonstrate operations concerns ([#171](https://github.com/stepflow-ai/stepflow/pull/171))
- Update the CLI documentation ([#203](https://github.com/stepflow-ai/stepflow/pull/203))
- Flesh out roadmap a little ([#211](https://github.com/stepflow-ai/stepflow/pull/211))
- Update docs to use stepflow-ai GitHub org ([#212](https://github.com/stepflow-ai/stepflow/pull/212))
- Lanchain mcp post and some concurency fixes resulting from such ([#241](https://github.com/stepflow-ai/stepflow/pull/241))

### Features

- Add schema field to flow ([#194](https://github.com/stepflow-ai/stepflow/pull/194))
- Use blobs for flows ([#195](https://github.com/stepflow-ai/stepflow/pull/195))
- Add extensible metadata to flows and steps ([#210](https://github.com/stepflow-ai/stepflow/pull/210))
- Add a visualize command to the cli ([#221](https://github.com/stepflow-ai/stepflow/pull/221))
- Allow empty flows ([#224](https://github.com/stepflow-ai/stepflow/pull/224))
- Allow Python components/UDFs to signal skip ([#239](https://github.com/stepflow-ai/stepflow/pull/239))
- Include process output in channel errors ([#323](https://github.com/stepflow-ai/stepflow/pull/323))
- Share validation between submit and validate ([#331](https://github.com/stepflow-ai/stepflow/pull/331))
- Enhance error responses with full stack traces and attachments ([#332](https://github.com/stepflow-ai/stepflow/pull/332))
- Improve error reporting with stack traces ([#333](https://github.com/stepflow-ai/stepflow/pull/333))
- Restart stdio subprocess; retry components ([#336](https://github.com/stepflow-ai/stepflow/pull/336))
- Add batch execution support with CLI and REST API ([#345](https://github.com/stepflow-ai/stepflow/pull/345))
- Protocol support for batch execution ([#348](https://github.com/stepflow-ai/stepflow/pull/348))
- Kubernetes + load balancer demo ([#349](https://github.com/stepflow-ai/stepflow/pull/349))
- Separate binaries and update release infrastructure ([#355](https://github.com/stepflow-ai/stepflow/pull/355))

### Miscellaneous Tasks

- Standardize on Stepflow capitalization ([#205](https://github.com/stepflow-ai/stepflow/pull/205))
- All the plumbing, files, and scripts for ICLA setup and maintenance. ([#218](https://github.com/stepflow-ai/stepflow/pull/218))
- License check revamp using correct license headers, configure licensure for such ([#223](https://github.com/stepflow-ai/stepflow/pull/223))
- Update release scripts ([#228](https://github.com/stepflow-ai/stepflow/pull/228))
- Undo dispatch changes ([#235](https://github.com/stepflow-ai/stepflow/pull/235))
- Release stepflow-rs v0.3.0 ([#234](https://github.com/stepflow-ai/stepflow/pull/234))
- Add Langflow to CI ([#243](https://github.com/stepflow-ai/stepflow/pull/243))
- Release stepflow-rs v0.4.0 ([#341](https://github.com/stepflow-ai/stepflow/pull/341))
- Release stepflow-rs v0.5.0 ([#356](https://github.com/stepflow-ai/stepflow/pull/356))
- Remove verify-artifacts step; fix binaries ([#357](https://github.com/stepflow-ai/stepflow/pull/357))
- Release stepflow-rs v0.5.0 ([#358](https://github.com/stepflow-ai/stepflow/pull/358))
- No load balancer for windows ([#359](https://github.com/stepflow-ai/stepflow/pull/359))
- Release stepflow-rs v0.5.0 ([#360](https://github.com/stepflow-ai/stepflow/pull/360))
- Fix release script ([#362](https://github.com/stepflow-ai/stepflow/pull/362))
- Release stepflow-rs v0.5.0 ([#363](https://github.com/stepflow-ai/stepflow/pull/363))
- Change artifact names ([#364](https://github.com/stepflow-ai/stepflow/pull/364))

### Refactoring

- Introduce Step/Flow builders ([#238](https://github.com/stepflow-ai/stepflow/pull/238))

## <a id="0.4.0"></a> [Stepflow 0.4.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.4.0) - 2025-10-02

Skipped -- didn't go out due to bugs in the release script.
Changes will be included in 0.5.0.

## <a id="0.3.0"></a> [Stepflow 0.3.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.3.0) - 2025-08-25
### Documentation

- Demonstrate operations concerns ([#171](https://github.com/stepflow-ai/stepflow/pull/171))
- Update the CLI documentation ([#203](https://github.com/stepflow-ai/stepflow/pull/203))
- Flesh out roadmap a little ([#211](https://github.com/stepflow-ai/stepflow/pull/211))
- Update docs to use stepflow-ai GitHub org ([#212](https://github.com/stepflow-ai/stepflow/pull/212))

### Features

- Add schema field to flow ([#194](https://github.com/stepflow-ai/stepflow/pull/194))
- Use blobs for flows ([#195](https://github.com/stepflow-ai/stepflow/pull/195))
- Add extensible metadata to flows and steps ([#210](https://github.com/stepflow-ai/stepflow/pull/210))
- Add a visualize command to the cli ([#221](https://github.com/stepflow-ai/stepflow/pull/221))
- Allow empty flows ([#224](https://github.com/stepflow-ai/stepflow/pull/224))

### Miscellaneous Tasks

- Standardize on Stepflow capitalization ([#205](https://github.com/stepflow-ai/stepflow/pull/205))
- All the plumbing, files, and scripts for ICLA setup and maintenance. ([#218](https://github.com/stepflow-ai/stepflow/pull/218))
- License check revamp using correct license headers, configure licensure for such ([#223](https://github.com/stepflow-ai/stepflow/pull/223))
- Update release scripts ([#228](https://github.com/stepflow-ai/stepflow/pull/228))

## <a id="0.2.2"></a> [Stepflow 0.2.2](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.2.2) - 2025-07-30
### Miscellaneous Tasks

- Add response logging ([#191](https://github.com/stepflow-ai/stepflow/pull/191))

## <a id="0.2.1"></a> [Stepflow 0.2.1](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.2.1) - 2025-07-29

- Complete removal of plugin prefix (previously partly removed)
- Fix docker image version names

## <a id="0.2.0"></a> [Stepflow 0.2.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.2.0) - 2025-07-29
### Bug Fixes

- Verify status reported during execution ([#117](https://github.com/stepflow-ai/stepflow/pull/117))
- Upgrade to schemars 1.0 ([#119](https://github.com/stepflow-ai/stepflow/pull/119))
- Cleanup Component ([#126](https://github.com/stepflow-ai/stepflow/pull/126))

### Documentation

- Update contributing with link to conventional commits ([#121](https://github.com/stepflow-ai/stepflow/pull/121))
- Use generated schemas in documentation ([#127](https://github.com/stepflow-ai/stepflow/pull/127))
- Added details on error handling with examples to claude and contrib docs ([#146](https://github.com/stepflow-ai/stepflow/pull/146))
- Updates to paths, routes, and invocations to ensure all examples are working ([#161](https://github.com/stepflow-ai/stepflow/pull/161))

### Features

- Standardize naming ([#112](https://github.com/stepflow-ai/stepflow/pull/112))
- Initial basic implementation of MCP server working with… ([#113](https://github.com/stepflow-ai/stepflow/pull/113))
- Plugin config loading and proper URL handling, tool discovery w… ([#116](https://github.com/stepflow-ai/stepflow/pull/116))
- Generate protocol schema from Rust code ([#120](https://github.com/stepflow-ai/stepflow/pull/120))
- Generate and use protocol types ([#122](https://github.com/stepflow-ai/stepflow/pull/122))
- Initial checked-in workflow Schema ([#123](https://github.com/stepflow-ai/stepflow/pull/123))
- Introduce ValueTemplate ([#125](https://github.com/stepflow-ai/stepflow/pull/125))
- Mcp components 3 ([#124](https://github.com/stepflow-ai/stepflow/pull/124))
- Add FlowBuilder and Value API ([#135](https://github.com/stepflow-ai/stepflow/pull/135))
- Support `.` and `[...]` in paths ([#138](https://github.com/stepflow-ai/stepflow/pull/138))
- Add routing rules and router ([#142](https://github.com/stepflow-ai/stepflow/pull/142))
- Change component names URLs to paths ([#144](https://github.com/stepflow-ai/stepflow/pull/144))
- Switch to path-based routing for components ([#145](https://github.com/stepflow-ai/stepflow/pull/145))
- Implement protocol over HTTP+SSE ([#147](https://github.com/stepflow-ai/stepflow/pull/147))
- Allow substitutions in environment variables ([#148](https://github.com/stepflow-ai/stepflow/pull/148))
- Add eval method and demonstrate loop/map ([#153](https://github.com/stepflow-ai/stepflow/pull/153))
- Change to trie-based routing ([#157](https://github.com/stepflow-ai/stepflow/pull/157))
- Replace HTTP+SSE with Streamable HTTP ([#168](https://github.com/stepflow-ai/stepflow/pull/168))
- Added validate to CLI args. Supports both flow and config ([#175](https://github.com/stepflow-ai/stepflow/pull/175))

### Miscellaneous Tasks

- Configure license headers ([#115](https://github.com/stepflow-ai/stepflow/pull/115))
- Debug tracing for blob and value resolver ([#137](https://github.com/stepflow-ai/stepflow/pull/137))
- Setup CI for python ([#139](https://github.com/stepflow-ai/stepflow/pull/139))
- Fix python lints & mypy ([#140](https://github.com/stepflow-ai/stepflow/pull/140))
- Add scripts for the CI checks ([#156](https://github.com/stepflow-ai/stepflow/pull/156))
- Fix cliff.toml

### Refactoring

- Remove unnecessary get_step_result_by_id ([#109](https://github.com/stepflow-ai/stepflow/pull/109))
- Move OutputFormat to list_components.rs ([#110](https://github.com/stepflow-ai/stepflow/pull/110))
- Remove unused components and update examples ([#111](https://github.com/stepflow-ai/stepflow/pull/111))
- Replace PluginError with strongly-typed McpError variant ([#143](https://github.com/stepflow-ai/stepflow/pull/143))
- Move tests from stepflow-rs to top-level ([#150](https://github.com/stepflow-ai/stepflow/pull/150))
- Eliminate unsafe blocks for env tests ([#158](https://github.com/stepflow-ai/stepflow/pull/158))
- Some doc fixes, load test scripts ([#169](https://github.com/stepflow-ai/stepflow/pull/169))
- Update config to camel case consistently ([#170](https://github.com/stepflow-ai/stepflow/pull/170))
- Rename stepflow-sdk to stepflow-py ([#172](https://github.com/stepflow-ai/stepflow/pull/172))

## <a id="0.1.0"></a> [Stepflow 0.1.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-rs-0.1.0) - 2025-06-27
### Bug Fixes

- Input search path and remove unneeded flow_dir variable ([#74](https://github.com/stepflow-ai/stepflow/pull/74))

### Documentation

- Quick fix to remove working_directory from function comment ([#93](https://github.com/stepflow-ai/stepflow/pull/93))

### Miscellaneous Tasks

- Setup release scripts and documentation ([#95](https://github.com/stepflow-ai/stepflow/pull/95))

### Refactoring

- Standardize on `input` naming ([#77](https://github.com/stepflow-ai/stepflow/pull/77))

## <a id="0.0.1"></a> Stepflow 0.0.1 - 2025-06-25
Initial, unreleased version of Stepflow.
