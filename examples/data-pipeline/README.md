# Sales Data Pipeline Example

This example demonstrates a complete data processing pipeline that:

1. **Processes sales data** using Python components
2. **Calculates multiple metrics** in parallel
3. **Shows data flow dependencies** between steps
4. **Demonstrates concurrent execution**

## What This Shows

- **Parallel Processing**: Revenue sum, count, and average calculations run simultaneously
- **Data Dependencies**: Performance ratio waits for total revenue calculation
- **Multi-Component Workflow**: Combines multiple Python components in a pipeline
- **Real-World Use Case**: Practical business data analysis

## Running the Example

### Option 1: Metrics-Only Pipeline (Recommended for demos)
```bash
# From the project root
cargo build

# Run the metrics pipeline (no OpenAI API key needed)
./target/debug/stepflow-main run \
  --flow=examples/data-pipeline/debug-pipeline.yaml \
  --input=examples/data-pipeline/sales-data.json
```

### Option 2: Full AI-Powered Pipeline
```bash
# Set your OpenAI API key
export OPENAI_API_KEY="your-api-key-here"

# Run the complete pipeline with AI insights
./target/debug/stepflow-main run \
  --flow=examples/data-pipeline/pipeline.yaml \
  --input=examples/data-pipeline/sales-data.json

# Or pipe the input
cat examples/data-pipeline/sales-data.json | \
  ./target/debug/stepflow-main run \
  --flow=examples/data-pipeline/pipeline.yaml
```

## Expected Output

### Metrics-Only Pipeline (`debug-pipeline.yaml`)

```json
{
  "total_revenue": 10180.0,
  "sales_count": 8,
  "average_sale": 1272.5,
  "performance_ratio": 127.25,
  "formatted_summary": "Sales Performance Analysis:\n\n📊 Key Metrics:\n• Total Revenue: $10,180.00\n• Sales Count: 8 transactions\n• Average Sale Value: $1,272.50\n• Target Revenue: $8,000.00\n• Performance vs Target: 127.2%\n\n🎯 Performance Status: ✅ EXCEEDED TARGET\n\nPlease analyze this sales data and provide:\n1. Key insights about our sales performance\n2. What the metrics reveal about our business\n3. Specific recommendations for improving sales\n4. Any concerning trends or positive highlights\n\nFocus on actionable business insights that would help a sales manager make strategic decisions."
}
```

### Full AI Pipeline (`pipeline.yaml`)

When OpenAI API quota is available, the full pipeline returns:

```json
{
  "total_revenue": 10180.0,
  "sales_count": 8,
  "average_sale": 1272.5,
  "performance_ratio": 127.25,
  "ai_insights": "Based on your sales performance analysis, you've achieved exceptional results by exceeding your $8,000 target by 27.25%. Here are the key insights:\n\n**Strengths:**\n- Strong average sale value of $1,272.50 indicates quality customer acquisition\n- 127% performance ratio shows effective sales execution\n- Total revenue of $10,180 demonstrates solid market presence\n\n**Recommendations:**\n1. Analyze which products/regions drove the highest performance\n2. Scale successful strategies to maintain this momentum\n3. Consider raising targets to match this new performance baseline\n4. Investigate if this represents sustainable growth or a one-time spike\n\n**Strategic Focus:**\nWith only 8 transactions generating over $10K, focus on increasing transaction volume while maintaining the high average sale value."
}
```

### Performance Analysis

The results show:
- **Total Revenue**: $10,180 (27.25% above target)
- **Sales Efficiency**: High average sale value of $1,272.50
- **Target Achievement**: ✅ Exceeded goal by $2,180
- **Transaction Volume**: 8 sales (focus area for growth)

## How It Works

1. **Input**: Sales records with revenue data and a target revenue goal
2. **Parallel Processing**: Three calculations run simultaneously:
   - Sum all revenue values
   - Count total sales
   - Calculate average sale amount
3. **Dependent Step**: Performance ratio calculation waits for total revenue
4. **Output**: Structured results showing all calculated metrics

## The Data Flow

```
sales_data ──┬── sum_field ──────┐
             ├── count_items      │
             └── average_field    │
                                  │
target_revenue ──────────────────┴── divide ──► performance_ratio
```

This demonstrates how StepFlow automatically handles dependencies and runs independent steps in parallel.

## Files in this Example

- `pipeline.yaml` - Full AI-powered pipeline with OpenAI integration
- `debug-pipeline.yaml` - Metrics-only version (no OpenAI required)
- `sales-data.json` - Sample sales data input
- `stepflow-config.yml` - Plugin configuration for Python components