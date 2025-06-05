import sys
import msgspec
import inspect
from typing import Any, List
from stepflow_sdk.server import StepflowStdioServer
from stepflow_sdk.context import StepflowContext

# Create server instance
server = StepflowStdioServer()

# Global cache for compiled functions by blob_id
_function_cache = {}

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

class UdfInput(msgspec.Struct):
    blob_id: str
    input: dict

# Updated input type that can handle nested data
class NestedDataInput(msgspec.Struct):
    data: dict  # Can contain nested structure    

# Filter data by field value
class FilterInput(msgspec.Struct):
    data: List[dict]
    field: str
    value: str

class FilterOutput(msgspec.Struct):
    filtered_data: List[dict]
    filtered_count: int

# Compare two regions (for the simple West vs East analysis)
class CompareRegionsInput(msgspec.Struct):
    west_metrics: dict
    east_metrics: dict

class CompareRegionsOutput(msgspec.Struct):
    comparison: dict

# Enhanced metrics formatting for regional analysis
class EnhancedMetricsInput(msgspec.Struct):
    total_revenue: float
    sales_count: int
    average_sale: float
    performance_ratio: float
    target_revenue: float
    regional_comparison: dict

# Compare all regional metrics
class CompareAllRegionsInput(msgspec.Struct):
    west_metrics: dict
    east_metrics: dict
    north_metrics: dict
    south_metrics: dict

class CompareAllRegionsOutput(msgspec.Struct):
    analysis: dict        

class ComprehensiveSummaryInput(msgspec.Struct):
    total_revenue: float
    sales_count: int
    average_sale: float
    performance_ratio: float
    target_revenue: float
    regional_comparison: dict

class ComprehensiveSummaryOutput(msgspec.Struct):
    summary: str

# Filter data by field value
@server.component
def filter_by_field(input: FilterInput) -> FilterOutput:
    filtered = [item for item in input.data if item.get(input.field) == input.value]
    return FilterOutput(
        filtered_data=filtered,
        filtered_count=len(filtered)
    )

# Compare two regions (West vs East)
@server.component
def compare_regions(input: CompareRegionsInput) -> CompareRegionsOutput:
    west_rev = input.west_metrics.get("revenue", 0)
    east_rev = input.east_metrics.get("revenue", 0)
    west_count = input.west_metrics.get("count", 0)
    east_count = input.east_metrics.get("count", 0)
    
    total_rev = west_rev + east_rev
    
    comparison = {
        "west_revenue": west_rev,
        "east_revenue": east_rev,
        "west_count": west_count,
        "east_count": east_count,
        "west_percentage": (west_rev / total_rev * 100) if total_rev > 0 else 0,
        "leading_region": "West" if west_rev > east_rev else "East",
        "revenue_gap": abs(west_rev - east_rev)
    }
    
    return CompareRegionsOutput(comparison=comparison)

# Enhanced metrics formatting
@server.component
def format_enhanced_metrics(input: EnhancedMetricsInput) -> SummaryResult:
    regional = input.regional_comparison
    
    summary = f"""📊 Sales Performance Analysis with Regional Breakdown:

🎯 Overall Performance:
• Total Revenue: ${input.total_revenue:,.2f}
• Sales Count: {input.sales_count} transactions
• Average Sale: ${input.average_sale:,.2f}
• Target: ${input.target_revenue:,.2f}
• Performance vs Target: {input.performance_ratio:.1f}%
• Status: {"✅ EXCEEDED TARGET" if input.performance_ratio > 100 else "❌ BELOW TARGET"}

🌍 Regional Breakdown (West vs East):
• West Region: ${regional['west_revenue']:,.2f} ({regional['west_percentage']:.1f}%) from {regional['west_count']} sales
• East Region: ${regional['east_revenue']:,.2f} ({100-regional['west_percentage']:.1f}%) from {regional['east_count']} sales
• Leading Region: {regional['leading_region']}
• Revenue Gap: ${regional['revenue_gap']:,.2f}

🔍 Analysis Needed:
1. Why is {regional['leading_region']} outperforming the other region?
2. What strategies can improve the underperforming region?
3. How can we replicate success across regions?
4. What are the specific action items for sales management?

Please provide strategic insights and actionable recommendations based on this comprehensive analysis."""

    return SummaryResult(summary=summary)

@server.component
def compare_all_regions(input: CompareAllRegionsInput) -> CompareAllRegionsOutput:
    regions = {
        "West": input.west_metrics,
        "East": input.east_metrics, 
        "North": input.north_metrics,
        "South": input.south_metrics
    }
    
    # Calculate totals and rankings
    total_revenue = sum(region.get("revenue", 0) for region in regions.values())
    total_count = sum(region.get("count", 0) for region in regions.values())
    
    # Rank regions by revenue
    ranked_regions = sorted(regions.items(), key=lambda x: x[1].get("revenue", 0), reverse=True)
    
    # Calculate percentages and insights
    region_analysis = {}
    for region_name, metrics in regions.items():
        revenue = metrics.get("revenue", 0)
        count = metrics.get("count", 0)
        avg = metrics.get("average", revenue / count if count > 0 else 0)
        
        region_analysis[region_name] = {
            "revenue": revenue,
            "count": count,
            "average_sale": avg,
            "revenue_percentage": (revenue / total_revenue * 100) if total_revenue > 0 else 0,
            "count_percentage": (count / total_count * 100) if total_count > 0 else 0
        }
    
    analysis = {
        "regional_breakdown": region_analysis,
        "rankings": {
            "by_revenue": [{"region": name, "revenue": metrics.get("revenue", 0)} 
                          for name, metrics in ranked_regions],
            "top_performer": ranked_regions[0][0] if ranked_regions else None,
            "bottom_performer": ranked_regions[-1][0] if ranked_regions else None
        },
        "insights": {
            "total_revenue": total_revenue,
            "total_transactions": total_count,
            "region_count": len([r for r in regions.values() if r.get("count", 0) > 0]),
            "revenue_concentration": region_analysis.get(ranked_regions[0][0], {}).get("revenue_percentage", 0) if ranked_regions else 0
        }
    }
    
    return CompareAllRegionsOutput(analysis=analysis)

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

# Format method for the comprehensive summary response
@server.component
def format_comprehensive_summary(input: ComprehensiveSummaryInput) -> ComprehensiveSummaryOutput:
    regional_data = input.regional_comparison.get("regional_breakdown", {})
    rankings = input.regional_comparison.get("rankings", {})
    insights = input.regional_comparison.get("insights", {})
    
    # Build regional breakdown text
    regional_text = []
    for region, data in regional_data.items():
        regional_text.append(
            f"• {region}: ${data.get('revenue', 0):,.2f} ({data.get('revenue_percentage', 0):.1f}%) "
            f"from {data.get('count', 0)} sales (avg: ${data.get('average_sale', 0):,.2f})"
        )
    
    summary = f"""📊 COMPREHENSIVE SALES PERFORMANCE ANALYSIS

🎯 OVERALL PERFORMANCE:
• Total Revenue: ${input.total_revenue:,.2f}
• Sales Transactions: {input.sales_count}
• Average Sale Value: ${input.average_sale:,.2f}
• Target Revenue: ${input.target_revenue:,.2f}
• Performance vs Target: {input.performance_ratio:.1f}%
• Status: {"✅ TARGET EXCEEDED" if input.performance_ratio > 100 else "❌ BELOW TARGET"}

🌍 REGIONAL PERFORMANCE BREAKDOWN:
{chr(10).join(regional_text)}

🏆 REGIONAL RANKINGS:
• Top Performer: {rankings.get('top_performer', 'N/A')}
• Needs Attention: {rankings.get('bottom_performer', 'N/A')}
• Revenue Concentration: {insights.get('revenue_concentration', 0):.1f}% in top region

📈 KEY INSIGHTS NEEDED:
1. Why is {rankings.get('top_performer', 'top region')} outperforming others?
2. What strategies can boost {rankings.get('bottom_performer', 'bottom region')} performance?
3. Is the regional distribution healthy for business growth?
4. What are the specific action items for each region?
5. How can we replicate top-performing region's success?

Please provide a detailed strategic analysis with:
- Root cause analysis of regional performance differences
- Specific recommendations for each region
- Overall strategic priorities for the sales organization
- Risk assessment and mitigation strategies"""

    return ComprehensiveSummaryOutput(summary=summary)

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
async def udf(input: UdfInput, context: StepflowContext) -> Any:
    """
    Execute user-defined function (UDF) using cached compiled functions from blobs.
    
    Args:
        input: Contains blob_id (referencing stored code/schema) and input (data)
    
    Returns:
        The result of the UDF execution.
    """
    # Check if we have a cached function for this blob_id
    if input.blob_id in _function_cache:
        print(f"Using cached function for blob_id: {input.blob_id}", file=sys.stderr)
        compiled_func = _function_cache[input.blob_id]['function']
    else:
        print(f"Loading and compiling function for blob_id: {input.blob_id}", file=sys.stderr)
        
        # Get the blob containing the function definition
        try:
            blob_data = await context.get_blob(input.blob_id)
        except Exception as e:
            raise ValueError(f"Failed to retrieve blob {input.blob_id}: {e}")
        
        # Extract code and schema from blob
        if not isinstance(blob_data, dict):
            raise ValueError(f"Blob {input.blob_id} must contain a dictionary")
        
        code = blob_data.get('code')
        input_schema = blob_data.get('input_schema')
        function_name = blob_data.get('function_name')
        
        if not code:
            raise ValueError(f"Blob {input.blob_id} must contain 'code' field")
        if not input_schema:
            raise ValueError(f"Blob {input.blob_id} must contain 'input_schema' field")
        
        # Compile the function with validation built-in
        compiled_func = _compile_function(code, function_name, input_schema, context)
        
        # Cache the compiled function
        _function_cache[input.blob_id] = {
            'function': compiled_func,
            'input_schema': input_schema,
            'function_name': function_name
        }
    
    # Execute the cached function (validation happens inside)
    try:
        if inspect.iscoroutinefunction(compiled_func):
            result = await compiled_func(input.input)
        else:
            result = compiled_func(input.input)
    except Exception as e:
        raise ValueError(f"Function execution failed: {e}")
    
    print(f"Result: {result}", file=sys.stderr)
    return result


def _compile_function(code: str, function_name: str | None, input_schema: dict, context: StepflowContext):
    """
    Compile a function from code string and return the callable with validation.
    """
    import json
    import jsonschema
    
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
            'isinstance': isinstance,
        },
        'json': json,
        'math': __import__('math'),
        're': __import__('re'),
        'context': context,
    }
    
    def validate_input(data):
        """Validate input data against the schema."""
        try:
            jsonschema.validate(data, input_schema)
        except jsonschema.ValidationError as e:
            raise ValueError(f"Input validation failed: {e.message}")
        except jsonschema.SchemaError as e:
            raise ValueError(f"Invalid schema: {e.message}")
    
    if function_name is not None:
        # Code contains function definition(s)
        local_scope = {}
        try:
            exec(code, safe_globals, local_scope)
        except Exception as e:
            raise ValueError(f"Code execution failed: {e}")
        
        # Look for the specified function
        if function_name not in local_scope:
            raise ValueError(f"Function '{function_name}' not found in code")
        
        func = local_scope[function_name]
        if not callable(func):
            raise ValueError(f"'{function_name}' is not a function")
        
        sig = inspect.signature(func)
        params = list(sig.parameters)
        if len(params) == 2 and params[1] == "context":
            # Function expects context as second parameter
            async def wrapper(data):
                validate_input(data)
                if inspect.iscoroutinefunction(func):
                    return await func(data, context)
                else:
                    return func(data, context)
            return wrapper
        else:
            # Function only expects data
            def wrapper(data):
                validate_input(data)
                return func(data)
            return wrapper
    else:
        # Code is a function body - wrap it appropriately
        try:
            # Try as expression first (for simple cases)
            wrapped_code = f"lambda data: {code}"
            func = eval(wrapped_code, safe_globals)
            
            def wrapper(data):
                validate_input(data)
                return func(data)
            return wrapper
        except:
            # If that fails, try as statements in a function body
            try:
                # Properly indent each line of the code
                indented_lines = []
                for line in code.split('\n'):
                    if line.strip():  # Only indent non-empty lines
                        indented_lines.append('    ' + line)
                    else:
                        indented_lines.append('')  # Keep empty lines as-is
                
                func_code = f"""def _temp_func(data, context):
{chr(10).join(indented_lines)}"""
                local_scope = {}
                exec(func_code, safe_globals, local_scope)
                temp_func = local_scope['_temp_func']
                
                # Wrap to always pass context and validate
                async def wrapper(data):
                    validate_input(data)
                    if inspect.iscoroutinefunction(temp_func):
                        return await temp_func(data, context)
                    else:
                        return temp_func(data, context)
                return wrapper
            except Exception as e:
                raise ValueError(f"Code compilation failed: {e}")

def main():
    # Start the server
    server.run()

if __name__ == "__main__":
    main()