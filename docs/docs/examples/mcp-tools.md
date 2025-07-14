# Using MCP Tools in StepFlow

[Model Context Protocol (MCP)](https://modelcontextprotocol.io/) provides a standard way for AI systems to interact with external tools and data sources. StepFlow's MCP integration allows you to use any MCP-compatible server as a source of components in your workflows.

## What is MCP?

MCP is an open protocol that standardizes how AI systems connect to external tools. With MCP support in StepFlow, you can:

- Access file systems, databases, and APIs through a unified interface
- Use the growing ecosystem of MCP tools without writing custom components
- Integrate enterprise tools that support the MCP standard

## Configuration

MCP servers are configured as plugins in your `stepflow-config.yml`:

```yaml
plugins:
  # Filesystem MCP server
  - name: fs
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp/workspace"]

  # Database MCP server (example)
  - name: database
    type: mcp
    command: mcp-server-postgres
    args: ["--connection-string", "postgresql://localhost/mydb"]
    env:
      PGPASSWORD: "secret"

  # Multiple MCP servers can be configured
  - name: github
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_TOKEN: "${GITHUB_TOKEN}"
```

### Configuration Options

- **name**: Identifier for the plugin (used in component references)
- **type**: Must be `mcp` for MCP servers
- **command**: Executable to run the MCP server
- **args**: Command-line arguments for the server
- **env**: Environment variables (supports `${VAR}` expansion)

## Using MCP Tools in Workflows

MCP tools are referenced using the format `/plugin_name/tool_name`:

```yaml
name: file_processing
version: "1.0"

inputs:
  content:
    type: string
    description: Content to write to file

# Note: On macOS, use "/private/tmp" instead of "/tmp" due to symlink resolution
steps:
  # Write content to a file
  - name: save_content
    component: /fs/write_file
    inputs:
      path: /tmp/workspace/output.txt
      content: ${{ inputs.content }}

  # Read the file back
  - name: verify_content
    component: /fs/read_file
    inputs:
      path: /tmp/workspace/output.txt

  # List directory contents
  - name: list_files
    component: /fs/list_directory
    inputs:
      path: /tmp/workspace

outputs:
  written_content: ${{ steps.verify_content }}
  files: ${{ steps.list_files }}
```

## Complete Example: Document Processing

Here's a more complex example that demonstrates file operations with MCP:

```yaml
name: document_processor
version: "1.0"

inputs:
  documents:
    type: array
    items:
      type: object
      properties:
        name:
          type: string
        content:
          type: string

steps:
  # Create a working directory
  - name: setup_workspace
    component: /fs/create_directory
    inputs:
      path: /tmp/workspace/docs

  # Process each document
  - name: save_documents
    forEach: ${{ inputs.documents }}
    iteratorName: doc
    steps:
      - name: write_doc
        component: /fs/write_file
        inputs:
          path: /tmp/workspace/docs/${{ doc.name }}.txt
          content: ${{ doc.content }}

  # Generate index file
  - name: create_index
    component: template
    inputs:
      template: |
        # Document Index
        {% for doc in documents %}
        - {{ doc.name }}
        {% endfor %}
      documents: ${{ inputs.documents }}

  # Save the index
  - name: save_index
    component: /fs/write_file
    inputs:
      path: /tmp/workspace/docs/index.md
      content: ${{ steps.create_index }}

  # List all created files
  - name: list_output
    component: /fs/list_directory
    inputs:
      path: /tmp/workspace/docs

outputs:
  files_created: ${{ steps.list_output }}
  index_content: ${{ steps.create_index }}
```

## Available MCP Servers

### Official MCP Servers

1. **Filesystem** (`@modelcontextprotocol/server-filesystem`)
   - Tools: `read_file`, `write_file`, `list_directory`, `create_directory`, `delete_file`
   - Use case: File operations within a sandboxed directory

2. **GitHub** (`@modelcontextprotocol/server-github`)
   - Tools: Repository operations, issue management, PR creation
   - Requires: `GITHUB_TOKEN` environment variable

3. **PostgreSQL** (`@modelcontextprotocol/server-postgres`)
   - Tools: Database queries, schema inspection
   - Requires: Database connection string

### Using NPX for MCP Servers

Many MCP servers are available as npm packages and can be run with `npx`:

```yaml
plugins:
  - name: mcp_tool
    type: mcp
    command: npx
    args: ["-y", "@package/mcp-server-name", "--config", "value"]
```

The `-y` flag automatically confirms package installation, making it suitable for automated environments.

## Discovering Available Tools

To see what tools an MCP server provides, create a simple workflow:

```yaml
name: discover_tools
version: "1.0"

steps:
  # This will fail but show available tools in the error message
  - name: discover
    component: /fs/unknown_tool
    inputs: {}
```

The error message will list all available tools from the MCP server.

## Error Handling

MCP tools can return two types of errors:

1. **Tool Errors**: Expected failures (e.g., file not found)
   ```yaml
   - name: read_config
     component: /fs/read_file
     inputs:
       path: /tmp/workspace/config.json
     continueOnError: true

   - name: use_default
     component: template
     when: ${{ steps.read_config.failed }}
     inputs:
       template: '{"default": true}'
   ```

2. **System Errors**: MCP server failures, communication issues
   - These will stop workflow execution
   - Check server logs for debugging

## Best Practices

### 1. Sandbox File Operations

Always configure filesystem MCP servers with specific directories:

```yaml
# Good: Restricted to specific directory
command: npx
args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp/workspace"]

# Bad: Full system access
args: ["-y", "@modelcontextprotocol/server-filesystem", "/"]
```

### 2. Environment Variables

Use environment variable expansion for sensitive data:

```yaml
env:
  API_KEY: "${API_KEY}"  # Reads from system environment
```

### 3. Tool Validation

MCP tools validate their inputs according to their schemas. Always check the tool's documentation for required parameters.

### 4. Error Recovery

Use `continueOnError` and conditional steps for graceful error handling:

```yaml
- name: try_operation
  component: /mcp_tool/operation
  continueOnError: true

- name: fallback
  when: ${{ steps.try_operation.failed }}
  component: log
  inputs:
    message: "Operation failed, using fallback"
```

## Troubleshooting

### MCP Server Won't Start

1. Check the command and arguments are correct
2. Verify required environment variables are set
3. Run the command manually to see error messages:
   ```bash
   npx -y @modelcontextprotocol/server-filesystem /tmp/workspace
   ```

### Tools Not Found

1. Ensure the MCP server is properly initialized
2. Check server logs for initialization errors
3. Verify the tool name is correct (case-sensitive)

### Permission Errors

1. Ensure the MCP server has necessary permissions
2. For filesystem operations, verify directory permissions
3. Check if security policies restrict operations

## Advanced Usage

### Dynamic Tool Selection

```yaml
steps:
  - name: select_operation
    component: template
    inputs:
      template: "${{ inputs.operation_type }}://{{ inputs.tool_name }}"

  - name: execute
    component: ${{ steps.select_operation }}
    inputs: ${{ inputs.tool_params }}
```

### Batch Operations

```yaml
- name: batch_write
  forEach: ${{ inputs.files }}
  iteratorName: file
  steps:
    - name: write
      component: /fs/write_file
      inputs:
        path: /tmp/workspace/${{ file.name }}
        content: ${{ file.content }}
```
