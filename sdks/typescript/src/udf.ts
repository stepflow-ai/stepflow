import * as vm from 'vm';
import Ajv from 'ajv';
import { StepflowContext } from './transport';

const ajv = new Ajv();

interface UdfInput {
  blob_id: string;
  input: any;
}

interface UdfBlob {
  code: string;
  input_schema?: any;
  function_name?: string;
}

type CompiledFunction = (input: any, context?: StepflowContext) => any | Promise<any>;

const functionCache = new Map<string, CompiledFunction>();

export async function udf(
  input: UdfInput,
  context?: StepflowContext
): Promise<any> {
  if (!context) {
    throw new Error('Context is required for UDF component');
  }
  
  const { blob_id, input: functionInput } = input;
  
  let compiledFunction = functionCache.get(blob_id);
  
  if (!compiledFunction) {
    const blobData = await context.getBlob(blob_id);
    
    if (!blobData) {
      throw new Error(`Blob ${blob_id} not found`);
    }
    
    const udfBlob = blobData as UdfBlob;
    
    if (udfBlob.input_schema) {
      const validate = ajv.compile(udfBlob.input_schema);
      if (!validate(functionInput)) {
        throw new Error(`Input validation failed: ${ajv.errorsText(validate.errors)}`);
      }
    }
    
    compiledFunction = compileFunction(udfBlob);
    functionCache.set(blob_id, compiledFunction);
  }
  
  try {
    const result = await compiledFunction(functionInput, context);
    return result;
  } catch (error: any) {
    if (error.message.includes('Failed to compile UDF')) {
      throw error;
    }
    throw new Error(`UDF execution failed: ${error.message}`);
  }
}

function compileFunction(udfBlob: UdfBlob): CompiledFunction {
  const { code, function_name } = udfBlob;
  
  // Create a safe sandbox with limited built-ins
  const sandbox: Record<string, any> = {
    JSON,
    Math,
    Date,
    Array,
    Object,
    String,
    Number,
    Boolean,
    RegExp,
    Promise,
    console: {
      log: () => {},
      error: () => {},
      warn: () => {},
      info: () => {},
    },
    parseInt,
    parseFloat,
    isNaN,
    isFinite,
  };
  
  // Create VM context
  const vmContext = vm.createContext(sandbox);
  
  try {
    if (function_name) {
      // Execute the code to define the function
      vm.runInContext(code, vmContext, {
        timeout: 5000,
        displayErrors: true,
      });
      
      // Get the function from the context
      const fn = sandbox[function_name];
      
      if (!fn || typeof fn !== 'function') {
        throw new Error(`Function ${function_name} not found in code`);
      }
      
      // Return a wrapper function
      return async (input: any, ctx?: StepflowContext) => {
        // Create a new context for each execution
        const executionSandbox: Record<string, any> = { ...sandbox };
        executionSandbox[function_name] = fn;
        const executionContext = vm.createContext(executionSandbox);
        
        // Check if function accepts context parameter
        const fnLength = fn.length;
        const codeToRun = fnLength === 2
          ? `${function_name}(input, context)`
          : `${function_name}(input)`;
        
        // Set input and context in the execution context
        executionSandbox.input = input;
        executionSandbox.context = ctx;
        
        const result = await vm.runInContext(codeToRun, executionContext, {
          timeout: 30000,
          displayErrors: true,
        });
        
        return result;
      };
    } else {
      const trimmedCode = code.trim();
      
      // Simple expression - wrap in return statement
      if (!trimmedCode.includes('\n') && !trimmedCode.includes(';') && !trimmedCode.includes('{')) {
        return (input: any, ctx?: StepflowContext) => {
          const executionSandbox: Record<string, any> = { ...sandbox, input, context: ctx };
          const executionContext = vm.createContext(executionSandbox);
          
          const result = vm.runInContext(trimmedCode, executionContext, {
            timeout: 30000,
            displayErrors: true,
          });
          
          return result;
        };
      } else {
        // Multi-line code - test compilation first
        const testSandbox: Record<string, any> = { ...sandbox };
        const testContext = vm.createContext(testSandbox);
        
        // Check if code has explicit return
        const hasReturn = /\breturn\b/.test(trimmedCode);
        
        try {
          if (hasReturn) {
            // Test compile with wrapper
            const wrappedCode = `(function() { ${trimmedCode} })`;
            vm.runInContext(wrappedCode, testContext, {
              timeout: 1000,
              displayErrors: true,
            });
          } else if (!trimmedCode.includes('throw')) {
            // Test compile as statements (skip if contains throw)
            vm.runInContext(trimmedCode, testContext, {
              timeout: 1000,
              displayErrors: true,
            });
          }
        } catch (error: any) {
          // Re-throw compilation errors
          throw error;
        }
        
        // Return execution function
        return (input: any, ctx?: StepflowContext) => {
          const executionSandbox: Record<string, any> = { ...sandbox, input, context: ctx };
          const executionContext = vm.createContext(executionSandbox);
          
          if (hasReturn) {
            // Code has return statement, wrap in function
            const wrappedCode = `(function() { ${trimmedCode} })()`;
            const result = vm.runInContext(wrappedCode, executionContext, {
              timeout: 30000,
              displayErrors: true,
            });
            return result;
          } else {
            // No return statement, evaluate last expression
            const lines = trimmedCode.split('\n').filter(line => line.trim());
            if (lines.length === 0) return undefined;
            
            // Execute all but last line
            if (lines.length > 1) {
              const setupCode = lines.slice(0, -1).join('\n');
              vm.runInContext(setupCode, executionContext, {
                timeout: 30000,
                displayErrors: true,
              });
            }
            
            // Evaluate and return last line
            const lastLine = lines[lines.length - 1];
            const result = vm.runInContext(lastLine, executionContext, {
              timeout: 30000,
              displayErrors: true,
            });
            return result;
          }
        };
      }
    }
  } catch (error: any) {
    throw new Error(`Failed to compile UDF: ${error.message}`);
  }
}

export const udfSchema = {
  component: {
    description: 'Execute user-defined functions stored as blobs',
    input_schema: {
      type: 'object',
      properties: {
        blob_id: {
          type: 'string',
          description: 'ID of the blob containing the function code and schema'
        },
        input: {
          description: 'Input data to pass to the function'
        }
      },
      required: ['blob_id', 'input']
    },
    output_schema: {
      description: 'Output from the executed function'
    }
  }
};