import msgspec
from stepflow_sdk.server import StepflowStdioServer

# Create server instance
server = StepflowStdioServer()

# Example input/output types
class AddInput(msgspec.Struct):
    a: int
    b: int

class AddOutput(msgspec.Struct):
    result: int

# Register a simple addition component
@server.component
def add(input: AddInput) -> AddOutput:
    return AddOutput(result=input.a + input.b)

def main():
    # Start the server
    server.run()

if __name__ == "__main__":
    main()
