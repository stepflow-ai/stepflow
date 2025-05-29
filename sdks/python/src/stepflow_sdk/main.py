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
class MetricsInput(msgspec.Struct):
    total_revenue: float
    sales_count: int
    average_sale: float
    performance_ratio: float
    target_revenue: float

class SummaryResult(msgspec.Struct):
    summary: str

# Updated input type that can handle nested data
class NestedDataInput(msgspec.Struct):
    data: dict  # Can contain nested structure    

@server.component
def extract_sales_data(input: NestedDataInput) -> ArrayInput:
    # Extract sales_data from the nested structure
    sales_data = input.data.get("sales_data", [])
    return ArrayInput(data=sales_data)

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

@server.component
def format_metrics(input: MetricsInput) -> SummaryResult:
    summary = f"""Sales Performance Analysis:

📊 Key Metrics:
• Total Revenue: ${input.total_revenue:,.2f}
• Sales Count: {input.sales_count} transactions
• Average Sale Value: ${input.average_sale:,.2f}
• Target Revenue: ${input.target_revenue:,.2f}
• Performance vs Target: {input.performance_ratio:.1f}%

🎯 Performance Status: {"✅ EXCEEDED TARGET" if input.performance_ratio > 100 else "❌ BELOW TARGET"}

Please analyze this sales data and provide:
1. Key insights about our sales performance
2. What the metrics reveal about our business
3. Specific recommendations for improving sales
4. Any concerning trends or positive highlights

Focus on actionable business insights that would help a sales manager make strategic decisions."""

    return SummaryResult(summary=summary)

def main():
    # Start the server
    server.run()

if __name__ == "__main__":
    main()
