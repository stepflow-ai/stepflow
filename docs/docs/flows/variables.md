---
sidebar_position: 5
---

# Variables

Variables provide a way to parameterize workflows with values that can be supplied at runtime, making workflows reusable across different environments (development, staging, production) without modifying the workflow definition itself.

:::tip When to Use Overrides vs. Variables
**Overrides** are best for **ad-hoc or impromptu changes** during development, testing, or debugging:
- Testing different component implementations
- Skipping expensive steps during development
- Debugging with fixed values
- A/B testing variations

**Variables** are better for **intentional, planned configuration** that varies by environment:
- API endpoints and credentials
- Environment-specific settings (dev/staging/prod)
- Feature flags and resource limits

See [Overrides](./overrides.md) for execution-time changes to an existing flow.
:::

## Overview

Variables enable:
- **Environment-specific configuration**: Different values for dev, staging, and production
- **Reusable workflows**: Same workflow definition with different parameters
- **Secret management**: Secure handling of sensitive configuration
- **Runtime flexibility**: Supply values when executing workflows

Variables are defined in a schema and provided at runtime via:
- Command-line arguments (`--variables` or `--variables-json`)
- Environment variables
- Configuration files

## Defining Variables Schema

Define a variables schema to specify what variables your workflow expects:

```yaml
# workflow.yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "Production API Workflow"
description: "Workflow that uses environment-specific variables"

variables_schema:
  type: object
  properties:
    api_endpoint:
      type: string
      description: "API endpoint URL"
    api_key:
      type: string
      is_secret: true
      description: "API authentication key"
    timeout_seconds:
      type: integer
      default: 30
      description: "Request timeout in seconds"
    retry_attempts:
      type: integer
      default: 3
      description: "Number of retry attempts"
    environment:
      type: string
      enum: ["development", "staging", "production"]
      description: "Deployment environment"
  required: ["api_endpoint", "api_key", "environment"]

steps:
  - id: call_api
    component: /http/request
    input:
      url: { $from: { variable: api_endpoint } }
      headers:
        Authorization: 
          $from: { variable: api_key }
          transform: "Bearer " + x
      timeout: { $from: { variable: timeout_seconds } }
```

### External Variables Schema

For complex schemas, use an external file:

```yaml
# workflow.yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "Data Processing Pipeline"
variables_schema: "./variables-schema.yaml"

steps:
  - id: connect_db
    component: /python/database_connector
    input:
      connection_string: { $from: { variable: database_url } }
```

```yaml
# variables-schema.yaml
type: object
properties:
  database_url:
    type: string
    is_secret: true
    description: "Database connection string"
  batch_size:
    type: integer
    default: 100
    description: "Number of records to process per batch"
  max_workers:
    type: integer
    default: 4
    description: "Maximum number of parallel workers"
  log_level:
    type: string
    enum: ["DEBUG", "INFO", "WARNING", "ERROR"]
    default: "INFO"
    description: "Logging level"
required: ["database_url"]
```

## Referencing Variables

Use the `$from` expression syntax to reference variables in your workflow:

```yaml
steps:
  - id: configure_service
    component: /python/service_config
    input:
      # Simple variable reference
      endpoint: { $from: { variable: api_endpoint } }
      
      # Variable with transformation
      auth_header:
        $from: { variable: api_key }
        transform: "Bearer " + x
      
      # Variable with default fallback
      timeout: { $from: { variable: timeout_seconds } }
      
      # Nested in objects
      config:
        url: { $from: { variable: api_endpoint } }
        retries: { $from: { variable: retry_attempts } }
```

## Providing Variables at Runtime

### CLI: Inline JSON

```bash
stepflow run --flow=workflow.yaml --input=input.json \
  --variables-json '{
    "api_endpoint": "https://api.production.example.com",
    "api_key": "sk-prod-abc123",
    "environment": "production"
  }'
```

### CLI: Inline YAML

```bash
stepflow run --flow=workflow.yaml --input=input.json \
  --variables-yaml '
    api_endpoint: https://api.production.example.com
    api_key: sk-prod-abc123
    environment: production
  '
```

### CLI: Variables File

```yaml
# prod-variables.yaml
api_endpoint: "https://api.production.example.com"
api_key: "sk-prod-abc123"
timeout_seconds: 60
retry_attempts: 5
environment: "production"
```

```bash
stepflow run --flow=workflow.yaml --input=input.json \
  --variables=prod-variables.yaml
```

### Environment-Specific Variable Files

Organize variables by environment:

```bash
# Development
stepflow run --flow=workflow.yaml --input=input.json \
  --variables=variables/dev.yaml

# Staging
stepflow run --flow=workflow.yaml --input=input.json \
  --variables=variables/staging.yaml

# Production
stepflow run --flow=workflow.yaml --input=input.json \
  --variables=variables/prod.yaml
```

## Production Environment Patterns

### Pattern 1: Environment-Based Configuration

Create separate variable files for each environment:

```yaml
# variables/dev.yaml
api_endpoint: "http://localhost:8080"
api_key: "dev-key-12345"
database_url: "sqlite:dev.db"
log_level: "DEBUG"
environment: "development"
cache_enabled: false
```

```yaml
# variables/staging.yaml
api_endpoint: "https://api.staging.example.com"
api_key: "${STAGING_API_KEY}"  # From environment variable
database_url: "${STAGING_DB_URL}"
log_level: "INFO"
environment: "staging"
cache_enabled: true
```

```yaml
# variables/prod.yaml
api_endpoint: "https://api.example.com"
api_key: "${PROD_API_KEY}"  # From environment variable
database_url: "${PROD_DB_URL}"
log_level: "WARNING"
environment: "production"
cache_enabled: true
```

**Usage:**
```bash
# Set environment variables
export PROD_API_KEY="sk-prod-secure-key"
export PROD_DB_URL="postgresql://prod-db:5432/app"

# Run with production variables
stepflow run --flow=workflow.yaml --input=input.json \
  --variables=variables/prod.yaml
```

### Pattern 2: Secrets from Environment Variables

Keep secrets out of version control by using environment variables:

```yaml
# workflow.yaml
variables_schema:
  type: object
  properties:
    openai_api_key:
      type: string
      is_secret: true
    anthropic_api_key:
      type: string
      is_secret: true
    database_password:
      type: string
      is_secret: true
  required: ["openai_api_key"]
```

```bash
# Set secrets in environment
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
export DATABASE_PASSWORD="secure-password"

# Provide via CLI
stepflow run --flow=workflow.yaml --input=input.json \
  --variables-json "{
    \"openai_api_key\": \"$OPENAI_API_KEY\",
    \"anthropic_api_key\": \"$ANTHROPIC_API_KEY\",
    \"database_password\": \"$DATABASE_PASSWORD\"
  }"
```

### Pattern 3: Kubernetes ConfigMaps and Secrets

In Kubernetes deployments, use ConfigMaps for non-sensitive variables and Secrets for sensitive data:

```yaml
# configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: workflow-variables
data:
  variables.yaml: |
    api_endpoint: "https://api.example.com"
    timeout_seconds: 60
    retry_attempts: 5
    environment: "production"
```

```yaml
# secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: workflow-secrets
type: Opaque
stringData:
  api_key: "sk-prod-secure-key"
  database_url: "postgresql://user:pass@db:5432/app"
```

```yaml
# deployment.yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: stepflow-workflow
spec:
  template:
    spec:
      containers:
      - name: stepflow
        image: stepflow:latest
        command:
          - stepflow
          - run
          - --flow=/workflows/workflow.yaml
          - --input=/inputs/input.json
          - --variables=/config/variables.yaml
          - --variables-json
          - |
            {
              "api_key": "$(API_KEY)",
              "database_url": "$(DATABASE_URL)"
            }
        env:
        - name: API_KEY
          valueFrom:
            secretKeyRef:
              name: workflow-secrets
              key: api_key
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: workflow-secrets
              key: database_url
        volumeMounts:
        - name: config
          mountPath: /config
        - name: workflows
          mountPath: /workflows
      volumes:
      - name: config
        configMap:
          name: workflow-variables
      - name: workflows
        configMap:
          name: workflow-definitions
```

### Pattern 4: Dynamic Environment Selection

Use a script to select the appropriate variables file:

```bash
#!/bin/bash
# run-workflow.sh

ENVIRONMENT=${ENVIRONMENT:-development}
VARIABLES_FILE="variables/${ENVIRONMENT}.yaml"

if [ ! -f "$VARIABLES_FILE" ]; then
  echo "Error: Variables file not found: $VARIABLES_FILE"
  exit 1
fi

echo "Running workflow in $ENVIRONMENT environment"
stepflow run \
  --flow=workflow.yaml \
  --input=input.json \
  --variables="$VARIABLES_FILE"
```

```bash
# Development
ENVIRONMENT=development ./run-workflow.sh

# Production
ENVIRONMENT=production ./run-workflow.sh
```

## Variables vs. Overrides vs. Input

Understanding when to use each mechanism is key to building maintainable workflows:

### Variables
**Use for:** Intentional, environment-specific configuration that changes between deployments
- API endpoints and URLs
- Authentication credentials
- Resource limits and timeouts
- Feature flags
- Environment identifiers

**Characteristics:**
- Defined in workflow's `variables_schema`
- Planned and documented configuration
- Consistent across all executions in an environment
- Validated against schema

**Example:**
```yaml
variables_schema:
  properties:
    api_endpoint: { type: string }
    max_retries: { type: integer }
```

### Input
**Use for:** Data that varies with each individual workflow execution
- User requests and queries
- Items to process
- Request-specific parameters
- Business data that changes per execution

**Characteristics:**
- Defined in workflow's `input_schema`
- Different for every workflow run
- Represents the "what" to process
- Validated against schema

**Key distinction:** Variables configure **how** the workflow operates (environment settings), while Input provides **what** the workflow processes (business data).

**Example:**
```yaml
input_schema:
  properties:
    user_id: { type: string }
    query: { type: string }
    items: { type: array }
```

See [Input and Output](./input-output.md) for details.

### Overrides
**Use for:** Ad-hoc, impromptu modifications for testing or debugging
- Changing component implementations temporarily
- Skipping expensive steps during development
- Modifying parameters for A/B testing
- Debugging with fixed values

**Characteristics:**
- Not defined in workflow schema
- Temporary and situational
- Applied at execution time
- Useful for development and testing

**Example:**
```yaml
# debug-overrides.yaml
expensive_step:
  value:
    skip: true
```

See [Runtime Overrides](./overrides.md) for details.

### Comparison Table

| Aspect | Variables | Input | Overrides |
|--------|-----------|-------|-----------|
| **Purpose** | Environment configuration | Business data | Temporary modifications |
| **Defined in** | `variables_schema` | `input_schema` | Not in workflow |
| **Changes** | Per environment | Per execution | Ad-hoc |
| **Validated** | Yes (schema) | Yes (schema) | No |
| **Use case** | API keys, endpoints | User queries, items | Debugging, testing |
| **Intentional** | Yes (planned) | Yes (expected) | No (impromptu) |

## Secret Variables

Mark sensitive variables as secrets to ensure they're redacted in logs:

```yaml
variables_schema:
  type: object
  properties:
    api_key:
      type: string
      is_secret: true
      description: "API authentication key"
    database_password:
      type: string
      is_secret: true
      description: "Database password"
    webhook_secret:
      type: string
      is_secret: true
      description: "Webhook signing secret"
```

**Benefits:**
- Automatic redaction in logs and error messages
- Prevents accidental exposure in debug output
- Works across all Stepflow components

See [Secrets](./secrets.md) for comprehensive secret management documentation.

## Best Practices

### 1. Use Descriptive Names

```yaml
# Good
variables_schema:
  properties:
    openai_api_key: { type: string, is_secret: true }
    max_retry_attempts: { type: integer, default: 3 }
    request_timeout_seconds: { type: integer, default: 30 }

# Avoid
variables_schema:
  properties:
    key: { type: string }
    max: { type: integer }
    timeout: { type: integer }
```

### 2. Provide Defaults for Optional Variables

```yaml
variables_schema:
  properties:
    log_level:
      type: string
      default: "INFO"
      enum: ["DEBUG", "INFO", "WARNING", "ERROR"]
    cache_ttl_seconds:
      type: integer
      default: 3600
    enable_monitoring:
      type: boolean
      default: true
```

### 3. Document Variables

```yaml
variables_schema:
  properties:
    api_endpoint:
      type: string
      description: "Base URL for the API service (e.g., https://api.example.com)"
    batch_size:
      type: integer
      default: 100
      description: "Number of items to process in each batch. Higher values use more memory but may be faster."
```

### 4. Validate Variable Values

```yaml
variables_schema:
  properties:
    environment:
      type: string
      enum: ["development", "staging", "production"]
      description: "Deployment environment"
    port:
      type: integer
      minimum: 1024
      maximum: 65535
      description: "Service port number"
```

### 5. Organize Variables by Purpose

```yaml
# variables-schema.yaml
type: object
properties:
  # API Configuration
  api_endpoint: { type: string }
  api_key: { type: string, is_secret: true }
  api_timeout: { type: integer, default: 30 }
  
  # Database Configuration
  database_url: { type: string, is_secret: true }
  database_pool_size: { type: integer, default: 10 }
  
  # Feature Flags
  enable_caching: { type: boolean, default: true }
  enable_analytics: { type: boolean, default: false }
  
  # Performance Tuning
  max_workers: { type: integer, default: 4 }
  batch_size: { type: integer, default: 100 }
```

### 6. Keep Secrets Out of Version Control

```bash
# .gitignore
variables/prod.yaml
variables/staging.yaml
*.secret.yaml
.env
```

Use environment variables or secret management systems for sensitive values.

## Programmatic Usage (Python SDK)

```python
from stepflow_py import StepflowContext, Flow
import asyncio

async def execute_with_variables():
    async with StepflowContext.from_config("stepflow-config.yml") as ctx:
        flow = Flow.from_file("workflow.yaml")
        
        # Define variables programmatically
        variables = {
            "api_endpoint": "https://api.production.example.com",
            "api_key": "sk-prod-abc123",
            "timeout_seconds": 60,
            "environment": "production"
        }
        
        result = await ctx.evaluate_flow(
            flow=flow,
            input={"query": "Process this data"},
            variables=variables
        )
        
        print(result)

asyncio.run(execute_with_variables())
```

## Related Documentation

- [Secrets](./secrets.md) - Secure handling of sensitive data
- [Runtime Overrides](./overrides.md) - Temporary workflow modifications
- [Input and Output](./input-output.md) - Workflow input/output schemas
- [Configuration](../configuration.md) - Environment variable substitution in configuration
- [Expressions](./expressions.md) - Referencing variables in workflows