// Equivalent to server.py
// Contains the server implementation

import * as readline from 'readline';
import * as url from 'url';
import { Incoming, MethodResponse, createSuccessResponse, createErrorResponse, parseIncoming, serializeResponse } from './transport';
import * as protocol from './protocol';

/**
 * Component entry in the server registry
 */
export interface ComponentEntry<TInput, TOutput> {
  name: string;
  function: (input: TInput) => TOutput;
  inputSchema: Record<string, any>;
  outputSchema: Record<string, any>;
}

/**
 * Type for component registration options
 */
interface ComponentOptions {
  name?: string;
}

/**
 * StepflowStdioServer
 * 
 * Server implementation for Stepflow using stdio
 */
export class StepflowStdioServer {
  private components: Map<string, ComponentEntry<any, any>> = new Map();
  private initialized: boolean = false;
  private rl?: readline.Interface;

  /**
   * Register a component with the server
   */
  public component(options?: ComponentOptions): any;
  public component(target: any, context?: ClassMethodDecoratorContext): any;
  public component(targetOrOptions?: any, context?: ClassMethodDecoratorContext): any {
    // If used as a decorator with options
    if (targetOrOptions && typeof targetOrOptions === 'object' && !context) {
      const options = targetOrOptions as ComponentOptions;
      return (target: any, context: ClassMethodDecoratorContext) => {
        return this.registerComponent(target, options.name || context.name?.toString() || 'unknown');
      };
    }

    // If used as a simple decorator
    if (targetOrOptions && context) {
      return this.registerComponent(
        targetOrOptions,
        context.name?.toString() || 'unknown'
      );
    }

    // If used as a factory
    return (target: any, context: ClassMethodDecoratorContext) => {
      return this.registerComponent(target, context.name?.toString() || 'unknown');
    };
  }

  /**
   * Internal method to register a component
   */
  public registerComponent<TInput, TOutput>(
    func: (input: TInput) => TOutput,
    name: string
  ): (input: TInput) => TOutput {
    // For TypeScript, we can't introspect types at runtime like Python
    // So we need to infer schemas from sample data or provide them manually
    // For now, we'll create empty schemas
    const inputSchema: Record<string, any> = {};
    const outputSchema: Record<string, any> = {};

    this.components.set(name, {
      name,
      function: func,
      inputSchema,
      outputSchema,
    });

    return func;
  }

  /**
   * Get a component by URL
   */
  public getComponent(componentUrl: string): ComponentEntry<any, any> | undefined {
    const parsedUrl = url.parse(componentUrl);
    let componentName = parsedUrl.host || '';
    if (parsedUrl.pathname && parsedUrl.pathname !== '/') {
      componentName += parsedUrl.pathname;
    }

    return this.components.get(componentName);
  }

  /**
   * Handle an incoming message
   */
  private async handleMessage(request: Incoming): Promise<MethodResponse | null> {
    if (!request.id && request.method === 'initialized') {
      this.initialized = true;
      return null;
    }

    if (!this.initialized && request.method !== 'initialize') {
      return createErrorResponse(
        request.id || '',
        -32002,
        'Server not initialized',
        null
      );
    }

    const id = request.id || '';

    switch (request.method) {
      case 'initialize':
        return createSuccessResponse(id, { server_protocol_version: 1 });

      case 'component_info': {
        const infoRequest = request.params as protocol.ComponentInfoRequest;
        const component = this.getComponent(infoRequest.component);

        if (!component) {
          return createErrorResponse(
            id,
            -32601,
            `Component ${infoRequest.component} not found`,
            null
          );
        }

        return createSuccessResponse(id, {
          input_schema: component.inputSchema,
          output_schema: component.outputSchema,
        });
      }

      case 'component_execute': {
        const executeRequest = request.params as protocol.ComponentExecuteRequest;
        const component = this.getComponent(executeRequest.component);

        if (!component) {
          return createErrorResponse(
            id,
            -32601,
            `Component ${executeRequest.component} not found`,
            null
          );
        }

        try {
          // Execute the component function with the input
          const output = component.function(executeRequest.input);

          return createSuccessResponse(id, {
            output
          });
        } catch (e) {
          return createErrorResponse(
            id,
            -32000,
            e instanceof Error ? e.message : String(e),
            null
          );
        }
      }

      case 'list_components':
        return createSuccessResponse(id, {
          components: Array.from(this.components.keys())
        });

      default:
        return createErrorResponse(
          id,
          -32601,
          `Unknown method: ${request.method}`,
          null
        );
    }
  }

  /**
   * Start the server
   */
  public async start(): Promise<void> {
    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
      terminal: false
    });

    console.error('Starting server...');

    for await (const line of this.rl) {
      if (!line) continue;

      try {
        const request = parseIncoming(line);
        console.error(`Received request: ${JSON.stringify(request)}`);

        const response = await this.handleMessage(request);

        if (response) {
          console.error(`Sending response: ${JSON.stringify(response)}`);
          const responseStr = serializeResponse(response) + '\n';
          process.stdout.write(responseStr);
        }
      } catch (e) {
        console.error(`Error: ${e}`);

        // Send error response
        if (typeof line === 'string') {
          try {
            const request = JSON.parse(line);
            const id = request.id || '';

            const errorResponse = createErrorResponse(
              id,
              -32000,
              e instanceof Error ? e.message : String(e),
              null
            );

            process.stdout.write(serializeResponse(errorResponse) + '\n');
          } catch (parseError) {
            // If we can't parse the request, we can't send a proper response
            console.error(`Failed to parse request: ${parseError}`);
          }
        }
      }
    }
  }

  /**
   * Run the server (wrapper for start)
   */
  public run(): void {
    this.start().catch(err => {
      console.error('Server error:', err);
      process.exit(1);
    });
  }

  /**
   * Set input and output schema for a component
   */
  public setComponentSchema(
    name: string,
    inputSchema: Record<string, any>,
    outputSchema: Record<string, any>
  ): void {
    const component = this.components.get(name);
    if (component) {
      component.inputSchema = inputSchema;
      component.outputSchema = outputSchema;
    }
  }
}