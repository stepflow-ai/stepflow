---
sidebar_position: 5
---

# Custom Components

Building custom components is key to extending StepFlow's capabilities. These examples demonstrate how to create reusable components using the Python SDK, TypeScript SDK, and other languages.

## Python SDK Components

### Basic Component Server

A simple Python component server demonstrating the core patterns.

```python
# my_components.py
from stepflow_sdk import StepflowStdioServer, StepflowContext
import msgspec
import datetime
import requests
from typing import Optional

# Create the server instance
server = StepflowStdioServer()

# Define input/output schemas using msgspec
class ProcessDataInput(msgspec.Struct):
    data: dict
    processing_rules: Optional[dict] = None
    include_metadata: bool = True

class ProcessDataOutput(msgspec.Struct):
    processed_data: dict
    metadata: dict
    processing_time_ms: int

# Simple data processing component
@server.component
def process_data(input: ProcessDataInput, context: StepflowContext) -> ProcessDataOutput:
    """Process data according to specified rules with metadata tracking."""
    import time
    start_time = time.time()

    # Apply processing rules
    processed = input.data.copy()
    rules = input.processing_rules or {}

    for key, value in processed.items():
        if isinstance(value, str):
            # Apply string transformations
            if rules.get('uppercase', False):
                processed[key] = value.upper()
            elif rules.get('lowercase', False):
                processed[key] = value.lower()
            elif rules.get('title_case', False):
                processed[key] = value.title()

    # Add metadata if requested
    metadata = {}
    if input.include_metadata:
        metadata = {
            'processed_at': datetime.datetime.now().isoformat(),
            'original_keys': list(input.data.keys()),
            'processed_keys': list(processed.keys()),
            'rules_applied': rules,
            'component_version': '1.0.0'
        }

    processing_time = int((time.time() - start_time) * 1000)

    return ProcessDataOutput(
        processed_data=processed,
        metadata=metadata,
        processing_time_ms=processing_time
    )

# HTTP client component with retries
class HttpRequestInput(msgspec.Struct):
    url: str
    method: str = "GET"
    headers: Optional[dict] = None
    json_data: Optional[dict] = None
    timeout: int = 30
    retries: int = 3

class HttpRequestOutput(msgspec.Struct):
    success: bool
    status_code: int
    data: Optional[dict] = None
    error_message: Optional[str] = None
    response_time_ms: int
    attempts_made: int

@server.component
def http_request(input: HttpRequestInput, context: StepflowContext) -> HttpRequestOutput:
    """Make HTTP requests with automatic retries and error handling."""
    import time
    start_time = time.time()

    headers = input.headers or {}
    headers.setdefault('User-Agent', 'StepFlow-Component/1.0')

    last_error = None

    for attempt in range(input.retries + 1):
        try:
            response = requests.request(
                method=input.method,
                url=input.url,
                headers=headers,
                json=input.json_data,
                timeout=input.timeout
            )

            response_time = int((time.time() - start_time) * 1000)

            # Try to parse JSON response
            try:
                data = response.json() if response.content else None
            except ValueError:
                data = {"text": response.text} if response.text else None

            return HttpRequestOutput(
                success=response.status_code < 400,
                status_code=response.status_code,
                data=data,
                error_message=None if response.status_code < 400 else f"HTTP {response.status_code}",
                response_time_ms=response_time,
                attempts_made=attempt + 1
            )

        except Exception as e:
            last_error = str(e)
            if attempt < input.retries:
                time.sleep(2 ** attempt)  # Exponential backoff
                continue

    response_time = int((time.time() - start_time) * 1000)

    return HttpRequestOutput(
        success=False,
        status_code=0,
        data=None,
        error_message=last_error,
        response_time_ms=response_time,
        attempts_made=input.retries + 1
    )

# Component that uses blob storage
class CacheDataInput(msgspec.Struct):
    key: str
    data: dict
    ttl_minutes: int = 60

class CacheDataOutput(msgspec.Struct):
    blob_id: str
    cache_key: str
    expires_at: str

@server.component
async def cache_data(input: CacheDataInput, context: StepflowContext) -> CacheDataOutput:
    """Cache data using StepFlow's blob storage with metadata."""

    # Create cache entry with metadata
    cache_entry = {
        'data': input.data,
        'cached_at': datetime.datetime.now().isoformat(),
        'ttl_minutes': input.ttl_minutes,
        'expires_at': (datetime.datetime.now() + datetime.timedelta(minutes=input.ttl_minutes)).isoformat(),
        'cache_key': input.key
    }

    # Store in blob storage using context
    blob_id = await context.put_blob(cache_entry)

    return CacheDataOutput(
        blob_id=blob_id,
        cache_key=input.key,
        expires_at=cache_entry['expires_at']
    )

class RetrieveCacheInput(msgspec.Struct):
    blob_id: str

class RetrieveCacheOutput(msgspec.Struct):
    found: bool
    data: Optional[dict] = None
    is_expired: bool = False
    cached_at: Optional[str] = None

@server.component
async def retrieve_cache(input: RetrieveCacheInput, context: StepflowContext) -> RetrieveCacheOutput:
    """Retrieve cached data with expiration checking."""

    try:
        # Get data from blob storage
        cache_entry = await context.get_blob(input.blob_id)

        # Check if expired
        expires_at = datetime.datetime.fromisoformat(cache_entry['expires_at'])
        is_expired = datetime.datetime.now() > expires_at

        return RetrieveCacheOutput(
            found=True,
            data=cache_entry['data'] if not is_expired else None,
            is_expired=is_expired,
            cached_at=cache_entry['cached_at']
        )

    except Exception:
        return RetrieveCacheOutput(
            found=False,
            data=None,
            is_expired=False,
            cached_at=None
        )

# Run the server
if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Component Server")
    parser.add_argument("--http", action="store_true", help="Run in HTTP mode")
    parser.add_argument("--port", type=int, default=8080, help="HTTP port")
    args = parser.parse_args()

    if args.http:
        import uvicorn
        # Create HTTP app from server
        app = server.create_http_app()
        uvicorn.run(app, host="0.0.0.0", port=args.port)
    else:
        # Run in STDIO mode
        server.run()
```

### Configuration for Python Components

**STDIO Transport (Default):**
```yaml
# stepflow-config.yml
plugins:
  builtin:
    type: builtin
  my_components:
    type: stepflow
    transport: stdio
    command: python
    args: ["my_components.py"]
    working_directory: "."
    env:
      PYTHONPATH: "."
```

**HTTP Transport:**
```yaml
# stepflow-config.yml
plugins:
  builtin:
    type: builtin
  my_components_http:
    type: stepflow
    transport: http
    url: "http://localhost:8080"
    timeout: 60
```

Start the HTTP server:
```bash
# Start the component server in HTTP mode
uv run --extra http python my_components.py --http --port 8080
```

### Using the Custom Components

```yaml
name: "Custom Component Demo"
description: "Demonstrate custom Python components"

input_schema:
  type: object
  properties:
    api_url: { type: string }
    user_data: { type: object }

steps:
  # Use custom HTTP component
  - id: fetch_api_data
    component: /my_components/http_request
    input:
      url: { $from: { workflow: input }, path: "api_url" }
      method: "GET"
      timeout: 15
      retries: 2

  # Process data with custom component
  - id: process_user_data
    component: /my_components/process_data
    input:
      data: { $from: { workflow: input }, path: "user_data" }
      processing_rules:
        title_case: true
      include_metadata: true

  # Cache the results
  - id: cache_results
    component: /my_components/cache_data
    input:
      key: "processed_user_data"
      data:
        api_response: { $from: { step: fetch_api_data } }
        processed_user: { $from: { step: process_user_data } }
      ttl_minutes: 30

output:
  api_data: { $from: { step: fetch_api_data } }
  processed_data: { $from: { step: process_user_data } }
  cache_info: { $from: { step: cache_results } }

test:
  cases:
  - name: basic_processing
    input:
      api_url: "https://jsonplaceholder.typicode.com/posts/1"
      user_data:
        name: "john doe"
        email: "john@example.com"
        role: "developer"
```

## Advanced Python Component Patterns

### Database Component with Connection Pooling

```python
# database_components.py
from stepflow_sdk import StepflowStdioServer
import msgspec
import asyncio
import asyncpg
from typing import Optional, List, Dict, Any
import json

server = StepflowStdioServer()

# Global connection pool
_connection_pools = {}

class DatabaseQueryInput(msgspec.Struct):
    connection_string: str
    query: str
    parameters: Optional[List] = None
    fetch_mode: str = "all"  # "all", "one", "none"
    timeout: int = 30

class DatabaseQueryOutput(msgspec.Struct):
    success: bool
    rows: Optional[List[Dict[str, Any]]] = None
    row_count: int = 0
    execution_time_ms: int = 0
    error_message: Optional[str] = None

async def get_connection_pool(connection_string: str):
    """Get or create a connection pool for the given connection string."""
    if connection_string not in _connection_pools:
        _connection_pools[connection_string] = await asyncpg.create_pool(
            connection_string,
            min_size=1,
            max_size=10,
            command_timeout=30
        )
    return _connection_pools[connection_string]

@server.component
async def database_query(input: DatabaseQueryInput) -> DatabaseQueryOutput:
    """Execute database queries with connection pooling."""
    import time
    start_time = time.time()

    try:
        pool = await get_connection_pool(input.connection_string)

        async with pool.acquire() as connection:
            if input.fetch_mode == "none":
                # Execute without fetching (INSERT, UPDATE, DELETE)
                result = await connection.execute(input.query, *(input.parameters or []))
                row_count = int(result.split()[-1]) if result.split() else 0
                rows = None
            elif input.fetch_mode == "one":
                # Fetch single row
                row = await connection.fetchrow(input.query, *(input.parameters or []))
                rows = [dict(row)] if row else []
                row_count = len(rows)
            else:
                # Fetch all rows
                result = await connection.fetch(input.query, *(input.parameters or []))
                rows = [dict(row) for row in result]
                row_count = len(rows)

        execution_time = int((time.time() - start_time) * 1000)

        return DatabaseQueryOutput(
            success=True,
            rows=rows,
            row_count=row_count,
            execution_time_ms=execution_time,
            error_message=None
        )

    except Exception as e:
        execution_time = int((time.time() - start_time) * 1000)
        return DatabaseQueryOutput(
            success=False,
            rows=None,
            row_count=0,
            execution_time_ms=execution_time,
            error_message=str(e)
        )

# Bulk operations component
class BulkInsertInput(msgspec.Struct):
    connection_string: str
    table_name: str
    records: List[Dict[str, Any]]
    batch_size: int = 1000
    upsert_key: Optional[List[str]] = None

class BulkInsertOutput(msgspec.Struct):
    success: bool
    records_processed: int
    batches_processed: int
    execution_time_ms: int
    error_message: Optional[str] = None

@server.component
async def bulk_insert(input: BulkInsertInput) -> BulkInsertOutput:
    """Perform bulk insert operations with batching."""
    import time
    start_time = time.time()

    try:
        pool = await get_connection_pool(input.connection_string)
        records_processed = 0
        batches_processed = 0

        # Process records in batches
        for i in range(0, len(input.records), input.batch_size):
            batch = input.records[i:i + input.batch_size]

            async with pool.acquire() as connection:
                async with connection.transaction():
                    if input.upsert_key:
                        # Implement upsert logic
                        for record in batch:
                            columns = list(record.keys())
                            values = list(record.values())
                            placeholders = ', '.join(f'${i+1}' for i in range(len(values)))

                            upsert_query = f"""
                                INSERT INTO {input.table_name} ({', '.join(columns)})
                                VALUES ({placeholders})
                                ON CONFLICT ({', '.join(input.upsert_key)})
                                DO UPDATE SET {', '.join(f'{col} = EXCLUDED.{col}' for col in columns if col not in input.upsert_key)}
                            """
                            await connection.execute(upsert_query, *values)
                    else:
                        # Simple insert
                        if batch:
                            columns = list(batch[0].keys())
                            values_list = [list(record.values()) for record in batch]

                            query = f"""
                                INSERT INTO {input.table_name} ({', '.join(columns)})
                                VALUES ({', '.join(f'${i+1}' for i in range(len(columns)))})
                            """
                            await connection.executemany(query, values_list)

            records_processed += len(batch)
            batches_processed += 1

        execution_time = int((time.time() - start_time) * 1000)

        return BulkInsertOutput(
            success=True,
            records_processed=records_processed,
            batches_processed=batches_processed,
            execution_time_ms=execution_time,
            error_message=None
        )

    except Exception as e:
        execution_time = int((time.time() - start_time) * 1000)
        return BulkInsertOutput(
            success=False,
            records_processed=records_processed,
            batches_processed=batches_processed,
            execution_time_ms=execution_time,
            error_message=str(e)
        )

if __name__ == "__main__":
    server.run()
```

## TypeScript SDK Components

:::info Future Component
The TypeScript SDK is a planned future feature. This example shows what the API would look like.
:::

```typescript
// image-processing-components.ts
import { StepflowStdioServer, StepflowContext } from '@stepflow/sdk';
import sharp from 'sharp';
import { z } from 'zod';

const server = new StepflowStdioServer();

// Define schemas using Zod
const ImageProcessInput = z.object({
  image_path: z.string(),
  operations: z.array(z.object({
    type: z.enum(['resize', 'crop', 'rotate', 'blur', 'sharpen']),
    parameters: z.record(z.any())
  })),
  output_format: z.enum(['jpeg', 'png', 'webp']).default('jpeg'),
  quality: z.number().min(1).max(100).default(85)
});

const ImageProcessOutput = z.object({
  success: z.boolean(),
  output_path: z.string().optional(),
  original_size: z.object({
    width: z.number(),
    height: z.number(),
    fileSize: z.number()
  }).optional(),
  processed_size: z.object({
    width: z.number(),
    height: z.number(),
    fileSize: z.number()
  }).optional(),
  operations_applied: z.array(z.string()),
  processing_time_ms: z.number(),
  error_message: z.string().optional()
});

// Image processing component
server.component('process_image', {
  inputSchema: ImageProcessInput,
  outputSchema: ImageProcessOutput,
  execute: async (input, context: StepflowContext) => {
    const startTime = Date.now();

    try {
      // Load the image
      const image = sharp(input.image_path);
      const metadata = await image.metadata();

      let pipeline = image;
      const appliedOperations: string[] = [];

      // Apply operations in sequence
      for (const operation of input.operations) {
        switch (operation.type) {
          case 'resize':
            pipeline = pipeline.resize(
              operation.parameters.width,
              operation.parameters.height,
              { fit: operation.parameters.fit || 'cover' }
            );
            appliedOperations.push(`resize(${operation.parameters.width}x${operation.parameters.height})`);
            break;

          case 'crop':
            pipeline = pipeline.extract({
              left: operation.parameters.left || 0,
              top: operation.parameters.top || 0,
              width: operation.parameters.width,
              height: operation.parameters.height
            });
            appliedOperations.push(`crop(${operation.parameters.width}x${operation.parameters.height})`);
            break;

          case 'rotate':
            pipeline = pipeline.rotate(operation.parameters.degrees || 90);
            appliedOperations.push(`rotate(${operation.parameters.degrees || 90}°)`);
            break;

          case 'blur':
            pipeline = pipeline.blur(operation.parameters.sigma || 1);
            appliedOperations.push(`blur(${operation.parameters.sigma || 1})`);
            break;

          case 'sharpen':
            pipeline = pipeline.sharpen(
              operation.parameters.sigma || 1,
              operation.parameters.flat || 1,
              operation.parameters.jagged || 2
            );
            appliedOperations.push('sharpen');
            break;
        }
      }

      // Set output format and quality
      switch (input.output_format) {
        case 'jpeg':
          pipeline = pipeline.jpeg({ quality: input.quality });
          break;
        case 'png':
          pipeline = pipeline.png({ quality: input.quality });
          break;
        case 'webp':
          pipeline = pipeline.webp({ quality: input.quality });
          break;
      }

      // Generate output path
      const outputPath = input.image_path.replace(
        /\.[^/.]+$/,
        `_processed.${input.output_format}`
      );

      // Save the processed image
      await pipeline.toFile(outputPath);

      // Get processed image stats
      const outputStats = await sharp(outputPath).stats();
      const outputMetadata = await sharp(outputPath).metadata();

      const processingTime = Date.now() - startTime;

      return {
        success: true,
        output_path: outputPath,
        original_size: {
          width: metadata.width || 0,
          height: metadata.height || 0,
          fileSize: metadata.size || 0
        },
        processed_size: {
          width: outputMetadata.width || 0,
          height: outputMetadata.height || 0,
          fileSize: outputMetadata.size || 0
        },
        operations_applied: appliedOperations,
        processing_time_ms: processingTime
      };

    } catch (error) {
      const processingTime = Date.now() - startTime;

      return {
        success: false,
        operations_applied: [],
        processing_time_ms: processingTime,
        error_message: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }
});

// Batch image processing component
const BatchImageProcessInput = z.object({
  image_directory: z.string(),
  operations: z.array(z.object({
    type: z.enum(['resize', 'crop', 'rotate', 'blur', 'sharpen']),
    parameters: z.record(z.any())
  })),
  output_directory: z.string(),
  output_format: z.enum(['jpeg', 'png', 'webp']).default('jpeg'),
  quality: z.number().min(1).max(100).default(85),
  parallel_limit: z.number().min(1).max(10).default(3)
});

const BatchImageProcessOutput = z.object({
  success: z.boolean(),
  total_images: z.number(),
  processed_images: z.number(),
  failed_images: z.number(),
  processing_time_ms: z.number(),
  results: z.array(z.object({
    input_path: z.string(),
    output_path: z.string().optional(),
    success: z.boolean(),
    error_message: z.string().optional()
  }))
});

server.component('batch_process_images', {
  inputSchema: BatchImageProcessInput,
  outputSchema: BatchImageProcessOutput,
  execute: async (input, context: StepflowContext) => {
    const startTime = Date.now();
    const fs = await import('fs/promises');
    const path = await import('path');

    try {
      // Read directory and filter image files
      const files = await fs.readdir(input.image_directory);
      const imageFiles = files.filter(file =>
        /\.(jpg|jpeg|png|webp|tiff|bmp)$/i.test(file)
      );

      // Ensure output directory exists
      await fs.mkdir(input.output_directory, { recursive: true });

      // Process images with concurrency limit
      const results: any[] = [];
      const chunks = [];

      for (let i = 0; i < imageFiles.length; i += input.parallel_limit) {
        chunks.push(imageFiles.slice(i, i + input.parallel_limit));
      }

      for (const chunk of chunks) {
        const chunkPromises = chunk.map(async (filename) => {
          const inputPath = path.join(input.image_directory, filename);
          const outputFilename = filename.replace(
            /\.[^/.]+$/,
            `_processed.${input.output_format}`
          );
          const outputPath = path.join(input.output_directory, outputFilename);

          try {
            // Use the single image processing logic
            let pipeline = sharp(inputPath);

            for (const operation of input.operations) {
              // Apply operations (same logic as single image processor)
              // ... operation logic here ...
            }

            await pipeline
              .jpeg({ quality: input.quality })
              .toFile(outputPath);

            return {
              input_path: inputPath,
              output_path: outputPath,
              success: true
            };
          } catch (error) {
            return {
              input_path: inputPath,
              success: false,
              error_message: error instanceof Error ? error.message : 'Unknown error'
            };
          }
        });

        const chunkResults = await Promise.all(chunkPromises);
        results.push(...chunkResults);
      }

      const processingTime = Date.now() - startTime;
      const processedCount = results.filter(r => r.success).length;
      const failedCount = results.filter(r => !r.success).length;

      return {
        success: failedCount === 0,
        total_images: imageFiles.length,
        processed_images: processedCount,
        failed_images: failedCount,
        processing_time_ms: processingTime,
        results
      };

    } catch (error) {
      const processingTime = Date.now() - startTime;

      return {
        success: false,
        total_images: 0,
        processed_images: 0,
        failed_images: 0,
        processing_time_ms: processingTime,
        results: [],
        error_message: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }
});

// Start the server
server.run();
```

## Rust Component Example

For maximum performance, components can be written in Rust using the protocol directly.

```rust
// rust_components/src/main.rs
use serde::{Deserialize, Serialize};
use stepflow_protocol::{
    ComponentExecuteParams, ComponentExecuteResult, ComponentInfoParams, ComponentInfoResult,
    InitializeParams, InitializeResult, JsonRpcRequest, JsonRpcResponse,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Serialize, Deserialize)]
struct MathOperationInput {
    operation: String, // "add", "multiply", "power"
    operands: Vec<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MathOperationOutput {
    result: f64,
    operation_performed: String,
}

struct MathComponent;

impl MathComponent {
    fn execute(&self, input: MathOperationInput) -> Result<MathOperationOutput, String> {
        let result = match input.operation.as_str() {
            "add" => input.operands.iter().sum(),
            "multiply" => input.operands.iter().product(),
            "power" => {
                if input.operands.len() != 2 {
                    return Err("Power operation requires exactly 2 operands".to_string());
                }
                input.operands[0].powf(input.operands[1])
            }
            _ => return Err(format!("Unknown operation: {}", input.operation)),
        };

        Ok(MathOperationOutput {
            result,
            operation_performed: format!("{} on {:?}", input.operation, input.operands),
        })
    }

    fn get_info(&self) -> ComponentInfoResult {
        ComponentInfoResult {
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["add", "multiply", "power"]
                    },
                    "operands": {
                        "type": "array",
                        "items": { "type": "number" },
                        "minItems": 1
                    }
                },
                "required": ["operation", "operands"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "result": { "type": "number" },
                    "operation_performed": { "type": "string" }
                },
                "required": ["result", "operation_performed"]
            }),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    let math_component = MathComponent;

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            break; // EOF
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse request: {}", e);
                continue;
            }
        };

        let response = match request.method.as_str() {
            "initialize" => {
                let _params: InitializeParams = serde_json::from_value(request.params)?;
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(serde_json::to_value(InitializeResult {})?),
                    error: None,
                }
            }
            "component_info" => {
                let _params: ComponentInfoParams = serde_json::from_value(request.params)?;
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(serde_json::to_value(math_component.get_info())?),
                    error: None,
                }
            }
            "component_execute" => {
                let params: ComponentExecuteParams = serde_json::from_value(request.params)?;
                let input: MathOperationInput = serde_json::from_value(params.input)?;

                match math_component.execute(input) {
                    Ok(output) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(serde_json::to_value(ComponentExecuteResult {
                            output: serde_json::to_value(output)?,
                        })?),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(serde_json::json!({
                            "code": -32000,
                            "message": e
                        })),
                    },
                }
            }
            _ => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(serde_json::json!({
                    "code": -32601,
                    "message": "Method not found"
                })),
            },
        };

        let response_json = serde_json::to_string(&response)?;
        stdout.write_all(response_json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}
```

```toml
# rust_components/Cargo.toml
[package]
name = "rust_components"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
stepflow-protocol = { path = "../../stepflow-rs/crates/stepflow-protocol" }
```

## Component Testing Strategies

### Unit Testing Python Components

```python
# test_my_components.py
import pytest
import asyncio
from my_components import process_data, ProcessDataInput, ProcessDataOutput
from stepflow_sdk import StepflowContext

class MockContext(StepflowContext):
    """Mock context for testing components."""

    def __init__(self):
        self._blobs = {}
        self._blob_counter = 0

    async def put_blob(self, data):
        blob_id = f"blob_{self._blob_counter}"
        self._blob_counter += 1
        self._blobs[blob_id] = data
        return blob_id

    async def get_blob(self, blob_id):
        return self._blobs[blob_id]

def test_process_data_basic():
    """Test basic data processing functionality."""
    input_data = ProcessDataInput(
        data={"name": "john doe", "email": "john@example.com"},
        processing_rules={"title_case": True},
        include_metadata=True
    )

    context = MockContext()
    result = process_data(input_data, context)

    assert isinstance(result, ProcessDataOutput)
    assert result.processed_data["name"] == "John Doe"
    assert result.processed_data["email"] == "john@example.com"
    assert result.metadata["component_version"] == "1.0.0"
    assert result.processing_time_ms >= 0

def test_process_data_no_rules():
    """Test data processing without any rules."""
    input_data = ProcessDataInput(
        data={"name": "john doe", "status": "active"},
        include_metadata=False
    )

    context = MockContext()
    result = process_data(input_data, context)

    assert result.processed_data == input_data.data
    assert result.metadata == {}

@pytest.mark.asyncio
async def test_cache_data():
    """Test caching functionality."""
    from my_components import cache_data, CacheDataInput

    input_data = CacheDataInput(
        key="test_key",
        data={"test": "value"},
        ttl_minutes=60
    )

    context = MockContext()
    result = await cache_data(input_data, context)

    assert result.cache_key == "test_key"
    assert result.blob_id.startswith("blob_")

    # Verify data was stored correctly
    cached_data = await context.get_blob(result.blob_id)
    assert cached_data["data"] == {"test": "value"}
    assert cached_data["cache_key"] == "test_key"
```

### Integration Testing

```yaml
# test_custom_components.yaml
name: "Custom Component Integration Test"
description: "Test custom components in a real workflow"

input_schema:
  type: object
  properties:
    test_data: { type: object }

steps:
  - id: process_test_data
    component: /my_components/process_data
    input:
      data: { $from: { workflow: input }, path: "test_data" }
      processing_rules:
        uppercase: true
      include_metadata: true

  - id: cache_processed_data
    component: /my_components/cache_data
    input:
      key: "integration_test"
      data: { $from: { step: process_test_data } }
      ttl_minutes: 5

  - id: retrieve_cached_data
    component: /my_components/retrieve_cache
    input:
      blob_id: { $from: { step: cache_processed_data }, path: "blob_id" }

output:
  original_processing: { $from: { step: process_test_data } }
  cache_result: { $from: { step: cache_processed_data } }
  retrieved_data: { $from: { step: retrieve_cached_data } }

test:
  cases:
  - name: full_integration_test
    description: Test the complete custom component workflow
    input:
      test_data:
        name: "alice"
        role: "engineer"
        department: "platform"
    output:
      outcome: success
      result:
        original_processing:
          processed_data:
            name: "ALICE"
            role: "ENGINEER"
            department: "PLATFORM"
          metadata:
            component_version: "1.0.0"
        retrieved_data:
          found: true
          is_expired: false
```

## Best Practices for Custom Components

### Error Handling

```python
from stepflow_sdk import StepflowStdioServer, StepflowContext
import msgspec
from typing import Optional
import logging

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class ComponentError(Exception):
    """Custom exception for component errors."""

    def __init__(self, message: str, error_code: str = "COMPONENT_ERROR"):
        self.message = message
        self.error_code = error_code
        super().__init__(message)

class RobustProcessingInput(msgspec.Struct):
    data: dict
    strict_mode: bool = False
    max_retries: int = 3

class RobustProcessingOutput(msgspec.Struct):
    success: bool
    processed_data: Optional[dict] = None
    error_message: Optional[str] = None
    error_code: Optional[str] = None
    retries_attempted: int = 0

@server.component
def robust_processing(input: RobustProcessingInput, context: StepflowContext) -> RobustProcessingOutput:
    """Component with comprehensive error handling and retry logic."""

    retries = 0
    last_error = None

    while retries <= input.max_retries:
        try:
            logger.info(f"Processing attempt {retries + 1} for data: {list(input.data.keys())}")

            # Validate input data
            if not input.data:
                raise ComponentError("Input data is empty", "EMPTY_INPUT")

            # Simulate processing that might fail
            processed = {}
            for key, value in input.data.items():
                if not isinstance(value, (str, int, float)):
                    if input.strict_mode:
                        raise ComponentError(f"Invalid data type for key {key}", "INVALID_TYPE")
                    else:
                        logger.warning(f"Skipping invalid data type for key {key}")
                        continue

                # Process the value
                if isinstance(value, str):
                    processed[key] = value.strip().title()
                else:
                    processed[key] = value

            logger.info(f"Successfully processed {len(processed)} fields")

            return RobustProcessingOutput(
                success=True,
                processed_data=processed,
                retries_attempted=retries
            )

        except ComponentError as e:
            logger.error(f"Component error on attempt {retries + 1}: {e.message}")
            last_error = e
            if input.strict_mode or retries >= input.max_retries:
                return RobustProcessingOutput(
                    success=False,
                    error_message=e.message,
                    error_code=e.error_code,
                    retries_attempted=retries
                )
            retries += 1

        except Exception as e:
            logger.error(f"Unexpected error on attempt {retries + 1}: {str(e)}")
            last_error = e
            if retries >= input.max_retries:
                return RobustProcessingOutput(
                    success=False,
                    error_message=f"Unexpected error: {str(e)}",
                    error_code="UNEXPECTED_ERROR",
                    retries_attempted=retries
                )
            retries += 1

    # If we get here, all retries failed
    return RobustProcessingOutput(
        success=False,
        error_message=f"Failed after {input.max_retries + 1} attempts: {str(last_error)}",
        error_code="MAX_RETRIES_EXCEEDED",
        retries_attempted=retries
    )
```

### Performance Optimization

```python
# High-performance component patterns
import asyncio
import concurrent.futures
from typing import List

class BatchProcessingInput(msgspec.Struct):
    items: List[dict]
    batch_size: int = 100
    max_workers: int = 4

class BatchProcessingOutput(msgspec.Struct):
    processed_items: List[dict]
    processing_stats: dict

@server.component
async def batch_processing(input: BatchProcessingInput, context: StepflowContext) -> BatchProcessingOutput:
    """High-performance batch processing with parallel execution."""

    def process_single_item(item: dict) -> dict:
        """Process a single item (CPU-intensive work)."""
        # Simulate processing
        return {
            **item,
            'processed': True,
            'checksum': hash(str(item))
        }

    # Use thread pool for CPU-intensive work
    with concurrent.futures.ThreadPoolExecutor(max_workers=input.max_workers) as executor:
        # Process items in batches
        all_results = []
        batch_count = 0

        for i in range(0, len(input.items), input.batch_size):
            batch = input.items[i:i + input.batch_size]
            batch_count += 1

            # Submit batch to thread pool
            futures = [executor.submit(process_single_item, item) for item in batch]

            # Collect results
            batch_results = []
            for future in concurrent.futures.as_completed(futures):
                batch_results.append(future.result())

            all_results.extend(batch_results)

        return BatchProcessingOutput(
            processed_items=all_results,
            processing_stats={
                'total_items': len(input.items),
                'batches_processed': batch_count,
                'batch_size': input.batch_size,
                'workers_used': input.max_workers
            }
        )
```

## HTTP Transport Example

This example shows how to create a distributed component server using HTTP transport, ideal for microservices architectures.

### HTTP Component Server

```python
# http_components.py
from stepflow_sdk import StepflowServer
import msgspec
import uvicorn
import argparse
from typing import Optional

# Create the core server (transport-agnostic)
server = StepflowServer()

class DistributedProcessingInput(msgspec.Struct):
    task_id: str
    data: dict
    processing_mode: str = "standard"
    priority: int = 1

class DistributedProcessingOutput(msgspec.Struct):
    task_id: str
    result: dict
    processing_time_ms: int
    server_id: str
    session_id: Optional[str] = None

@server.component
def distributed_processing(input: DistributedProcessingInput) -> DistributedProcessingOutput:
    """Process data in a distributed environment."""
    import time
    import os
    import uuid

    start_time = time.time()

    # Simulate distributed processing
    processed_data = {
        "original_data": input.data,
        "processed_by": f"server-{os.getpid()}",
        "task_id": input.task_id,
        "mode": input.processing_mode,
        "priority": input.priority,
        "timestamp": time.time()
    }

    # Simulate processing time based on priority
    processing_delay = max(0.1, 1.0 / input.priority)
    time.sleep(processing_delay)

    processing_time = int((time.time() - start_time) * 1000)

    return DistributedProcessingOutput(
        task_id=input.task_id,
        result=processed_data,
        processing_time_ms=processing_time,
        server_id=f"server-{os.getpid()}",
        session_id=str(uuid.uuid4())
    )

class HealthCheckOutput(msgspec.Struct):
    status: str
    server_id: str
    uptime_seconds: int

@server.component
def health_check() -> HealthCheckOutput:
    """Health check endpoint for monitoring."""
    import time
    import os

    return HealthCheckOutput(
        status="healthy",
        server_id=f"server-{os.getpid()}",
        uptime_seconds=int(time.time() - getattr(server, '_start_time', time.time()))
    )

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Distributed Component Server")
    parser.add_argument("--port", type=int, default=8080, help="HTTP port")
    parser.add_argument("--host", default="0.0.0.0", help="Host to bind to")
    args = parser.parse_args()

    # Track startup time
    import time
    server._start_time = time.time()

    # Create and run the HTTP server
    app = server.create_http_app()
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")
```

### HTTP Server Configuration

```yaml
# distributed-config.yml
plugins:
  builtin:
    type: builtin
  processing_cluster:
    type: stepflow
    transport: http
    url: "http://processing-server:8080"
    timeout: 120
  monitoring:
    type: stepflow
    transport: http
    url: "http://monitoring-server:8081"
    timeout: 30

routes:
  "/processing/{component}":
    - plugin: processing_cluster
  "/monitoring/{component}":
    - plugin: monitoring
  "/{component}"
    - plugin: builtin

stateStore:
  type: sqlite
  databaseUrl: "sqlite:distributed_state.db"
  autoMigrate: true
```

### Docker Deployment

```dockerfile
# Dockerfile for HTTP component server
FROM python:3.11-slim

WORKDIR /app

# Install dependencies
COPY requirements.txt .
RUN pip install -r requirements.txt

# Copy component server
COPY http_components.py .

# Expose port
EXPOSE 8080

# Run the server
CMD ["python", "http_components.py", "--port", "8080", "--host", "0.0.0.0"]
```

```yaml
# docker-compose.yml for distributed setup
version: '3.8'

services:
  processing-server:
    build: .
    ports:
      - "8080:8080"
    environment:
      - SERVER_ID=processing-1

  monitoring-server:
    build: .
    ports:
      - "8081:8080"
    environment:
      - SERVER_ID=monitoring-1
    command: ["python", "http_components.py", "--port", "8080"]

  stepflow-runtime:
    image: stepflow:latest
    volumes:
      - ./distributed-config.yml:/app/stepflow-config.yml
      - ./workflows:/app/workflows
    depends_on:
      - processing-server
      - monitoring-server
    command: ["stepflow", "serve", "--config", "/app/stepflow-config.yml"]
```

### Using Distributed Components

```yaml
# distributed-workflow.yaml
name: "Distributed Processing Workflow"
description: "Process data using distributed HTTP components"

input_schema:
  type: object
  properties:
    tasks:
      type: array
      items:
        type: object
        properties:
          id: { type: string }
          data: { type: object }
          priority: { type: integer, default: 1 }

steps:
  # Health check before processing
  - id: health_check
    component: /processing/health_check
    input: {}

  # Process multiple tasks in parallel
  - id: process_tasks
    component: /processing/distributed_processing
    input:
      task_id: { $from: { workflow: input }, path: "tasks[*].id" }
      data: { $from: { workflow: input }, path: "tasks[*].data" }
      processing_mode: "distributed"
      priority: { $from: { workflow: input }, path: "tasks[*].priority" }
    foreach: { $from: { workflow: input }, path: "tasks" }

  # Monitor processing results
  - id: monitor_results
    component: /monitoring/health_check
    input: {}

output:
  health_status: { $from: { step: health_check } }
  processing_results: { $from: { step: process_tasks } }
  monitoring_info: { $from: { step: monitor_results } }

test:
  cases:
  - name: distributed_processing_test
    input:
      tasks:
        - id: "task-1"
          data: { message: "Process this data" }
          priority: 2
        - id: "task-2"
          data: { message: "Higher priority task" }
          priority: 1
    output:
      outcome: success
```

## Next Steps

Custom components are the key to extending StepFlow's capabilities:

- **Start Simple**: Begin with basic data processing components
- **Choose Transport**: Use STDIO for simple local components, HTTP for distributed systems
- **Add Error Handling**: Implement robust error handling and retry logic
- **Optimize Performance**: Use async/await and parallel processing for scalability
- **Test Thoroughly**: Write comprehensive unit and integration tests
- **Document Well**: Provide clear schemas and usage examples
- **Scale Appropriately**: Use HTTP transport for microservices and distributed deployments