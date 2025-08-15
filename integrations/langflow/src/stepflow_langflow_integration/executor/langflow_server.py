"""Stepflow component server for Langflow integration."""

from stepflow_py import StepflowStdioServer

# Handle relative imports when run directly
try:
    from .udf_executor import UDFExecutor
except ImportError:
    import sys
    from pathlib import Path
    
    # Add parent directory to path for direct execution
    sys.path.insert(0, str(Path(__file__).parent.parent))
    from executor.udf_executor import UDFExecutor


class StepflowLangflowServer:
    """Stepflow component server with Langflow UDF execution capabilities."""
    
    def __init__(self):
        """Initialize the Langflow component server."""
        self.server = StepflowStdioServer()
        self.udf_executor = UDFExecutor()
        
        # Register components
        self._register_components()
    
    def _register_components(self) -> None:
        """Register all Langflow components."""
        # Register the main UDF executor component
        from typing import Dict, Any
        from stepflow_py import StepflowContext
        
        @self.server.component(name="udf_executor")
        async def udf_executor(input_data: Dict[str, Any], context: StepflowContext) -> Dict[str, Any]:
            """Execute a Langflow UDF component."""
            return await self.udf_executor.execute(input_data, context)
        
        # TODO: Register native component implementations
        # self.server.component(name="openai_chat", func=self._openai_chat)
        # self.server.component(name="chat_input", func=self._chat_input)
    
    def run(self) -> None:
        """Run the component server."""
        self.server.run()
    
    async def serve(self, host: str = "localhost", port: int = 8000) -> None:
        """Run the component server in HTTP mode.
        
        Args:
            host: Server host
            port: Server port
        """
        # This would require HTTP server implementation
        # For now, fallback to stdio
        self.run()


if __name__ == "__main__":
    """Run the Langflow component server."""
    server = StepflowLangflowServer()
    server.run()