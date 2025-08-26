# AI Research Assistant: Combining LangChain and MCP with Stepflow

This example demonstrates the power of Stepflow's orchestration capabilities by combining LangChain's AI processing with MCP's tool integration to create an automated research assistant.

## Overview

The Research Assistant workflow showcases:
- **LangChain Integration**: AI-powered text analysis, question generation, and report creation
- **MCP Tool Usage**: Filesystem operations for saving and organizing research outputs
- **Workflow Orchestration**: Declarative workflow definition with automatic parallelization
- **Type Safety**: JSON Schema validation throughout the pipeline
- **Modular Architecture**: Reusable components that can be mixed and matched

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Stepflow Orchestrator                     │
├─────────────────────────┬───────────────────────────────────┤
│   LangChain Components  │      MCP Components               │
├─────────────────────────┼───────────────────────────────────┤
│ • Question Generator    │  • Create Directory               │
│ • Text Analyzer         │  • Write File                     │
│ • Note Generator        │  • Read File                      │
│ • Report Generator      │  • List Directory                 │
└─────────────────────────┴───────────────────────────────────┘
```

## Prerequisites

1. **Rust** (for Stepflow engine)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Python 3.11+** with uv
   ```bash
   curl -LsSf https://astral.sh/uv/install.sh | sh
   ```

3. **Node.js** (for MCP filesystem server)
   ```bash
   # Install via your package manager or from nodejs.org
   ```

4. **LangChain dependencies**
   ```bash
   cd ../../sdks/python
   uv add --group dev langchain-core
   ```

## Quick Start

### 1. Build Stepflow

```bash
cd ../../stepflow-rs
cargo build --release
```

### 2. Run the Research Assistant

```bash
# From the stepflow-rs directory
./target/release/stepflow run \
  --flow=../examples/research-assistant/workflow.yaml \
  --input=../examples/research-assistant/input_ai_workflows.json \
  --config=../examples/research-assistant/stepflow-config.yml
```

## Workflow Components

### LangChain Components

#### Question Generator (`/research/question_generator`)
Generates research questions based on a topic and context:
- Creates 8+ targeted research questions
- Adapts questions based on context keywords
- Outputs formatted markdown and structured data

#### Text Analyzer (`/research/text_analyzer`)
Analyzes text for research insights:
- Extracts key points and statistics
- Generates summaries
- Provides formatted analysis reports

#### Note Generator (`/research/note_generator`)
Creates structured research notes:
- Organizes questions by priority
- Defines research methodology
- Outlines next steps and objectives

#### Report Generator (`/research/report_generator`)
Produces comprehensive JSON reports:
- Combines all research artifacts
- Provides executive summary
- Includes methodology and recommendations

### MCP Components

#### Filesystem Operations
- `create_directory`: Creates output directories
- `write_file`: Saves research artifacts
- `list_directory`: Lists created files

## Input Examples

Three example inputs are provided to demonstrate different research scenarios:

### 1. AI Workflow Orchestration
```json
{
  "topic": "AI Workflow Orchestration",
  "initial_context": "...",
  "output_dir": "/tmp/research/ai_workflows"
}
```

### 2. LangChain Integration Patterns
```json
{
  "topic": "LangChain Integration Patterns",
  "initial_context": "...",
  "output_dir": "/tmp/research/langchain"
}
```

### 3. Model Context Protocol
```json
{
  "topic": "Model Context Protocol and Tool Integration",
  "initial_context": "...",
  "output_dir": "/tmp/research/mcp_tools"
}
```

## Output Structure

The workflow creates a structured research directory:

```
/tmp/research/<topic>/
├── research_questions.md    # Generated research questions
├── analysis_summary.md      # Context analysis
├── research_notes.md        # Structured research notes
└── research_report.json    # Comprehensive JSON report
```

## Workflow Execution Flow

1. **Parallel AI Processing**: Questions and analysis run concurrently
2. **Note Generation**: Combines insights from parallel steps
3. **Directory Creation**: Prepares output structure
4. **File Saving**: Persists all artifacts
5. **Report Generation**: Creates final comprehensive report
6. **Directory Listing**: Returns list of created files

## Key Features

### Type Safety
Every component uses `msgspec` structs with JSON Schema validation:
```python
class QuestionGeneratorInput(msgspec.Struct):
    topic: str
    context: str
    execution_mode: str = "invoke"
```

### Declarative Workflow
Steps are defined declaratively with clear dependencies:
```yaml
- id: generate_notes
  component: /research/note_generator
  input:
    topic: { $from: { workflow: input }, path: "topic" }
    questions: { $from: { step: generate_questions }, path: "questions" }
```

### Automatic Parallelization
Independent steps execute concurrently for optimal performance.

### Error Handling
Failed steps are captured with detailed error information, allowing workflows to handle failures gracefully.

## Extending the Example

### Adding New AI Components

1. Create a new LangChain component in `research_server.py`:
```python
@server.langchain_component(name="research/literature_reviewer")
def create_literature_reviewer():
    """Review and summarize academic literature."""
    # Implementation here
```

2. Add the component to your workflow:
```yaml
- id: review_literature
  component: /research/literature_reviewer
  input:
    topic: { $from: { workflow: input }, path: "topic" }
```

### Adding New Tools

1. Configure a new MCP server in `stepflow-config.yml`:
```yaml
plugins:
  web_search:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-brave-search"]
    env:
      BRAVE_API_KEY: "${BRAVE_API_KEY}"
```

2. Use the tool in your workflow:
```yaml
- id: search_web
  component: /web_search/brave_web_search
  input:
    query: { $from: { step: generate_questions }, path: "questions[0]" }
```

## Testing

To test the workflow with different inputs:

```bash
# Test with AI workflows topic
./target/release/stepflow run \
  --flow=../examples/research-assistant/workflow.yaml \
  --input=../examples/research-assistant/input_ai_workflows.json \
  --config=../examples/research-assistant/stepflow-config.yml

# Test with LangChain integration topic
./target/release/stepflow run \
  --flow=../examples/research-assistant/workflow.yaml \
  --input=../examples/research-assistant/input_langchain_integration.json \
  --config=../examples/research-assistant/stepflow-config.yml

# Test with MCP tools topic
./target/release/stepflow run \
  --flow=../examples/research-assistant/workflow.yaml \
  --input=../examples/research-assistant/input_mcp_tools.json \
  --config=../examples/research-assistant/stepflow-config.yml
```

## Viewing Results

After running the workflow, examine the outputs:

```bash
# View generated files
ls -la /tmp/research/ai_workflows/

# Read research questions
cat /tmp/research/ai_workflows/research_questions.md

# View JSON report
cat /tmp/research/ai_workflows/research_report.json | jq .
```

## Performance Considerations

- **Parallel Execution**: Question generation and text analysis run concurrently
- **Caching**: LangChain components can cache results for repeated inputs
- **Streaming**: Large outputs can be streamed (not shown in this example)
- **Resource Management**: MCP servers are managed efficiently by Stepflow

## Troubleshooting

### Common Issues

1. **MCP Server Not Found**
   ```bash
   # Ensure npx is available
   which npx
   # Test MCP server directly
   npx -y @modelcontextprotocol/server-filesystem --help
   ```

2. **LangChain Import Errors**
   ```bash
   cd ../../sdks/python
   uv add langchain-core
   ```

3. **Permission Denied for /tmp**
   ```bash
   # Use a different output directory
   mkdir ~/research_output
   # Update input JSON to use ~/research_output
   ```

### Debug Mode

Run with verbose logging:
```bash
RUST_LOG=debug ./target/release/stepflow run \
  --flow=../examples/research-assistant/workflow.yaml \
  --input=../examples/research-assistant/input_ai_workflows.json \
  --config=../examples/research-assistant/stepflow-config.yml
```

## Blog Post Outline

When writing about this example, consider highlighting:

1. **The Power of Orchestration**: How Stepflow simplifies complex AI workflows
2. **Component Reusability**: Mix and match AI and tool components
3. **Type Safety**: JSON Schema validation prevents runtime errors
4. **Parallel Processing**: Automatic optimization of workflow execution
5. **Extensibility**: Easy to add new AI models or tools
6. **Real-World Application**: Practical research assistant use case

## Next Steps

- **Add Web Search**: Integrate Brave or Google search via MCP
- **Connect to LLMs**: Use OpenAI or Anthropic for enhanced analysis
- **Database Storage**: Save research to PostgreSQL or SQLite
- **API Integration**: Expose the workflow as a REST API
- **UI Development**: Build a web interface for the research assistant
- **Streaming Updates**: Implement SSE for real-time progress

## License

This example is part of the Stepflow project, licensed under Apache 2.0.

## Contributing

Contributions are welcome! Please feel free to submit pull requests with:
- New AI components
- Additional MCP tool integrations
- Improved documentation
- Bug fixes and optimizations

---

*Built with ❤️ using Stepflow, LangChain, and MCP*