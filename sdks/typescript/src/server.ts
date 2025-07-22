// Equivalent to server.py
// Contains the server implementation

import * as readline from 'readline';
import * as url from 'url';
import { Incoming, MethodResponse, createSuccessResponse, createErrorResponse, parseIncoming, serializeResponse, BidirectionalTransport, StepflowContext } from './transport';
import * as protocol from './protocol';

/**
 * Component entry in the server registry
 */
export interface ComponentEntry<TInput, TOutput> {
  name: string;
  function: (input: TInput, context?: StepflowContext) => TOutput | Promise<TOutput>;
  inputSchema?: Record<string, any>;
  outputSchema?: Record<string, any>;
  description?: string;
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
  private transport?: BidirectionalTransport;
  private protocolPrefix: string = 'typescript';

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
    func: (input: TInput, context?: StepflowContext) => TOutput | Promise<TOutput>,
    name: string,
    options?: {
      description?: string;
      inputSchema?: Record<string, any>;
      outputSchema?: Record<string, any>;
    }
  ): (input: TInput, context?: StepflowContext) => TOutput | Promise<TOutput> {
    this.components.set(name, {
      name,
      function: func,
      inputSchema: options?.inputSchema,
      outputSchema: options?.outputSchema,
      description: options?.description,
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
      case 'initialize': {
        const initRequest = request.params as protocol.InitializeParams;
        this.protocolPrefix = initRequest.protocol_prefix;
        return createSuccessResponse(id, { server_protocol_version: 1 });
      }

      case 'components/info': {
        const infoRequest = request.params as protocol.ComponentInfoParams;
        const component = this.getComponent(infoRequest.component);

        if (!component) {
          return createErrorResponse(
            id,
            -32601,
            `Component ${infoRequest.component} not found`,
            null
          );
        }

        const componentInfo: protocol.ComponentInfo = {
          component: infoRequest.component,
          description: component.description,
          input_schema: component.inputSchema,
          output_schema: component.outputSchema,
        };

        return createSuccessResponse(id, { info: componentInfo });
      }

      case 'components/execute': {
        const executeRequest = request.params as protocol.ComponentExecuteParams;
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
          // Create context for bidirectional communication
          const context = this.transport?.createContext();
          
          // Execute the component function with the input and context
          const output = await component.function(executeRequest.input, context);

          return createSuccessResponse(id, { output });
        } catch (e) {
          return createErrorResponse(
            id,
            -32000,
            e instanceof Error ? e.message : String(e),
            null
          );
        }
      }

      case 'components/list': {
        const componentInfos: protocol.ComponentInfo[] = Array.from(this.components.values()).map(comp => ({
          component: comp.name,
          description: comp.description,
          input_schema: comp.inputSchema,
          output_schema: comp.outputSchema,
        }));
        
        return createSuccessResponse(id, { components: componentInfos });
      }

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

    // Initialize bidirectional transport
    this.transport = new BidirectionalTransport((message: string) => {
      process.stdout.write(message + '\n');
    });

    console.error('Starting server...');

    for await (const line of this.rl) {
      if (!line) continue;

      try {
        const message = JSON.parse(line);
        console.error(`Received message: ${JSON.stringify(message)}`);

        // Check if this is a response to our outgoing request
        if (message.id && ('result' in message || 'error' in message)) {
          this.transport.handleResponse(message);
          continue;
        }

        // Handle incoming request
        const request = parseIncoming(line);
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
   * @deprecated Use registerComponent with options instead
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