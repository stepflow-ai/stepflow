import express, { Request, Response } from 'express';
import { createSession } from 'better-sse';
import { v4 as uuidv4 } from 'uuid';
import { StepflowStdioServer, ComponentEntry } from './server';
import { BidirectionalTransport, StepflowContext } from './transport';
import {
  MethodRequest,
  MethodSuccess,
  MethodError,
  InitializeParams,
  ComponentListParams,
  ComponentInfoParams,
  ComponentExecuteParams,
  ComponentInfo,
  ListComponentsResult,
  ComponentInfoResult,
  ComponentExecuteResult
} from './protocol';

interface Session {
  id: string;
  transport: BidirectionalTransport;
  eventQueue: Array<any>;
  context: StepflowContext;
}

export class StepflowHttpServer {
  private app: express.Application;
  private server: StepflowStdioServer;
  private sessions: Map<string, Session>;
  private host: string;
  private port: number;

  constructor(server: StepflowStdioServer, host: string = 'localhost', port: number = 8080) {
    this.app = express();
    this.server = server;
    this.sessions = new Map();
    this.host = host;
    this.port = port;

    this.setupMiddleware();
    this.setupRoutes();
  }

  private setupMiddleware(): void {
    this.app.use(express.json());

    // Enable CORS for all origins
    this.app.use((req, res, next) => {
      res.header('Access-Control-Allow-Origin', '*');
      res.header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
      res.header('Access-Control-Allow-Headers', 'Content-Type');
      next();
    });
  }

  private setupRoutes(): void {
    // SSE endpoint for session negotiation and bidirectional communication
    this.app.get('/runtime/events', async (req: Request, res: Response) => {
      const session = await createSession(req, res);
      const sessionId = uuidv4();

      // Create transport and context for this session
      const transport = new BidirectionalTransport(
        // Write function - not used for HTTP mode
        async (data: string) => { }
      );
      const context = transport.createContext();

      // Store session
      const sessionData: Session = {
        id: sessionId,
        transport,
        eventQueue: [],
        context
      };
      this.sessions.set(sessionId, sessionData);

      // Send endpoint event with session ID
      session.push({
        type: 'endpoint',
        data: JSON.stringify({ endpoint: `/?sessionId=${sessionId}` })
      });

      // Setup event forwarding from transport to SSE
      transport.onMessage((message) => {
        sessionData.eventQueue.push(message);
        session.push({
          type: 'message',
          data: JSON.stringify(message)
        });
      });

      // Keepalive interval
      const keepaliveInterval = setInterval(() => {
        session.push({ type: 'keepalive' });
      }, 30000);

      // Cleanup on disconnect
      session.on('disconnected', () => {
        clearInterval(keepaliveInterval);
        this.sessions.delete(sessionId);
      });

      // Auto-initialize the server for HTTP mode
      await this.handleInitialize(sessionData);
    });

    // JSON-RPC endpoint
    this.app.post('/', async (req: Request, res: Response) => {
      const sessionId = req.query.sessionId as string;

      if (!sessionId) {
        res.json(this.createErrorResponse(null, -32600, 'Missing sessionId parameter'));
        return;
      }

      const session = this.sessions.get(sessionId);
      if (!session) {
        res.json(this.createErrorResponse(null, -32600, 'Invalid sessionId'));
        return;
      }

      const request = req.body as MethodRequest;

      try {
        const response = await this.handleRequest(request, session);
        res.json(response);
      } catch (error: any) {
        res.json(this.createErrorResponse(request.id, -32603, error.message));
      }
    });

    // Health check endpoint
    this.app.get('/health', (req: Request, res: Response) => {
      res.json({ status: 'ok' });
    });
  }

  private async handleInitialize(session: Session): Promise<void> {
    // Auto-initialize for HTTP mode
    const initRequest: MethodRequest = {
      jsonrpc: '2.0',
      id: 'init',
      method: 'initialize',
      params: {
        runtime_protocol_version: 1,
      } as InitializeParams
    };

    await this.handleRequest(initRequest, session);
  }

  private async handleRequest(request: MethodRequest, session: Session): Promise<MethodSuccess | MethodError> {
    switch (request.method) {
      case 'components/list':
        return this.handleComponentsList(request);

      case 'components/info':
        return this.handleComponentsInfo(request);

      case 'components/execute':
        return this.handleComponentsExecute(request, session);

      case 'initialize':
        return {
          jsonrpc: '2.0',
          id: request.id,
          result: {
            server_protocol_version: 1
          }
        };

      default:
        return this.createErrorResponse(request.id, -32601, `Method not found: ${request.method}`);
    }
  }

  private handleComponentsList(request: MethodRequest): MethodSuccess {
    const components = this.server.getComponents();

    const result: ListComponentsResult = {
      components: components.map(name => {
        const comp = this.server.getComponent(name);
        return {
          component: name,
          description: comp?.options?.description,
          input_schema: comp?.options?.inputSchema,
          output_schema: comp?.options?.outputSchema
        } as ComponentInfo;
      })
    };

    return {
      jsonrpc: '2.0',
      id: request.id,
      result
    };
  }

  private handleComponentsInfo(request: MethodRequest): MethodSuccess | MethodError {
    const params = request.params as ComponentInfoParams;
    const component = this.server.getComponent(params.component);

    if (!component) {
      return this.createErrorResponse(request.id, -32001, `Component not found: ${params.component}`);
    }

    const result: ComponentInfoResult = {
      info: {
        component: params.component,
        description: component.options?.description,
        input_schema: component.options?.inputSchema,
        output_schema: component.options?.outputSchema
      }
    };

    return {
      jsonrpc: '2.0',
      id: request.id,
      result
    };
  }

  private async handleComponentsExecute(
    request: MethodRequest,
    session: Session
  ): Promise<MethodSuccess | MethodError> {
    const params = request.params as ComponentExecuteParams;
    const component = this.server.getComponent(params.component);

    if (!component) {
      return this.createErrorResponse(request.id, -32001, `Component not found: ${params.component}`);
    }

    try {
      // Execute the component with session-specific context
      const output = await component.handler(params.input, session.context);

      const result: ComponentExecuteResult = {
        output
      };

      return {
        jsonrpc: '2.0',
        id: request.id,
        result
      };
    } catch (error: any) {
      return this.createErrorResponse(request.id, -32603, error.message);
    }
  }

  private createErrorResponse(id: string | number | null, code: number, message: string): MethodError {
    return {
      jsonrpc: '2.0',
      id: id || 0,
      error: {
        code,
        message
      }
    };
  }

  public async start(): Promise<void> {
    return new Promise((resolve) => {
      this.app.listen(this.port, this.host, () => {
        console.error(`Stepflow HTTP server running at http://${this.host}:${this.port}`);
        resolve();
      });
    });
  }
}