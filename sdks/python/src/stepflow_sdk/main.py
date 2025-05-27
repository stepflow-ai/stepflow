import msgspec
from typing import List
from stepflow_sdk.server import StepflowStdioServer

# Create server instance
server = StepflowStdioServer()

# Example input/output types
class MathInput(msgspec.Struct):
    a: int
    b: int

class MathOutput(msgspec.Struct):
    result: int
    

class ArrayInput(msgspec.Struct):
    data: List[dict]

class FieldInput(msgspec.Struct):
    data: List[dict]
    field: str

class NumberResult(msgspec.Struct):
    result: float

class CountResult(msgspec.Struct):
    result: int

class DivideInput(msgspec.Struct):
    a: float
    b: float

# Register a simple addition component
@server.component
def add(input: MathInput) -> MathOutput:
    return MathOutput(result=input.a + input.b)

# Sum a specific field across array items
@server.component
def sum_field(input: FieldInput) -> NumberResult:
    total = sum(item.get(input.field, 0) for item in input.data)
    return NumberResult(result=total)

# Count items in array
@server.component  
def count_items(input: ArrayInput) -> CountResult:
    return CountResult(result=len(input.data))

# Calculate average of a field
@server.component
def average_field(input: FieldInput) -> NumberResult:
    values = [item.get(input.field, 0) for item in input.data]
    avg = sum(values) / len(values) if values else 0
    return NumberResult(result=avg)

# Divide two numbers (for ratio calculation)
@server.component
def divide(input: DivideInput) -> NumberResult:
    result = (input.a / input.b * 100) if input.b != 0 else 0  # Convert to percentage
    return NumberResult(result=result)

# Register a simple multiplication component
@server.component
def multiply(input: MathInput) -> MathOutput:
    return MathOutput(result=input.a * input.b)

def main():
    # Start the server
    server.run()

if __name__ == "__main__":
    main()
