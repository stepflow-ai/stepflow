# Building an AI-Powered Research Assistant with Stepflow: Orchestrating LangChain and MCP

*How to combine AI processing with practical tool integration using declarative workflows*

## Introduction

In the rapidly evolving landscape of AI applications, the ability to orchestrate complex workflows that combine language models with practical tools has become essential. Today, we'll explore how Stepflow, a modern workflow orchestration engine, seamlessly integrates LangChain's AI capabilities with the Model Context Protocol (MCP) to create a powerful research assistant.

This isn't just another "Hello World" example – we're building a practical system that generates research questions, analyzes text, creates structured notes, and saves everything to an organized file structure. All orchestrated through a declarative YAML workflow that's easy to understand and modify.

## The Challenge: Bridging AI and Tools

Modern AI applications need more than just language models. They need to:
- Process and analyze information
- Interact with filesystems and databases
- Coordinate multiple AI components
- Handle failures gracefully
- Maintain type safety across boundaries

Traditional approaches often require complex imperative code, making systems hard to maintain and extend. Stepflow takes a different approach with its declarative workflow engine and plugin architecture.

## Architecture Overview

Our research assistant combines three key technologies:

```
┌─────────────────────────────────────────────────────────────┐
│                    Stepflow Orchestrator                     │
│                 (Declarative YAML Workflows)                 │
├─────────────────────────┬───────────────────────────────────┤
│   LangChain Components  │      MCP Components               │
│   (AI Processing)       │   (Tool Integration)              │
├─────────────────────────┼───────────────────────────────────┤
│ • Question Generator    │  • Create Directory               │
│ • Text Analyzer         │  • Write File                     │
│ • Note Generator        │  • Read File                      │
│ • Report Generator      │  • List Directory                 │
└─────────────────────────┴───────────────────────────────────┘
```

### Stepflow: The Orchestration Layer

Stepflow provides the orchestration engine that coordinates everything. Written in Rust for performance and reliability, it offers:
- Declarative workflow definitions
- Automatic parallelization
- Type-safe component communication
- Plugin-based architecture

### LangChain: AI Processing Power

LangChain components handle the intelligent processing:
- Generate contextual research questions
- Analyze and summarize text
- Create structured research notes
- Produce comprehensive reports

### MCP: Bridging to the Real World

The Model Context Protocol enables interaction with external tools:
- Create directory structures
- Save generated content to files
- List and organize outputs
- Interface with any MCP-compatible tool

## Implementation Deep Dive

### The Workflow Definition

Let's look at how we define this workflow declaratively:

```yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "AI Research Assistant"

input_schema:
  type: object
  properties:
    topic:
      type: string
      description: "Research topic to explore"
    initial_context:
      type: string
      description: "Background information"
    output_dir:
      type: string
      default: "/tmp/research"

steps:
  # Generate research questions using AI
  - id: generate_questions
    component: /research/question_generator
    input:
      topic: { $from: { workflow: input }, path: "topic" }
      context: { $from: { workflow: input }, path: "initial_context" }

  # Analyze the context
  - id: analyze_context
    component: /research/text_analyzer
    input:
      text: { $from: { workflow: input }, path: "initial_context" }
      topic: { $from: { workflow: input }, path: "topic" }

  # Save outputs using MCP filesystem tools
  - id: save_questions
    component: /filesystem/write_file
    input:
      path: 
        $template: "{{output_dir}}/research_questions.md"
      content: { $from: { step: generate_questions }, path: "formatted_output" }
```

Notice how steps can reference outputs from previous steps, creating a data flow graph that Stepflow optimizes automatically.

### LangChain Components

Each AI component is a decorated LangChain runnable:

```python
@server.langchain_component(name="research/question_generator")
def create_question_generator():
    """Generate research questions based on topic and context."""
    
    def generate_questions(data):
        topic = data["topic"]
        context = data["context"]
        
        # Generate contextual questions
        questions = [
            f"What are the fundamental principles of {topic}?",
            f"How does {topic} relate to current industry trends?",
            f"What are the main challenges in {topic}?",
            # ... more intelligent generation
        ]
        
        # Add context-specific questions
        if "AI" in context:
            questions.append(f"How does {topic} intersect with AI?")
        
        return {
            "questions": questions,
            "formatted_output": format_as_markdown(questions),
            "metadata": {"count": len(questions)}
        }
    
    return RunnableLambda(generate_questions)
```

### Type Safety with msgspec

Every component uses strongly-typed interfaces:

```python
class QuestionGeneratorInput(msgspec.Struct):
    topic: str
    context: str
    execution_mode: str = "invoke"

class QuestionGeneratorOutput(msgspec.Struct):
    questions: List[str]
    formatted_output: str
    metadata: Dict[str, Any]
```

This ensures type safety across language boundaries – from Rust orchestrator to Python components.

## Running the Research Assistant

Setup is straightforward:

```bash
# Build Stepflow
cd stepflow-rs
cargo build --release

# Run the workflow
./target/release/stepflow run \
  --flow=../examples/research-assistant/workflow.yaml \
  --input=../examples/research-assistant/input_ai_workflows.json \
  --config=../examples/research-assistant/stepflow-config.yml
```

## Real-World Results

When we run this workflow with "AI Workflow Orchestration" as the topic, it generates:

### Research Questions
- What are the fundamental principles of AI Workflow Orchestration?
- How does it relate to current industry trends?
- What are the main challenges and opportunities?
- Who are the key researchers and organizations?
- What are the practical applications?
- How has it evolved over time?
- What future developments are expected?

### Structured Research Notes
The system creates comprehensive notes with:
- Executive summary
- Research framework
- Methodology outline
- Next steps and objectives
- Resources and references

### Complete JSON Report
A structured report combining all artifacts, ready for further processing or integration.

## Key Technical Highlights

### Automatic Parallelization
Stepflow analyzes the workflow DAG and automatically runs independent steps in parallel. Question generation and text analysis execute simultaneously, improving performance without explicit threading code.

### Bidirectional Communication
Components can call back to Stepflow for operations like storing blobs or accessing shared state:

```python
async def component_with_context(input: Input, context: StepflowContext):
    # Store intermediate results
    blob_id = await context.put_blob({"data": result})
    return Output(blob_id=blob_id)
```

### Plugin Architecture
Adding new capabilities is as simple as configuring a new plugin:

```yaml
plugins:
  web_search:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-brave-search"]
    env:
      BRAVE_API_KEY: "${BRAVE_API_KEY}"
```

### Error Handling
Failed steps produce detailed error information while the workflow continues where possible:

```yaml
on_error:
  action: skip  # or: retry, fail
  max_retries: 3
```

## Extending the System

The modular architecture makes extensions straightforward:

### Add Web Search
Integrate Brave Search for real-time information:
```yaml
- id: search_web
  component: /web_search/brave_web_search
  input:
    query: { $from: { step: generate_questions }, path: "questions[0]" }
```

### Connect to LLMs
Add OpenAI or Anthropic for enhanced analysis:
```yaml
- id: deep_analysis
  component: /builtin/openai
  input:
    model: "gpt-4"
    messages: [...]
```

### Database Persistence
Store research in PostgreSQL:
```yaml
- id: save_to_db
  component: /database/insert
  input:
    table: "research_reports"
    data: { $from: { step: create_report } }
```

## Performance Considerations

The system is designed for efficiency:
- **Parallel Execution**: Independent steps run concurrently
- **Lazy Evaluation**: Components execute only when needed
- **Resource Pooling**: Connections and processes are reused
- **Streaming Support**: Large outputs can be streamed

In our tests, a complete research workflow executes in under 2 seconds for typical inputs.

## Lessons Learned

Building this research assistant revealed several insights:

1. **Declarative is Powerful**: YAML workflows are easier to understand and modify than imperative code
2. **Type Safety Matters**: Schema validation catches errors early
3. **Modularity Wins**: Reusable components accelerate development
4. **Orchestration Simplifies**: Complex flows become manageable

## Conclusion

The combination of Stepflow, LangChain, and MCP demonstrates how modern orchestration can simplify AI application development. By separating concerns – orchestration, AI processing, and tool integration – we create systems that are both powerful and maintainable.

This research assistant is just the beginning. The same patterns apply to:
- Data processing pipelines
- Content generation systems
- Automated testing frameworks
- DevOps automation
- Business process automation

The key insight? **Orchestration transforms complexity into clarity.**

## Try It Yourself

The complete code is available in the Stepflow repository:
```bash
git clone https://github.com/stepflow-ai/stepflow
cd stepflow/examples/research-assistant
./run.sh
```

## What's Next?

We're excited about the future of AI orchestration:
- **Visual Workflow Designer**: Drag-and-drop workflow creation
- **Component Marketplace**: Share and discover components
- **Cloud Deployment**: Managed Stepflow service
- **Enhanced Observability**: Real-time monitoring and debugging

Join us in building the future of AI workflow orchestration!

---

*Stepflow is open-source and available at [github.com/stepflow-ai/stepflow](https://github.com/stepflow-ai/stepflow). We welcome contributions and feedback!*