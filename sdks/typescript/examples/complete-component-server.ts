#!/usr/bin/env ts-node

/**
 * Complete Component Server Example
 * 
 * This example demonstrates a full-featured StepFlow component server with:
 * - Multiple component types (math, string, data processing)
 * - Blob storage and retrieval
 * - UDF integration
 * - Error handling
 * - Input validation
 * - Both stdio and HTTP modes
 * - Real-world use cases
 */

import { 
  StepflowStdioServer, 
  StepflowHttpServer,
  StepflowContext,
  udf,
  udfSchema,
  runCLI
} from '../src';

console.log('🚀 Complete Component Server Example\n');

// ===============================================
// 1. CREATE SERVER INSTANCE
// ===============================================

// The server will be selected based on CLI args (stdio vs HTTP)
const server = new StepflowStdioServer();

// ===============================================
// 2. BASIC MATH COMPONENTS
// ===============================================

console.log('📊 Registering Math Components...');

// Addition component
server.registerComponent(
  (input: { a: number; b: number }): { result: number; operation: string } => {
    return {
      result: input.a + input.b,
      operation: `${input.a} + ${input.b} = ${input.a + input.b}`
    };
  },
  'add',
  {
    description: 'Adds two numbers together',
    inputSchema: {
      type: 'object',
      properties: {
        a: { type: 'number', description: 'First number' },
        b: { type: 'number', description: 'Second number' }
      },
      required: ['a', 'b']
    },
    outputSchema: {
      type: 'object',
      properties: {
        result: { type: 'number', description: 'Sum of a and b' },
        operation: { type: 'string', description: 'Operation performed' }
      }
    }
  }
);

// Calculator component with multiple operations
server.registerComponent(
  (input: { 
    a: number; 
    b: number; 
    operation: 'add' | 'subtract' | 'multiply' | 'divide' | 'power' | 'mod';
  }) => {
    let result: number;
    let description: string;

    switch (input.operation) {
      case 'add':
        result = input.a + input.b;
        description = `${input.a} + ${input.b}`;
        break;
      case 'subtract':
        result = input.a - input.b;
        description = `${input.a} - ${input.b}`;
        break;
      case 'multiply':
        result = input.a * input.b;
        description = `${input.a} × ${input.b}`;
        break;
      case 'divide':
        if (input.b === 0) {
          throw new Error('Division by zero is not allowed');
        }
        result = input.a / input.b;
        description = `${input.a} ÷ ${input.b}`;
        break;
      case 'power':
        result = Math.pow(input.a, input.b);
        description = `${input.a} ^ ${input.b}`;
        break;
      case 'mod':
        result = input.a % input.b;
        description = `${input.a} mod ${input.b}`;
        break;
      default:
        throw new Error(`Unknown operation: ${input.operation}`);
    }

    return {
      result,
      operation: input.operation,
      description,
      operands: { a: input.a, b: input.b }
    };
  },
  'calculate',
  {
    description: 'Performs various mathematical operations',
    inputSchema: {
      type: 'object',
      properties: {
        a: { type: 'number', description: 'First operand' },
        b: { type: 'number', description: 'Second operand' },
        operation: {
          type: 'string',
          enum: ['add', 'subtract', 'multiply', 'divide', 'power', 'mod'],
          description: 'Mathematical operation to perform'
        }
      },
      required: ['a', 'b', 'operation']
    }
  }
);

// Statistics component
server.registerComponent(
  (input: { numbers: number[] }) => {
    if (!Array.isArray(input.numbers) || input.numbers.length === 0) {
      throw new Error('Input must be a non-empty array of numbers');
    }

    const numbers = input.numbers.filter(n => typeof n === 'number' && !isNaN(n));
    if (numbers.length === 0) {
      throw new Error('No valid numbers found in input');
    }

    const sum = numbers.reduce((a, b) => a + b, 0);
    const mean = sum / numbers.length;
    const sorted = [...numbers].sort((a, b) => a - b);
    const median = sorted.length % 2 === 0 
      ? (sorted[sorted.length / 2 - 1] + sorted[sorted.length / 2]) / 2
      : sorted[Math.floor(sorted.length / 2)];
    
    const variance = numbers.reduce((acc, n) => acc + Math.pow(n - mean, 2), 0) / numbers.length;
    const stdDev = Math.sqrt(variance);

    return {
      count: numbers.length,
      sum,
      mean,
      median,
      min: Math.min(...numbers),
      max: Math.max(...numbers),
      variance,
      standardDeviation: stdDev,
      range: Math.max(...numbers) - Math.min(...numbers)
    };
  },
  'statistics',
  {
    description: 'Calculates statistical measures for a set of numbers',
    inputSchema: {
      type: 'object',
      properties: {
        numbers: {
          type: 'array',
          items: { type: 'number' },
          minItems: 1,
          description: 'Array of numbers to analyze'
        }
      },
      required: ['numbers']
    }
  }
);

// ===============================================
// 3. STRING PROCESSING COMPONENTS
// ===============================================

console.log('📝 Registering String Components...');

// Text transformer
server.registerComponent(
  (input: { 
    text: string; 
    operations: Array<'uppercase' | 'lowercase' | 'capitalize' | 'reverse' | 'trim'>;
  }) => {
    let result = input.text;
    const appliedOperations: string[] = [];

    input.operations.forEach(op => {
      switch (op) {
        case 'uppercase':
          result = result.toUpperCase();
          appliedOperations.push('converted to uppercase');
          break;
        case 'lowercase':
          result = result.toLowerCase();
          appliedOperations.push('converted to lowercase');
          break;
        case 'capitalize':
          result = result.split(' ')
            .map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
            .join(' ');
          appliedOperations.push('capitalized words');
          break;
        case 'reverse':
          result = result.split('').reverse().join('');
          appliedOperations.push('reversed characters');
          break;
        case 'trim':
          result = result.trim();
          appliedOperations.push('trimmed whitespace');
          break;
      }
    });

    return {
      original: input.text,
      result,
      operations: input.operations,
      appliedOperations,
      metadata: {
        originalLength: input.text.length,
        resultLength: result.length,
        transformedAt: new Date().toISOString()
      }
    };
  },
  'transform_text',
  {
    description: 'Applies various text transformations',
    inputSchema: {
      type: 'object',
      properties: {
        text: { type: 'string', description: 'Text to transform' },
        operations: {
          type: 'array',
          items: {
            type: 'string',
            enum: ['uppercase', 'lowercase', 'capitalize', 'reverse', 'trim']
          },
          description: 'Array of operations to apply in order'
        }
      },
      required: ['text', 'operations']
    }
  }
);

// Text analyzer
server.registerComponent(
  (input: { text: string; includeWordFrequency?: boolean }) => {
    const words = input.text.toLowerCase()
      .replace(/[^\w\s]/g, '')
      .split(/\s+/)
      .filter(word => word.length > 0);

    const sentences = input.text.split(/[.!?]+/).filter(s => s.trim().length > 0);
    const paragraphs = input.text.split(/\n\s*\n/).filter(p => p.trim().length > 0);

    const result: any = {
      characters: input.text.length,
      charactersNoSpaces: input.text.replace(/\s/g, '').length,
      words: words.length,
      sentences: sentences.length,
      paragraphs: paragraphs.length,
      averageWordsPerSentence: words.length / sentences.length || 0,
      averageCharactersPerWord: words.reduce((sum, word) => sum + word.length, 0) / words.length || 0
    };

    if (input.includeWordFrequency) {
      const wordFreq: Record<string, number> = {};
      words.forEach(word => {
        wordFreq[word] = (wordFreq[word] || 0) + 1;
      });
      
      result.wordFrequency = Object.entries(wordFreq)
        .sort(([,a], [,b]) => b - a)
        .slice(0, 10)
        .reduce((obj, [word, count]) => {
          obj[word] = count;
          return obj;
        }, {} as Record<string, number>);
    }

    return result;
  },
  'analyze_text',
  {
    description: 'Analyzes text and provides statistics',
    inputSchema: {
      type: 'object',
      properties: {
        text: { type: 'string', description: 'Text to analyze' },
        includeWordFrequency: { 
          type: 'boolean', 
          default: false,
          description: 'Include word frequency analysis'
        }
      },
      required: ['text']
    }
  }
);

// ===============================================
// 4. DATA PROCESSING COMPONENTS
// ===============================================

console.log('🗄️  Registering Data Components...');

// Array processor
server.registerComponent(
  (input: { 
    data: any[]; 
    operation: 'filter' | 'map' | 'sort' | 'unique' | 'group' | 'chunk';
    config?: any;
  }) => {
    let result: any;
    const metadata: any = { originalLength: input.data.length };

    switch (input.operation) {
      case 'filter':
        if (!input.config?.condition) {
          throw new Error('Filter operation requires config.condition');
        }
        // Simple filtering - in real world you'd want more sophisticated condition parsing
        result = input.data.filter((item, index) => {
          const condition = input.config.condition;
          if (condition === 'truthy') return !!item;
          if (condition === 'falsy') return !item;
          if (condition.startsWith('index_')) {
            const op = condition.split('_')[1];
            const value = parseInt(condition.split('_')[2]);
            switch (op) {
              case 'gt': return index > value;
              case 'lt': return index < value;
              case 'eq': return index === value;
              default: return true;
            }
          }
          return true;
        });
        break;

      case 'map':
        const mapOp = input.config?.mapOperation || 'identity';
        result = input.data.map((item, index) => {
          switch (mapOp) {
            case 'identity': return item;
            case 'string': return String(item);
            case 'number': return Number(item);
            case 'index': return { index, value: item };
            case 'double': return typeof item === 'number' ? item * 2 : item;
            default: return item;
          }
        });
        break;

      case 'sort':
        const sortOrder = input.config?.order || 'asc';
        result = [...input.data].sort((a, b) => {
          if (a < b) return sortOrder === 'asc' ? -1 : 1;
          if (a > b) return sortOrder === 'asc' ? 1 : -1;
          return 0;
        });
        break;

      case 'unique':
        result = [...new Set(input.data)];
        break;

      case 'group':
        const groupSize = input.config?.size || 2;
        result = [];
        for (let i = 0; i < input.data.length; i += groupSize) {
          result.push(input.data.slice(i, i + groupSize));
        }
        break;

      case 'chunk':
        const chunkSize = input.config?.size || 1;
        result = [];
        for (let i = 0; i < input.data.length; i += chunkSize) {
          result.push(input.data.slice(i, i + chunkSize));
        }
        break;

      default:
        throw new Error(`Unknown operation: ${input.operation}`);
    }

    metadata.resultLength = Array.isArray(result) ? result.length : 1;
    metadata.operation = input.operation;
    metadata.config = input.config;

    return { result, metadata };
  },
  'process_array',
  {
    description: 'Processes arrays with various operations',
    inputSchema: {
      type: 'object',
      properties: {
        data: { type: 'array', description: 'Array to process' },
        operation: {
          type: 'string',
          enum: ['filter', 'map', 'sort', 'unique', 'group', 'chunk'],
          description: 'Operation to perform'
        },
        config: {
          type: 'object',
          description: 'Configuration for the operation'
        }
      },
      required: ['data', 'operation']
    }
  }
);

// Object transformer
server.registerComponent(
  (input: {
    data: Record<string, any>;
    transformations: Array<{
      type: 'rename' | 'remove' | 'add' | 'transform';
      field: string;
      newField?: string;
      value?: any;
      transformation?: string;
    }>;
  }) => {
    let result = { ...input.data };
    const appliedTransformations: string[] = [];

    input.transformations.forEach(transform => {
      switch (transform.type) {
        case 'rename':
          if (transform.newField && transform.field in result) {
            result[transform.newField] = result[transform.field];
            delete result[transform.field];
            appliedTransformations.push(`renamed ${transform.field} to ${transform.newField}`);
          }
          break;

        case 'remove':
          if (transform.field in result) {
            delete result[transform.field];
            appliedTransformations.push(`removed ${transform.field}`);
          }
          break;

        case 'add':
          result[transform.field] = transform.value;
          appliedTransformations.push(`added ${transform.field}`);
          break;

        case 'transform':
          if (transform.field in result) {
            const value = result[transform.field];
            switch (transform.transformation) {
              case 'uppercase':
                result[transform.field] = String(value).toUpperCase();
                break;
              case 'lowercase':
                result[transform.field] = String(value).toLowerCase();
                break;
              case 'number':
                result[transform.field] = Number(value);
                break;
              case 'string':
                result[transform.field] = String(value);
                break;
              case 'boolean':
                result[transform.field] = Boolean(value);
                break;
            }
            appliedTransformations.push(`transformed ${transform.field} with ${transform.transformation}`);
          }
          break;
      }
    });

    return {
      original: input.data,
      result,
      transformations: input.transformations,
      appliedTransformations,
      metadata: {
        originalFields: Object.keys(input.data).length,
        resultFields: Object.keys(result).length
      }
    };
  },
  'transform_object',
  {
    description: 'Transforms objects by renaming, adding, or removing fields',
    inputSchema: {
      type: 'object',
      properties: {
        data: { type: 'object', description: 'Object to transform' },
        transformations: {
          type: 'array',
          items: {
            type: 'object',
            properties: {
              type: {
                type: 'string',
                enum: ['rename', 'remove', 'add', 'transform']
              },
              field: { type: 'string' },
              newField: { type: 'string' },
              value: {},
              transformation: { type: 'string' }
            },
            required: ['type', 'field']
          }
        }
      },
      required: ['data', 'transformations']
    }
  }
);

// ===============================================
// 5. BLOB STORAGE COMPONENTS
// ===============================================

console.log('💾 Registering Blob Storage Components...');

// Store data as blob
server.registerComponent(
  async (input: { data: any; metadata?: any }, context?: StepflowContext) => {
    if (!context) {
      throw new Error('Context is required for blob operations');
    }

    const blobData = {
      data: input.data,
      metadata: {
        ...input.metadata,
        storedAt: new Date().toISOString(),
        type: Array.isArray(input.data) ? 'array' : typeof input.data,
        size: JSON.stringify(input.data).length
      }
    };

    const blobId = await context.putBlob(blobData);

    return {
      blobId,
      message: 'Data stored successfully',
      metadata: blobData.metadata
    };
  },
  'store_blob',
  {
    description: 'Stores data as a blob with metadata',
    inputSchema: {
      type: 'object',
      properties: {
        data: { description: 'Data to store as blob' },
        metadata: { type: 'object', description: 'Additional metadata' }
      },
      required: ['data']
    }
  }
);

// Retrieve blob data
server.registerComponent(
  async (input: { blobId: string }, context?: StepflowContext) => {
    if (!context) {
      throw new Error('Context is required for blob operations');
    }

    try {
      const blobData = await context.getBlob(input.blobId);
      
      return {
        blobId: input.blobId,
        data: blobData.data,
        metadata: {
          ...blobData.metadata,
          retrievedAt: new Date().toISOString()
        },
        success: true
      };
    } catch (error) {
      return {
        blobId: input.blobId,
        error: error instanceof Error ? error.message : 'Unknown error',
        success: false
      };
    }
  },
  'get_blob',
  {
    description: 'Retrieves data from a blob by ID',
    inputSchema: {
      type: 'object',
      properties: {
        blobId: { type: 'string', description: 'Blob ID to retrieve' }
      },
      required: ['blobId']
    }
  }
);

// ===============================================
// 6. UDF INTEGRATION
// ===============================================

console.log('🔧 Registering UDF Components...');

// Register the built-in UDF component
server.registerComponent(udf, 'udf', udfSchema.component);

// UDF helper: create and execute UDF in one step
server.registerComponent(
  async (input: {
    code: string;
    data: any;
    functionName?: string;
    inputSchema?: any;
  }, context?: StepflowContext) => {
    if (!context) {
      throw new Error('Context is required for UDF operations');
    }

    try {
      // Create UDF blob
      const udfDefinition: any = {
        code: input.code,
        input_schema: input.inputSchema || { type: 'object' }
      };

      if (input.functionName) {
        udfDefinition.function_name = input.functionName;
      }

      const blobId = await context.putBlob(udfDefinition);

      // Execute UDF
      const result = await udf({
        blob_id: blobId,
        input: input.data
      }, context);

      return {
        blobId,
        input: input.data,
        output: result,
        success: true,
        executedAt: new Date().toISOString()
      };
    } catch (error) {
      return {
        input: input.data,
        error: error instanceof Error ? error.message : 'Unknown error',
        success: false,
        executedAt: new Date().toISOString()
      };
    }
  },
  'execute_code',
  {
    description: 'Creates and executes JavaScript code as UDF',
    inputSchema: {
      type: 'object',
      properties: {
        code: { type: 'string', description: 'JavaScript code to execute' },
        data: { description: 'Input data for the code' },
        functionName: { type: 'string', description: 'Optional function name' },
        inputSchema: { type: 'object', description: 'Optional input schema' }
      },
      required: ['code', 'data']
    }
  }
);

// ===============================================
// 7. UTILITY COMPONENTS
// ===============================================

console.log('🔨 Registering Utility Components...');

// Health check component
server.registerComponent(
  () => {
    return {
      status: 'healthy',
      timestamp: new Date().toISOString(),
      uptime: process.uptime(),
      memory: process.memoryUsage(),
      version: process.version
    };
  },
  'health_check',
  {
    description: 'Returns server health status',
    inputSchema: { type: 'object', properties: {} }
  }
);

// Echo component for testing
server.registerComponent(
  (input: { message: any; delay?: number }) => {
    if (input.delay && input.delay > 0) {
      // Simulate processing time
      const start = Date.now();
      while (Date.now() - start < input.delay) {
        // Busy wait for demonstration
      }
    }

    return {
      echo: input.message,
      timestamp: new Date().toISOString(),
      delay: input.delay || 0,
      type: typeof input.message
    };
  },
  'echo',
  {
    description: 'Echoes back the input message with optional delay',
    inputSchema: {
      type: 'object',
      properties: {
        message: { description: 'Message to echo back' },
        delay: { 
          type: 'number', 
          minimum: 0, 
          maximum: 5000,
          description: 'Delay in milliseconds (max 5000)'
        }
      },
      required: ['message']
    }
  }
);

// Random data generator
server.registerComponent(
  (input: {
    type: 'number' | 'string' | 'boolean' | 'array' | 'object';
    count?: number;
    config?: any;
  }) => {
    const count = input.count || 1;
    const results: any[] = [];

    for (let i = 0; i < count; i++) {
      switch (input.type) {
        case 'number':
          const min = input.config?.min || 0;
          const max = input.config?.max || 100;
          results.push(Math.floor(Math.random() * (max - min + 1)) + min);
          break;

        case 'string':
          const length = input.config?.length || 10;
          const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
          let randomString = '';
          for (let j = 0; j < length; j++) {
            randomString += chars.charAt(Math.floor(Math.random() * chars.length));
          }
          results.push(randomString);
          break;

        case 'boolean':
          results.push(Math.random() > 0.5);
          break;

        case 'array':
          const arrayLength = input.config?.length || 5;
          const arrayType = input.config?.itemType || 'number';
          const array: any[] = [];
          for (let j = 0; j < arrayLength; j++) {
            if (arrayType === 'number') {
              array.push(Math.floor(Math.random() * 100));
            } else if (arrayType === 'string') {
              const chars = 'abcdefghijklmnopqrstuvwxyz';
              array.push(chars.charAt(Math.floor(Math.random() * chars.length)));
            } else {
              array.push(Math.random() > 0.5);
            }
          }
          results.push(array);
          break;

        case 'object':
          const fields = input.config?.fields || ['id', 'name', 'value'];
          const obj: any = {};
          fields.forEach((field: string) => {
            obj[field] = Math.random().toString(36).substring(7);
          });
          results.push(obj);
          break;
      }
    }

    return {
      type: input.type,
      count,
      config: input.config,
      results: count === 1 ? results[0] : results,
      generatedAt: new Date().toISOString()
    };
  },
  'generate_random',
  {
    description: 'Generates random data of specified type',
    inputSchema: {
      type: 'object',
      properties: {
        type: {
          type: 'string',
          enum: ['number', 'string', 'boolean', 'array', 'object'],
          description: 'Type of data to generate'
        },
        count: {
          type: 'number',
          minimum: 1,
          maximum: 100,
          default: 1,
          description: 'Number of items to generate'
        },
        config: {
          type: 'object',
          description: 'Type-specific configuration'
        }
      },
      required: ['type']
    }
  }
);

// ===============================================
// 8. START SERVER
// ===============================================

console.log('\n🎯 Server Configuration Complete!');
console.log(`📦 Registered ${server.getComponentNames?.()?.length || 'multiple'} components:`);
console.log('   - Math: add, calculate, statistics');
console.log('   - Text: transform_text, analyze_text');
console.log('   - Data: process_array, transform_object');
console.log('   - Storage: store_blob, get_blob');
console.log('   - UDF: udf, execute_code');
console.log('   - Utility: health_check, echo, generate_random');

console.log('\n🚀 Starting server...');
console.log('📡 Use --http flag for HTTP mode, otherwise runs in stdio mode');

// Use the CLI runner to handle both stdio and HTTP modes
if (require.main === module) {
  // Start the server using the CLI runner
  runCLI().catch(error => {
    console.error('❌ Server error:', error);
    process.exit(1);
  });
} else {
  // If imported as module, just run stdio server
  server.run();
}

// ===============================================
// 9. USAGE EXAMPLES
// ===============================================

console.log('\n📋 Usage Examples:');
console.log('==================');

console.log('\n1. Math Operations:');
console.log('   {"component": "add", "input": {"a": 10, "b": 20}}');
console.log('   {"component": "calculate", "input": {"a": 5, "b": 3, "operation": "multiply"}}');
console.log('   {"component": "statistics", "input": {"numbers": [1, 2, 3, 4, 5]}}');

console.log('\n2. Text Processing:');
console.log('   {"component": "transform_text", "input": {"text": "hello world", "operations": ["uppercase", "reverse"]}}');
console.log('   {"component": "analyze_text", "input": {"text": "The quick brown fox...", "includeWordFrequency": true}}');

console.log('\n3. Code Execution:');
console.log('   {"component": "execute_code", "input": {"code": "input.x + input.y", "data": {"x": 10, "y": 20}}}');

console.log('\n4. Data Processing:');
console.log('   {"component": "process_array", "input": {"data": [1,2,3,4,5], "operation": "filter", "config": {"condition": "index_gt_2"}}}');

console.log('\n✅ Complete component server ready for use!');