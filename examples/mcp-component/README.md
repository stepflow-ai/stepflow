# MCP Component Example

This example demonstrates how to use Model Context Protocol (MCP) tools within Stepflow workflows.

## Prerequisites

1. **Node.js**: The example uses an MCP filesystem server distributed via npm
2. **MCP Filesystem Server**: Will be automatically installed when the workflow runs

## Files

- `stepflow-config.yml` - Configuration that sets up an MCP plugin for filesystem operations
- `basic.yaml` - Example workflow that creates, reads, and lists files using MCP tools
- `input1.json` - Sample input data for the workflow

## How to Run

From the `stepflow-rs` directory, run:

```bash
cargo run -- run --flow=../examples/mcp-component/basic.yaml --input=../examples/mcp-component/input1.json --config=../examples/mcp-component/stepflow-config.yml
```

## What This Example Does

1. **Configure MCP Plugin**: The configuration sets up an MCP plugin that connects to the npm-distributed filesystem server
2. **Create a File**: Uses the MCP `write_file` tool to create a file with the specified content
3. **Read the File**: Uses the MCP `read_text_file` tool to read back the created file (with proper dependencies)
4. **List Directory**: Uses the MCP `list_directory` tool to show all files in the `/tmp` directory
5. **Proper Dependencies**: Steps are properly sequenced using `depends_on` to ensure correct execution order

## Understanding MCP Component Paths

MCP tools are referenced using Stepflow's plugin-based path format:
```
/plugin_name/tool_name
```

Where `plugin_name` corresponds to the name configured in the Stepflow config file. This follows Stepflow's standard plugin architecture where each plugin registers with a name that becomes part of the path.

For example:
- `/filesystem/write_file` - The `write_file` tool from the `filesystem` MCP server
- `/filesystem/read_file` - The `read_file` tool from the `filesystem` MCP server

## Expected Output

The workflow will output:
- `file_path`: The path to the created file
- `file_content`: The content that was read back from the created file  
- `directory_listing`: A list of files in the `/tmp` directory
- `success_message`: A formatted success message

## Adding More MCP Servers

You can add additional MCP servers to the configuration. For example, to add a Brave Search MCP server:

```yaml
plugins:
  brave-search:
    type: mcp
    command: npx
    args:
      - "-y"
      - "@modelcontextprotocol/server-brave-search"
    env:
      BRAVE_API_KEY: "your-api-key-here"

routes:
  "/brave-search/{*component}":
    - plugin: brave-search
```

Then use it in workflows like:
```yaml
search_step:
  component: "/brave-search/brave_web_search"
  input:
    query: "Stepflow workflow engine"
```

## Key Features Demonstrated

- **MCP Integration**: Shows how Stepflow integrates with MCP-compatible tool servers
- **Bidirectional Communication**: The MCP protocol handles server-to-client communication automatically
- **Proper Dependencies**: Steps use `depends_on` to ensure correct execution order
- **Error Handling**: MCP tool failures are handled gracefully by the workflow engine

## Troubleshooting

- **Command not found**: Make sure `npx` is available in your PATH
- **Permission errors**: Ensure you have write permissions to `/tmp`
- **Tool not found**: Verify the MCP server is properly configured and the tool names match those provided by the server
- **Execution failures**: Check that the MCP server supports bidirectional communication (this example works with the latest filesystem server)