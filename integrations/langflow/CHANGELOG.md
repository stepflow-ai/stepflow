# Changelog

All notable changes to this project will be documented in this file.

## <a id="0.4.0"></a> [stepflow-langflow-integration 0.4.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-langflow-0.4.0) - 2026-03-24
### Bug Fixes

- Update release scripts for gRPC-first architecture ([#823](https://github.com/stepflow-ai/stepflow/pull/823))

### Features

- Add StepsNeeded event to journal and SSE stream ([#729](https://github.com/stepflow-ai/stepflow/pull/729))
- Add Protocol Buffers + gRPC pull-based transport (Phase 3-5b) ([#734](https://github.com/stepflow-ai/stepflow/pull/734))
- Convert Python API client from REST to gRPC with HTTP/gRPC multiplexing ([#759](https://github.com/stepflow-ai/stepflow/pull/759))
- Journal task IDs for crash recovery ([#746](https://github.com/stepflow-ai/stepflow/pull/746)) ([#776](https://github.com/stepflow-ai/stepflow/pull/776))
- NATS JetStream task transport ([#740](https://github.com/stepflow-ai/stepflow/pull/740)) ([#792](https://github.com/stepflow-ai/stepflow/pull/792))

### Refactoring

- Remove OpenAPI REST client, replace flow types with msgspec ([#761](https://github.com/stepflow-ai/stepflow/pull/761))
- Remove aide routes, JSON-RPC protocol, and prune DTOs ([#808](https://github.com/stepflow-ai/stepflow/pull/808))
- Remove protocol.json, HTTP transport, and JSON-RPC protocol types ([#815](https://github.com/stepflow-ai/stepflow/pull/815))

## <a id="0.3.2"></a> [stepflow-langflow-integration 0.3.2](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-langflow-0.3.2) - 2026-03-04
### Features

- Add verbose/debug options to stepflow-langflow CLI and fix result display ([#718](https://github.com/stepflow-ai/stepflow/pull/718))

## <a id="0.3.1"></a> [stepflow-langflow-integration 0.3.1](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-langflow-0.3.1) - 2026-03-04
### Bug Fixes

- Resolve ValueExpr 'Multiple matches' deserialization error ([#709](https://github.com/stepflow-ai/stepflow/pull/709))

### Miscellaneous Tasks

- Gate PyPI publish on Docker build success ([#711](https://github.com/stepflow-ai/stepflow/pull/711))

## <a id="0.3.0"></a> [stepflow-langflow-integration 0.3.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-langflow-0.3.0) - 2026-03-02
### Bug Fixes

- Handle plain pandas DataFrame serialization in Langflow executor ([#674](https://github.com/stepflow-ai/stepflow/pull/674))

### Features

- Populate flow variables from environment ([#679](https://github.com/stepflow-ai/stepflow/pull/679))

## <a id="0.2.0"></a> [stepflow-langflow-integration 0.2.0](https://github.com/stepflow-ai/stepflow/releases/tag/stepflow-langflow-0.2.0) - 2026-02-24
### Bug Fixes

- Remove unnecessary prints from integration ([#310](https://github.com/stepflow-ai/stepflow/pull/310))
- Use stepflow-py >= 0.4.0 ([#343](https://github.com/stepflow-ai/stepflow/pull/343))
- Add logging setup to pick up errors from invoked components ([#380](https://github.com/stepflow-ai/stepflow/pull/380))
- Fix langflow test fixture to use Flow.model_dump() instead of yaml.safe_load() ([#535](https://github.com/stepflow-ai/stepflow/pull/535))
- Fix Python 3.13 compatibility for CI ([#601](https://github.com/stepflow-ai/stepflow/pull/601))
- Use commen 3-stage pattern in docker, Move o12y init into integr… ([#603](https://github.com/stepflow-ai/stepflow/pull/603))
- 581 k8s cleanup 2 ([#613](https://github.com/stepflow-ai/stepflow/pull/613))
- Cap numpy<2 on Intel Mac for torch 2.2.2 compatibility ([#622](https://github.com/stepflow-ai/stepflow/pull/622))

### Documentation

- Update langflow integration README ([#317](https://github.com/stepflow-ai/stepflow/pull/317))
- Draft blog post on langflow integration ([#326](https://github.com/stepflow-ai/stepflow/pull/326))
- Update blog post, ensure demo flow works ([#344](https://github.com/stepflow-ai/stepflow/pull/344))
- Blog post on langflow run ([#657](https://github.com/stepflow-ai/stepflow/pull/657))

### Features

- Initial Stepflow-Langflow integration ([#216](https://github.com/stepflow-ai/stepflow/pull/216))
- Remove duplicate types ([#240](https://github.com/stepflow-ai/stepflow/pull/240))
- Eliminate mocking in Langflow integration ([#284](https://github.com/stepflow-ai/stepflow/pull/284))
- Implement session handling for chat memory ([#298](https://github.com/stepflow-ai/stepflow/pull/298))
- Implement session handling for chat memory ([#301](https://github.com/stepflow-ai/stepflow/pull/301))
- Implement File component workflow input mapping for document QA ([#305](https://github.com/stepflow-ai/stepflow/pull/305))
- Get Vector Store Rag flow working ([#309](https://github.com/stepflow-ai/stepflow/pull/309))
- Implement tweaks and use in tests ([#316](https://github.com/stepflow-ai/stepflow/pull/316))
- Add HTTP protocol support for Langflow component server ([#319](https://github.com/stepflow-ai/stepflow/pull/319))
- Improve how results are parsed ([#324](https://github.com/stepflow-ai/stepflow/pull/324))
- Remove fusion and special cases for tools/embeddings ([#330](https://github.com/stepflow-ai/stepflow/pull/330))
- Upgrade langflow integration to 1.6.4 with lfx support ([#370](https://github.com/stepflow-ai/stepflow/pull/370))
- Add batch execution support to langflow integration ([#373](https://github.com/stepflow-ai/stepflow/pull/373))
- Use released containers; fix k8s demo ([#379](https://github.com/stepflow-ai/stepflow/pull/379))
- Migrate from tracing to log + fastrace observability ([#387](https://github.com/stepflow-ai/stepflow/pull/387))
- End-to-end distributed tracing for component servers ([#393](https://github.com/stepflow-ai/stepflow/pull/393))
- Standardize Python SDK logging ([#396](https://github.com/stepflow-ai/stepflow/pull/396))
- Add tracing to Langflow UDF ([#399](https://github.com/stepflow-ai/stepflow/pull/399))
- Http support for langflow integration ([#408](https://github.com/stepflow-ai/stepflow/pull/408))
- Add must_execute field for steps with side effects ([#412](https://github.com/stepflow-ai/stepflow/pull/412))
- Added options to sf-lf http server ([#416](https://github.com/stepflow-ai/stepflow/pull/416))
- Implement dynamic workflow overrides system ([#417](https://github.com/stepflow-ai/stepflow/pull/417))
- Refactor override system to use late binding and unified API ([#418](https://github.com/stepflow-ai/stepflow/pull/418))
- Set is_secret in langflow schemas ([#423](https://github.com/stepflow-ai/stepflow/pull/423))
- Use variables for load_from_db inputs ([#424](https://github.com/stepflow-ai/stepflow/pull/424))
- Introduce new edge syntax ([#451](https://github.com/stepflow-ai/stepflow/pull/451))
- Add static type checking for workflows ([#461](https://github.com/stepflow-ai/stepflow/pull/461))
- Unify single and multi-item run model ([#467](https://github.com/stepflow-ai/stepflow/pull/467))
- 428 k8s refactor ([#483](https://github.com/stepflow-ai/stepflow/pull/483))
- Add core component translation for Langflow integration ([#568](https://github.com/stepflow-ai/stepflow/pull/568))
- Introduces Docling integration component into integrations patt… ([#541](https://github.com/stepflow-ai/stepflow/pull/541))
- Split StateStore and add recovery architecture ([#585](https://github.com/stepflow-ai/stepflow/pull/585))
- Blob references, binary blobs, and automatic blobification ([#612](https://github.com/stepflow-ai/stepflow/pull/612))
- Support direct binary transfer for Blob Service ([#621](https://github.com/stepflow-ai/stepflow/pull/621))
- Make runs API async by default with optional wait ([#630](https://github.com/stepflow-ai/stepflow/pull/630))
- Add release workflow for stepflow-langflow-integration ([#654](https://github.com/stepflow-ai/stepflow/pull/654))
- Replace utoipa with aide + schemars for OpenAPI and JSON Schema ([#652](https://github.com/stepflow-ai/stepflow/pull/652))

### Miscellaneous Tasks

- Add Langflow to CI ([#243](https://github.com/stepflow-ai/stepflow/pull/243))
- Remove unused fixture ([#329](https://github.com/stepflow-ai/stepflow/pull/329))
- Split CLAUDE.md ([#439](https://github.com/stepflow-ai/stepflow/pull/439))
- Remove outdated docker directory. Closes #531 ([#532](https://github.com/stepflow-ai/stepflow/pull/532))
- Update dependencies in skds, integrations, and images to latest releases where relevant ([#564](https://github.com/stepflow-ai/stepflow/pull/564))
- Updates and tweaks for getting latest openrag ingestion flow working on k8s example architecture ([#577](https://github.com/stepflow-ai/stepflow/pull/577))
- Refresh langflow uv.lock ([#607](https://github.com/stepflow-ai/stepflow/pull/607))

### Refactoring

- Eliminate CachedStepflowContext ([#244](https://github.com/stepflow-ai/stepflow/pull/244))
- JSON extraction using stepflow CLI ([#300](https://github.com/stepflow-ai/stepflow/pull/300))
- Always use code loading approach ([#312](https://github.com/stepflow-ai/stepflow/pull/312))
- Simplify environment handling ([#313](https://github.com/stepflow-ai/stepflow/pull/313))
- Replace STDIO transport with unified HTTP transport ([#486](https://github.com/stepflow-ai/stepflow/pull/486))
- Rename stepflow-py package to stepflow-server ([#481](https://github.com/stepflow-ai/stepflow/pull/481))
- Remove schema versioning from Flow type ([#517](https://github.com/stepflow-ai/stepflow/pull/517))
- Reorganize Python SDK "primary" and orchestrator packages ([#523](https://github.com/stepflow-ai/stepflow/pull/523))
- Simplify Diagnostic and Path types for cleaner OpenAPI schemas ([#527](https://github.com/stepflow-ai/stepflow/pull/527))
- Reverse dependency between stepflow-py and stepflow-orchestrator ([#546](https://github.com/stepflow-ai/stepflow/pull/546))
- Add FieldHandler abstraction for template-field transformations ([#625](https://github.com/stepflow-ai/stepflow/pull/625))
- Unify InputHandler/OutputHandler pattern for Langflow type conversions ([#628](https://github.com/stepflow-ai/stepflow/pull/628))

### Testing

- Refactor langflow tests to use shared stepflow server ([#371](https://github.com/stepflow-ai/stepflow/pull/371))
- Add comprehensive file input tests for document QA ([#406](https://github.com/stepflow-ai/stepflow/pull/406))

## [Unreleased]

