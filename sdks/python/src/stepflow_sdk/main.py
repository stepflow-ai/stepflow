import msgspec
from stepflow_sdk.server import StepflowStdioServer

# Create server instance
server = StepflowStdioServer()

# Example input/output types
class MathInput(msgspec.Struct):
    a: int
    b: int

class MathOutput(msgspec.Struct):
    result: int

# Register a simple addition component
@server.component
def add(input: MathInput) -> MathOutput:
    return MathOutput(result=input.a + input.b)

# Register a simple multiplication component
@server.component
def multiply(input: MathInput) -> MathOutput:
    return MathOutput(result=input.a * input.b)

def main():
    # Start the server
    server.run()

if __name__ == "__main__":
    main()
