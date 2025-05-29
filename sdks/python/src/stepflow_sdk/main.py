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

class CustomComponentInput(msgspec.Struct):
    input_schema: dict
    code: str
    input: dict
    function_name: str = "custom_function"

class CustomComponentOutput(msgspec.Struct):
    result: dict

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

@server.component
def custom_component(input: CustomComponentInput) -> CustomComponentOutput:
    """
    Execute custom code with input validation against a JSON schema.
    
    Args:
        input: Contains input_schema (JSON schema), code (Python code), 
               input (data), and optional function_name
    
    Returns:
        CustomComponentOutput with the result
    """
    import jsonschema
    import json
    
    try:
        # Validate input against the schema
        jsonschema.validate(input.input, input.input_schema)
    except jsonschema.ValidationError as e:
        raise ValueError(f"Input validation failed: {e.message}")
    except jsonschema.SchemaError as e:
        raise ValueError(f"Invalid schema: {e.message}")
    
    # Create a safe execution environment
    safe_globals = {
        '__builtins__': {
            'len': len,
            'str': str,
            'int': int,
            'float': float,
            'bool': bool,
            'list': list,
            'dict': dict,
            'tuple': tuple,
            'set': set,
            'range': range,
            'sum': sum,
            'min': min,
            'max': max,
            'abs': abs,
            'round': round,
            'sorted': sorted,
            'reversed': reversed,
            'enumerate': enumerate,
            'zip': zip,
            'map': map,
            'filter': filter,
            'any': any,
            'all': all,
            'print': print,
        },
        'json': json,
        'math': __import__('math'),
        're': __import__('re'),
    }
    
    # Check if code defines a function or is a function body
    code_lines = input.code.strip().split('\n')
    has_function_def = any(line.strip().startswith('def ') for line in code_lines)
    
    if has_function_def:
        # Code contains function definition(s)
        local_scope = {}
        try:
            exec(input.code, safe_globals, local_scope)
        except Exception as e:
            raise ValueError(f"Code execution failed: {e}")
        
        # Look for the specified function
        if input.function_name not in local_scope:
            raise ValueError(f"Function '{input.function_name}' not found in code")
        
        func = local_scope[input.function_name]
        if not callable(func):
            raise ValueError(f"'{input.function_name}' is not a function")
        
        try:
            result = func(input.input)
        except Exception as e:
            raise ValueError(f"Function execution failed: {e}")
    else:
        # Code is a function body - wrap it in a lambda or function
        try:
            # Try as expression first (for simple cases)
            wrapped_code = f"lambda data: {input.code}"
            func = eval(wrapped_code, safe_globals)
            result = func(input.input)
        except:
            # If that fails, try as statements in a function body
            try:
                # Properly indent each line of the code
                indented_lines = []
                for line in input.code.split('\n'):
                    if line.strip():  # Only indent non-empty lines
                        indented_lines.append('    ' + line)
                    else:
                        indented_lines.append('')  # Keep empty lines as-is
                
                func_code = f"""def _temp_func(data):
{chr(10).join(indented_lines)}"""
                local_scope = {}
                exec(func_code, safe_globals, local_scope)
                result = local_scope['_temp_func'](input.input)
            except Exception as e:
                raise ValueError(f"Code execution failed: {e}")
    
    # Ensure result is JSON serializable
    try:
        json.dumps(result)
    except (TypeError, ValueError) as e:
        raise ValueError(f"Result is not JSON serializable: {e}")
    
    return CustomComponentOutput(result=result)

def main():
    # Start the server
    server.run()

if __name__ == "__main__":
    main()
