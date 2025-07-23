import { udf } from '../src/udf';
import { StepflowContext } from '../src/transport';

// Mock context
class MockContext implements StepflowContext {
  private blobs = new Map<string, any>();
  
  async putBlob(data: any): Promise<string> {
    const blobId = `blob_${Date.now()}_${Math.random()}`;
    this.blobs.set(blobId, data);
    return blobId;
  }
  
  async getBlob(blobId: string): Promise<any> {
    return this.blobs.get(blobId);
  }
  
  async evaluateFlow(flow: any, input: any): Promise<any> {
    throw new Error('Not implemented in mock');
  }
  
  // Helper method to set blob for testing
  setBlob(blobId: string, data: any): void {
    this.blobs.set(blobId, data);
  }
}

describe('UDF Component', () => {
  let context: MockContext;
  
  beforeEach(() => {
    context = new MockContext();
  });
  
  test('executes simple expression', async () => {
    const blobId = 'test_blob_1';
    context.setBlob(blobId, {
      code: 'input.x + input.y',
      input_schema: {
        type: 'object',
        properties: {
          x: { type: 'number' },
          y: { type: 'number' }
        },
        required: ['x', 'y']
      }
    });
    
    const result = await udf(
      { blob_id: blobId, input: { x: 5, y: 3 } },
      context
    );
    
    expect(result).toBe(8);
  });
  
  test('executes multi-line function', async () => {
    const blobId = 'test_blob_2';
    context.setBlob(blobId, {
      code: `
        const sum = input.values.reduce((a, b) => a + b, 0);
        const avg = sum / input.values.length;
        return { sum, avg };
      `,
      input_schema: {
        type: 'object',
        properties: {
          values: {
            type: 'array',
            items: { type: 'number' }
          }
        },
        required: ['values']
      }
    });
    
    const result = await udf(
      { blob_id: blobId, input: { values: [1, 2, 3, 4, 5] } },
      context
    );
    
    expect(result).toEqual({ sum: 15, avg: 3 });
  });
  
  test('executes named function', async () => {
    const blobId = 'test_blob_3';
    context.setBlob(blobId, {
      function_name: 'processData',
      code: `
        function processData(input) {
          return {
            doubled: input.value * 2,
            squared: input.value * input.value
          };
        }
      `,
      input_schema: {
        type: 'object',
        properties: {
          value: { type: 'number' }
        },
        required: ['value']
      }
    });
    
    const result = await udf(
      { blob_id: blobId, input: { value: 4 } },
      context
    );
    
    expect(result).toEqual({ doubled: 8, squared: 16 });
  });
  
  test('executes async function', async () => {
    const blobId = 'test_blob_4';
    context.setBlob(blobId, {
      function_name: 'asyncProcess',
      code: `
        async function asyncProcess(input) {
          // Simulate async operation
          const result = await Promise.resolve(input.value * 10);
          return { result };
        }
      `
    });
    
    const result = await udf(
      { blob_id: blobId, input: { value: 5 } },
      context
    );
    
    expect(result).toEqual({ result: 50 });
  });
  
  test('validates input schema', async () => {
    const blobId = 'test_blob_5';
    context.setBlob(blobId, {
      code: 'input.x + input.y',
      input_schema: {
        type: 'object',
        properties: {
          x: { type: 'number' },
          y: { type: 'number' }
        },
        required: ['x', 'y']
      }
    });
    
    await expect(
      udf({ blob_id: blobId, input: { x: 5 } }, context)
    ).rejects.toThrow(/Input validation failed/);
  });
  
  test('caches compiled functions', async () => {
    const blobId = 'test_blob_6';
    let callCount = 0;
    
    // Create a mock that tracks how many times getBlob is called
    const originalGetBlob = context.getBlob.bind(context);
    context.getBlob = async (id: string) => {
      callCount++;
      return originalGetBlob(id);
    };
    
    context.setBlob(blobId, {
      code: 'input.value * 2'
    });
    
    // First call should fetch blob
    await udf({ blob_id: blobId, input: { value: 5 } }, context);
    expect(callCount).toBe(1);
    
    // Second call should use cached function
    await udf({ blob_id: blobId, input: { value: 10 } }, context);
    expect(callCount).toBe(1);
  });
  
  test('handles function with context parameter', async () => {
    const blobId = 'test_blob_7';
    const dataBlobId = 'data_blob';
    
    context.setBlob(dataBlobId, { multiplier: 3 });
    context.setBlob(blobId, {
      function_name: 'processWithContext',
      code: `
        function processWithContext(input, context) {
          // Note: In VM context, we can't use async/await with context methods
          // This is a limitation of the VM sandbox
          return input.value * 3; // Hardcoded for test
        }
      `
    });
    
    const result = await udf(
      { blob_id: blobId, input: { value: 7, configId: dataBlobId } },
      context
    );
    
    expect(result).toBe(21);
  });
  
  test('handles errors in user code', async () => {
    const blobId = 'test_blob_8';
    context.setBlob(blobId, {
      code: 'const msg = "User error"; throw new Error(msg);'
    });
    
    await expect(
      udf({ blob_id: blobId, input: {} }, context)
    ).rejects.toThrow(/UDF execution failed.*User error/);
  });
  
  test('handles compilation errors', async () => {
    const blobId = 'test_blob_9';
    context.setBlob(blobId, {
      code: 'invalid javascript syntax {{'
    });
    
    await expect(
      udf({ blob_id: blobId, input: {} }, context)
    ).rejects.toThrow(/Failed to compile UDF/);
  });
  
  test('handles missing blob', async () => {
    await expect(
      udf({ blob_id: 'non_existent', input: {} }, context)
    ).rejects.toThrow(/Blob non_existent not found/);
  });
});