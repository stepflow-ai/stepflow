# Stepflow Load Testing

This directory contains load testing setup for Stepflow service using k6. The tests evaluate performance and reliability of different workflow execution patterns with OpenAI API calls.

## Test Scenarios

The load test uses the proper Stepflow API pattern: **POST /flows** (to store workflows and get hashes) → **POST /runs** (to execute by hash).

### 1. Python UDF OpenAI (`python-udf-openai.yaml/.json`)
- Uses Python User-Defined Functions (UDF) to call OpenAI API
- Creates a blob containing Python code that makes OpenAI API calls
- Executes the code using the `/python/udf` component
- Model: `gpt-4o-mini`

### 2. Python Custom Component OpenAI (`python-custom-openai.yaml/.json`)
- Uses a custom Python component server (`openai_custom_server.py`)
- Direct component-to-OpenAI API integration
- Uses structured input/output with msgspec
- Model: `gpt-4o-mini`

### 3. Rust Built-in OpenAI (`rust-builtin-openai.yaml/.json`)
- Uses Stepflow's built-in Rust OpenAI component
- Leverages `/create_messages` and `/openai` built-in components
- Most direct integration path
- Model: `gpt-4o-mini`

**Note**: Both YAML and JSON versions of workflows are provided. The k6 load test uses JSON files for compatibility.

## Prerequisites

1. **k6**: Install k6 for load testing
   ```bash
   # macOS
   brew install k6
   
   # Linux
   sudo gpg -k
   sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
   echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
   sudo apt-get update
   sudo apt-get install k6
   
   # Windows
   choco install k6
   ```

2. **OpenAI API Key**: Set your OpenAI API key
   ```bash
   export OPENAI_API_KEY="your-openai-api-key-here"
   ```

3. **Stepflow Dependencies**: Ensure Python SDK dependencies are available
   ```bash
   cd ../stepflow-rs
   # Python dependencies should be managed by uv as configured
   ```

## Running Load Tests

### Option 1: Standalone Tests (Fast, No OpenAI API required)

**Recommended for basic performance testing without external dependencies.**

1. **Start Standalone Service**:
   ```bash
   cd load-tests
   ./scripts/start-standalone-service.sh
   ```
   This will:
   - Build Stepflow in release mode
   - Start the service on port 7837
   - Use standalone configuration (no OpenAI required)

2. **Run Standalone Tests** (in a separate terminal):
   ```bash
   cd load-tests
   ./scripts/run-single-standalone-tests.sh
   ```
   This tests three workflow types with simple message creation:
   - `rust-builtin-messages` - Direct Rust built-in components
   - `python-custom-messages` - Python custom component server
   - `python-udf-messages` - Python UDF with blob storage

### Option 2: OpenAI Tests (Requires API key)

**For testing with actual OpenAI API calls.**

1. **Set OpenAI API Key**:
   ```bash
   export OPENAI_API_KEY="your-key-here"
   ```

2. **Start OpenAI Service**:
   ```bash
   cd load-tests
   ./scripts/start-service.sh
   ```

3. **Run OpenAI Tests**:
   ```bash
   cd load-tests
   ./scripts/run-single-openai-tests.sh
   ```
   This tests three workflow types with OpenAI GPT-4o-mini calls:
   - `rust-builtin-openai` - Direct Rust built-in OpenAI component
   - `python-custom-openai` - Python custom component with OpenAI
   - `python-udf-openai` - Python UDF with OpenAI integration

### Option 3: Combined Workflow Test (All workflows mixed)

**For testing multiple workflow types simultaneously.**

```bash
cd load-tests
./scripts/run-load-test.sh
```
This runs all workflows randomly mixed together.

### Option 4: Individual Test Control

**Test specific workflow types:**

**Standalone:**
```bash
cd load-tests/scripts
k6 run --env WORKFLOW_TYPE=rust-builtin-messages single-standalone-test.js
k6 run --env WORKFLOW_TYPE=python-custom-messages single-standalone-test.js
k6 run --env WORKFLOW_TYPE=python-udf-messages single-standalone-test.js
```

**OpenAI:**
```bash
cd load-tests/scripts
k6 run --env WORKFLOW_TYPE=rust-builtin-openai single-openai-test.js
k6 run --env WORKFLOW_TYPE=python-custom-openai single-openai-test.js
k6 run --env WORKFLOW_TYPE=python-udf-openai single-openai-test.js
```

### Option 3: Manual Setup

1. **Build and Start Stepflow Service**:
   ```bash
   cd stepflow-rs
   cargo build --release
   cargo run --release -- serve --port=7837 --config=../load-tests/workflows/stepflow-config.yml
   ```

2. **Run k6 Load Test**:
   ```bash
   cd load-tests/scripts
   # Mixed workflow test
   k6 run --env STEPFLOW_URL=http://localhost:7837 load-test.js
   
   # Individual workflow tests
   k6 run --env STEPFLOW_URL=http://localhost:7837 --env WORKFLOW_TYPE=rust-builtin-openai single-workflow-test.js
   ```

## Test Configuration

### Load Test Stages

#### Mixed Workflow Test (load-test.js)
- **Warm up**: 30s with 1 user
- **Ramp up**: 1m to reach 5 users
- **Sustain**: 2m at 5 users
- **Scale up**: 1m to reach 10 users  
- **Sustain**: 2m at 10 users
- **Peak load**: 1m to reach 20 users
- **Peak sustain**: 2m at 20 users
- **Ramp down**: 30s to 0 users

Total test duration: ~9 minutes

#### Individual Workflow Test (single-workflow-test.js) - Aggressive Load Testing
- **Quick warm up**: 10s to 5 users
- **Low load tier**: 20s to 10 users (baseline performance)
- **Medium ramp**: 30s to 25 users → 20s to 50 users
- **Medium load tier**: 30s at 100 users (load impact measurement)
- **High ramp**: 20s to 200 users
- **High load tier**: 30s at 300 users (stress testing)
- **Peak test**: 15s to 500 users (extreme limit finding)
- **Peak sustain**: 20s at 500 users
- **Ramp down**: 10s to 0 users

Total test duration: ~3.5 minutes (aggressive load testing optimized for finding breaking points)

### Performance Thresholds
- **Response Time**: 95% of requests must complete within 30 seconds
- **Failure Rate**: Must be less than 10%

### Test Data
- **20 diverse prompts** covering various topics (AI, technology, health, etc.)
- **Random selection** of prompts and workflows for each request
- **Structured system messages** tailored to each prompt type

## Interpreting Results

### Key Metrics to Monitor

#### Overall Metrics
1. **Average Response Time**: Overall latency per request
2. **95th Percentile Response Time**: Performance under load
3. **Request Rate**: Requests per second handled
4. **Failure Rate**: Percentage of failed requests
5. **HTTP Request Duration**: Time spent in HTTP calls
6. **Iterations**: Total number of test iterations completed

#### Per-Workflow Metrics (Available in mixed tests)
1. **python_udf_response_time**: Response times for Python UDF workflow
2. **python_udf_failure_rate**: Failure rate for Python UDF workflow
3. **python_custom_response_time**: Response times for Python custom component workflow
4. **python_custom_failure_rate**: Failure rate for Python custom component workflow
5. **rust_builtin_response_time**: Response times for Rust built-in workflow
6. **rust_builtin_failure_rate**: Failure rate for Rust built-in workflow
7. **Request distribution**: How many requests went to each workflow type

#### Load-Level Performance Metrics (Individual workflow tests)

**Latency Metrics:**
1. **low_load_latency**: Response times at ≤10 concurrent users (baseline performance)
2. **medium_load_latency**: Response times at ≤100 concurrent users (moderate load impact)
3. **high_load_latency**: Response times at ≤500 concurrent users (extreme stress conditions)

**Throughput Metrics (Request Counters - k6 shows both count and RPS):**
1. **low_load_requests**: Request count and rate at ≤10 concurrent users (baseline throughput)
2. **medium_load_requests**: Request count and rate at ≤100 concurrent users (throughput under load)
3. **high_load_requests**: Request count and rate at ≤500 concurrent users (throughput under extreme stress)

These metrics help identify:
- **Performance scaling**: How latency changes as load increases
- **Throughput limits**: Maximum RPS each workflow can handle at different load levels
- **Breaking points**: Where each integration approach starts failing

### Expected Performance Characteristics

- **Rust Built-in**: Fastest due to direct integration
- **Python Custom**: Moderate performance with component overhead
- **Python UDF**: Potentially slower due to blob creation and Python execution

### Common Performance Bottlenecks

1. **OpenAI API Rate Limits**: May cause failures at high load
2. **Python Process Startup**: UDF and custom components need Python processes
3. **Network Latency**: OpenAI API calls add external dependency latency
4. **Memory Usage**: Large blobs (UDF code) may impact memory
5. **Plugin Communication**: Stdio transport adds serialization overhead

## Customization

### Adjusting Load Patterns
Edit `load-test.js` to modify:
- **User counts**: Change `target` values in `stages`
- **Duration**: Adjust `duration` values for each stage
- **Ramp patterns**: Add/remove stages for different load curves

### Adding Test Prompts
Edit `test-prompts.json` to:
- Add more prompts for variety
- Adjust system messages for different contexts
- Include edge cases (very long/short prompts)

### Modifying Workflows
- Edit workflow YAML files to test different configurations
- Adjust OpenAI parameters (temperature, max_tokens, etc.)
- Add new workflow types by creating additional YAML files

### Different Models
Change the model in workflow files from `gpt-4o-mini` to:
- `gpt-4o`
- `gpt-4-turbo`
- `gpt-3.5-turbo`
- Any other supported OpenAI model

## Troubleshooting

### Service Won't Start
- Check `OPENAI_API_KEY` is set
- Verify port 7837 is available: `lsof -i :7837`
- Check Stepflow builds successfully: `cd stepflow-rs && cargo build`

### Load Test Failures
- Verify service is healthy: `curl http://localhost:7837/health`
- Check OpenAI API key validity
- Monitor OpenAI rate limits and quotas
- Review Stepflow service logs for errors

### Python Component Issues
- Ensure Python SDK dependencies are installed
- Check Python environment: `cd sdks/python && uv run python --version`
- Verify custom server script permissions

### Performance Issues
- Monitor system resources (CPU, memory)
- Check OpenAI API response times
- Consider reducing concurrent users
- Review workflow complexity

## Output Files

Load test results are saved to the `scripts/results/` directory:
- `results_TIMESTAMP.json`: Raw k6 metrics data
- `summary_TIMESTAMP.json`: Test summary statistics

Use k6's built-in analysis tools or import JSON data into your preferred analysis platform for detailed performance analysis.