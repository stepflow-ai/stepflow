#!/usr/bin/env ts-node

/**
 * User-Defined Function (UDF) Examples
 * 
 * This example demonstrates various UDF patterns and use cases:
 * - Simple expressions
 * - Multi-line code with variables
 * - Named function definitions
 * - Complex data processing
 * - Error handling
 * - Different input/output schemas
 */

import { StepflowStdioServer, udf, udfSchema, StepflowContext } from '../src';
import crypto from 'crypto';

console.log('🔧 User-Defined Function (UDF) Examples\n');

// ===============================================
// 1. UDF BLOB EXAMPLES
// ===============================================

console.log('1. 📝 UDF Blob Examples');
console.log('=======================');

// Example 1: Simple arithmetic expression
const simpleArithmetic = {
  code: "input.a + input.b * input.c",
  input_schema: {
    type: 'object',
    properties: {
      a: { type: 'number', description: 'First operand' },
      b: { type: 'number', description: 'Second operand' },
      c: { type: 'number', description: 'Third operand' }
    },
    required: ['a', 'b', 'c']
  }
};

console.log('\nSimple Arithmetic:');
console.log(JSON.stringify(simpleArithmetic, null, 2));

// Example 2: String manipulation
const stringProcessing = {
  code: `
    const words = input.text.split(' ');
    const processed = words
      .map(word => word.toLowerCase())
      .filter(word => word.length > input.minLength)
      .join(', ');
    return {
      original: input.text,
      wordCount: words.length,
      filtered: processed,
      averageLength: words.reduce((sum, word) => sum + word.length, 0) / words.length
    };
  `,
  input_schema: {
    type: 'object',
    properties: {
      text: { type: 'string', description: 'Text to process' },
      minLength: { type: 'number', default: 3, description: 'Minimum word length' }
    },
    required: ['text']
  }
};

console.log('\nString Processing:');
console.log(JSON.stringify(stringProcessing, null, 2));

// Example 3: Named function with complex logic
const dataTransformer = {
  function_name: 'transformUserData',
  code: `
    function transformUserData(input) {
      const { users, config } = input;
      
      const transformed = users.map(user => {
        const fullName = \`\${user.firstName} \${user.lastName}\`;
        const email = user.email.toLowerCase();
        const age = new Date().getFullYear() - new Date(user.birthDate).getFullYear();
        
        return {
          id: user.id,
          fullName,
          email,
          age,
          isAdult: age >= 18,
          category: age < 18 ? 'minor' : age < 65 ? 'adult' : 'senior',
          preferences: {
            ...user.preferences,
            newsletter: config.defaultNewsletter && user.preferences.newsletter !== false
          }
        };
      });
      
      const stats = {
        total: transformed.length,
        adults: transformed.filter(u => u.isAdult).length,
        categories: {
          minor: transformed.filter(u => u.category === 'minor').length,
          adult: transformed.filter(u => u.category === 'adult').length,
          senior: transformed.filter(u => u.category === 'senior').length
        },
        avgAge: transformed.reduce((sum, u) => sum + u.age, 0) / transformed.length
      };
      
      return {
        users: transformed,
        statistics: stats,
        processedAt: new Date().toISOString()
      };
    }
  `,
  input_schema: {
    type: 'object',
    properties: {
      users: {
        type: 'array',
        items: {
          type: 'object',
          properties: {
            id: { type: 'string' },
            firstName: { type: 'string' },
            lastName: { type: 'string' },
            email: { type: 'string' },
            birthDate: { type: 'string' },
            preferences: { type: 'object' }
          }
        }
      },
      config: {
        type: 'object',
        properties: {
          defaultNewsletter: { type: 'boolean', default: true }
        }
      }
    },
    required: ['users']
  }
};

console.log('\nData Transformer Function:');
console.log(JSON.stringify(dataTransformer, null, 2));

// Example 4: Array processing with validation
const arrayProcessor = {
  code: `
    if (!Array.isArray(input.data)) {
      throw new Error('Input data must be an array');
    }
    
    const operations = input.operations || ['sum', 'avg', 'min', 'max'];
    const numbers = input.data.filter(item => typeof item === 'number');
    
    if (numbers.length === 0) {
      return { error: 'No valid numbers found in input data' };
    }
    
    const results = {};
    
    if (operations.includes('sum')) {
      results.sum = numbers.reduce((a, b) => a + b, 0);
    }
    
    if (operations.includes('avg')) {
      results.average = results.sum ? results.sum / numbers.length : 
                      numbers.reduce((a, b) => a + b, 0) / numbers.length;
    }
    
    if (operations.includes('min')) {
      results.minimum = Math.min(...numbers);
    }
    
    if (operations.includes('max')) {
      results.maximum = Math.max(...numbers);
    }
    
    return {
      input: input.data,
      validNumbers: numbers,
      count: numbers.length,
      operations: operations,
      results: results
    };
  `,
  input_schema: {
    type: 'object',
    properties: {
      data: {
        type: 'array',
        description: 'Array of mixed data types'
      },
      operations: {
        type: 'array',
        items: { 
          type: 'string',
          enum: ['sum', 'avg', 'min', 'max']
        },
        default: ['sum', 'avg', 'min', 'max']
      }
    },
    required: ['data']
  }
};

console.log('\nArray Processor:');
console.log(JSON.stringify(arrayProcessor, null, 2));

// ===============================================
// 2. CREATE UDF SERVER WITH EXAMPLES
// ===============================================

console.log('\n\n2. 🚀 UDF Server Setup');
console.log('======================');

const server = new StepflowStdioServer();

// Register the built-in UDF component
server.registerComponent(udf, 'udf', udfSchema.component);

// Helper function to create and store UDF blobs
async function createUdfBlob(udfDefinition: any, context: StepflowContext): Promise<string> {
  const blobId = await context.putBlob(udfDefinition);
  console.log(`Created UDF blob: ${blobId}`);
  return blobId;
}

// Register a component that creates UDF blobs for testing
server.registerComponent(
  async (input: { name: string }, context?: StepflowContext) => {
    if (!context) throw new Error('Context required');
    
    const udfDefinitions: Record<string, any> = {
      'simple_arithmetic': simpleArithmetic,
      'string_processing': stringProcessing,
      'data_transformer': dataTransformer,
      'array_processor': arrayProcessor
    };
    
    const definition = udfDefinitions[input.name];
    if (!definition) {
      throw new Error(`Unknown UDF: ${input.name}`);
    }
    
    const blobId = await createUdfBlob(definition, context);
    
    return {
      name: input.name,
      blobId,
      definition,
      message: `UDF '${input.name}' created successfully`
    };
  },
  'create_udf',
  {
    description: 'Creates UDF blobs for testing',
    inputSchema: {
      type: 'object',
      properties: {
        name: {
          type: 'string',
          enum: ['simple_arithmetic', 'string_processing', 'data_transformer', 'array_processor']
        }
      },
      required: ['name']
    }
  }
);

// Register a component that demonstrates UDF execution
server.registerComponent(
  async (input: { udfBlobId: string; data: any }, context?: StepflowContext) => {
    if (!context) throw new Error('Context required');
    
    // Execute the UDF
    try {
      const result = await udf({ blob_id: input.udfBlobId, input: input.data }, context);
      
      return {
        success: true,
        input: input.data,
        output: result,
        executedAt: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        input: input.data,
        error: error instanceof Error ? error.message : 'Unknown error',
        executedAt: new Date().toISOString()
      };
    }
  },
  'execute_udf',
  {
    description: 'Executes a UDF with provided data',
    inputSchema: {
      type: 'object',
      properties: {
        udfBlobId: { type: 'string', description: 'Blob ID of the UDF' },
        data: { description: 'Input data for the UDF' }
      },
      required: ['udfBlobId', 'data']
    }
  }
);

// ===============================================
// 3. EXAMPLE TEST DATA
// ===============================================

console.log('\n\n3. 🧪 Example Test Data');
console.log('=======================');

const testData = {
  simple_arithmetic: {
    a: 10,
    b: 5,
    c: 3
  },
  
  string_processing: {
    text: "The quick brown fox jumps over the lazy dog",
    minLength: 4
  },
  
  data_transformer: {
    users: [
      {
        id: "user1",
        firstName: "John",
        lastName: "Doe", 
        email: "JOHN.DOE@EXAMPLE.COM",
        birthDate: "1990-05-15",
        preferences: {
          newsletter: true,
          notifications: false
        }
      },
      {
        id: "user2",
        firstName: "Jane",
        lastName: "Smith",
        email: "jane.smith@example.com",
        birthDate: "2005-12-01",
        preferences: {
          newsletter: false,
          notifications: true
        }
      },
      {
        id: "user3",
        firstName: "Bob",
        lastName: "Johnson",
        email: "bob@example.com",
        birthDate: "1975-03-20",
        preferences: {}
      }
    ],
    config: {
      defaultNewsletter: true
    }
  },
  
  array_processor: {
    data: [1, "hello", 3.14, null, 42, "world", 7, undefined, 2.718],
    operations: ["sum", "avg", "min", "max"]
  }
};

console.log('Test data for UDF examples:');
Object.entries(testData).forEach(([name, data]) => {
  console.log(`\n${name}:`);
  console.log(JSON.stringify(data, null, 2));
});

// ===============================================
// 4. ADVANCED UDF PATTERNS
// ===============================================

console.log('\n\n4. 🎯 Advanced UDF Patterns');
console.log('============================');

// Pattern 1: Conditional logic with error handling
const conditionalProcessor = {
  code: `
    try {
      const { condition, trueValue, falseValue, data } = input;
      
      let result;
      switch (condition) {
        case 'isEmpty':
          result = !data || data.length === 0 ? trueValue : falseValue;
          break;
        case 'isNumber':
          result = typeof data === 'number' ? trueValue : falseValue;
          break;
        case 'isPositive':
          result = typeof data === 'number' && data > 0 ? trueValue : falseValue;
          break;
        case 'hasProperty':
          const prop = input.property;
          result = data && typeof data === 'object' && prop in data ? trueValue : falseValue;
          break;
        default:
          throw new Error(\`Unknown condition: \${condition}\`);
      }
      
      return {
        condition,
        data,
        result,
        metadata: {
          evaluatedAt: new Date().toISOString(),
          type: typeof result
        }
      };
    } catch (error) {
      return {
        error: error.message,
        input: input
      };
    }
  `,
  input_schema: {
    type: 'object',
    properties: {
      condition: {
        type: 'string',
        enum: ['isEmpty', 'isNumber', 'isPositive', 'hasProperty']
      },
      data: { description: 'Data to evaluate' },
      trueValue: { description: 'Value to return if condition is true' },
      falseValue: { description: 'Value to return if condition is false' },
      property: { type: 'string', description: 'Property name for hasProperty condition' }
    },
    required: ['condition', 'data', 'trueValue', 'falseValue']
  }
};

// Pattern 2: Data aggregation and grouping
const dataAggregator = {
  function_name: 'aggregateData',
  code: `
    function aggregateData(input) {
      const { data, groupBy, aggregations } = input;
      
      if (!Array.isArray(data)) {
        throw new Error('Data must be an array');
      }
      
      // Group data
      const groups = data.reduce((acc, item) => {
        const key = groupBy ? item[groupBy] : 'all';
        if (!acc[key]) acc[key] = [];
        acc[key].push(item);
        return acc;
      }, {});
      
      // Apply aggregations
      const results = {};
      
      for (const [groupKey, groupData] of Object.entries(groups)) {
        const groupResult = { count: groupData.length };
        
        if (aggregations.includes('sum')) {
          const numericFields = Object.keys(groupData[0] || {})
            .filter(field => typeof groupData[0][field] === 'number');
          
          numericFields.forEach(field => {
            groupResult[\`sum_\${field}\`] = groupData
              .reduce((sum, item) => sum + (item[field] || 0), 0);
          });
        }
        
        if (aggregations.includes('avg')) {
          const numericFields = Object.keys(groupData[0] || {})
            .filter(field => typeof groupData[0][field] === 'number');
          
          numericFields.forEach(field => {
            const sum = groupData.reduce((sum, item) => sum + (item[field] || 0), 0);
            groupResult[\`avg_\${field}\`] = sum / groupData.length;
          });
        }
        
        if (aggregations.includes('distinct')) {
          const stringFields = Object.keys(groupData[0] || {})
            .filter(field => typeof groupData[0][field] === 'string');
          
          stringFields.forEach(field => {
            const unique = [...new Set(groupData.map(item => item[field]))];
            groupResult[\`distinct_\${field}\`] = unique;
          });
        }
        
        results[groupKey] = groupResult;
      }
      
      return {
        groupBy: groupBy || 'all',
        aggregations,
        totalRecords: data.length,
        groups: Object.keys(results).length,
        results
      };
    }
  `,
  input_schema: {
    type: 'object',
    properties: {
      data: {
        type: 'array',
        description: 'Array of objects to aggregate'
      },
      groupBy: {
        type: 'string',
        description: 'Field to group by (optional)'
      },
      aggregations: {
        type: 'array',
        items: {
          type: 'string',
          enum: ['sum', 'avg', 'distinct']
        },
        default: ['sum', 'avg']
      }
    },
    required: ['data', 'aggregations']
  }
};

console.log('Advanced UDF Patterns:');
console.log('\nConditional Processor:');
console.log(JSON.stringify(conditionalProcessor, null, 2));
console.log('\nData Aggregator:');
console.log(JSON.stringify(dataAggregator, null, 2));

// ===============================================
// 5. UDF USAGE IN WORKFLOWS
// ===============================================

console.log('\n\n5. 🏗️  UDF Usage in Workflows');
console.log('==============================');

// Example workflow step that uses UDF
const workflowWithUdf = {
  id: 'process_data',
  component: 'udf',
  input: {
    blob_id: 'SHA256_HASH_OF_UDF_BLOB',  // Would be actual blob ID
    input: {
      users: [
        { name: 'Alice', age: 30, department: 'Engineering' },
        { name: 'Bob', age: 25, department: 'Marketing' },
        { name: 'Carol', age: 35, department: 'Engineering' }
      ],
      groupBy: 'department',
      aggregations: ['avg', 'distinct']
    }
  }
};

console.log('Example workflow step using UDF:');
console.log(JSON.stringify(workflowWithUdf, null, 2));

console.log('\n✅ UDF examples completed successfully!');
console.log('\nKey Features Demonstrated:');
console.log('- Simple expression execution');
console.log('- Multi-line code with variables');
console.log('- Named function definitions');
console.log('- Complex data processing logic');
console.log('- Error handling within UDFs');
console.log('- Input schema validation');
console.log('- Conditional logic patterns');
console.log('- Data aggregation and grouping');
console.log('- Integration with workflow steps');

// Note: In a real scenario, you would run server.run() to start the server
// For this example, we're just demonstrating the setup
console.log('\n📝 Note: Run server.run() to start the UDF server');

// Uncomment to actually start the server:
// server.run();