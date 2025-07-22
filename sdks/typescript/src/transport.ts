// Transport layer for StepFlow TypeScript SDK
// Handles JSON-RPC message parsing and bidirectional communication

import { v4 as uuidv4 } from 'uuid';
import * as protocol from './protocol';

/**
 * Message sent to request a method execution.
 */
export interface Incoming {
  /**
   * The JSON-RPC version (must be "2.0")
   */
  jsonrpc: string;

  /**
   * The request id. If not set, this is a notification.
   */
  id?: string;

  /**
   * The method to execute.
   */
  method: string;

  /**
   * The parameters to pass to the method.
   */
  params: any;
}

/**
 * Context for component execution that allows bidirectional communication
 */
export interface StepflowContext {
  /**
   * Store data as a blob and return its ID
   */
  putBlob(data: protocol.Value): Promise<protocol.BlobId>;

  /**
   * Retrieve data by blob ID
   */
  getBlob(blobId: protocol.BlobId): Promise<protocol.Value>;

  /**
   * Evaluate a flow with given input
   */
  evaluateFlow(flow: any, input: protocol.Value): Promise<any>;
}

/**
 * The error that occurred during the method execution.
 */
export interface RemoteError {
  /**
   * The error code.
   */
  code: number;

  /**
   * The error message.
   */
  message: string;

  /**
   * The error data.
   */
  data: any;
}

/**
 * Message sent in response to a method request.
 */
export interface MethodResponse {
  /**
   * The JSON-RPC version (must be "2.0")
   */
  jsonrpc: string;

  /**
   * The request id.
   */
  id: string;

  /**
   * The result of the method execution.
   */
  result?: any;

  /**
   * The error that occurred during the method execution.
   */
  error?: RemoteError;
}

/**
 * Create a new method response with a result
 */
export function createSuccessResponse(id: string, result: any): MethodResponse {
  return {
    jsonrpc: "2.0",
    id,
    result
  };
}

/**
 * Create a new method response with an error
 */
export function createErrorResponse(id: string, code: number, message: string, data?: any): MethodResponse {
  return {
    jsonrpc: "2.0",
    id,
    error: {
      code,
      message,
      data: data || null
    }
  };
}

/**
 * Parse an incoming message from JSON
 */
export function parseIncoming(message: string): Incoming {
  return JSON.parse(message) as Incoming;
}

/**
 * Serialize a method response to JSON
 */
export function serializeResponse(response: MethodResponse): string {
  return JSON.stringify(response);
}

/**
 * Bidirectional communication helper
 */
export class BidirectionalTransport {
  private pendingRequests = new Map<string, {
    resolve: (value: any) => void;
    reject: (error: any) => void;
  }>();

  constructor(private sendMessage: (message: string) => void) {}

  /**
   * Send a request and wait for response
   */
  async sendRequest(method: protocol.Method, params: any): Promise<any> {
    const id = uuidv4();
    const request: protocol.MethodRequest = {
      jsonrpc: "2.0",
      id,
      method,
      params
    };

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
      
      // Set timeout to avoid hanging forever
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error(`Request timeout for method ${method}`));
        }
      }, 30000); // 30 second timeout

      this.sendMessage(JSON.stringify(request));
    });
  }

  /**
   * Handle incoming response message
   */
  handleResponse(message: protocol.MethodSuccess | protocol.MethodError): void {
    const pending = this.pendingRequests.get(String(message.id));
    if (!pending) {
      console.warn(`Received response for unknown request ID: ${message.id}`);
      return;
    }

    this.pendingRequests.delete(String(message.id));

    if ('result' in message) {
      pending.resolve(message.result);
    } else {
      pending.reject(new Error(message.error.message));
    }
  }

  /**
   * Create a StepflowContext for component execution
   */
  createContext(): StepflowContext {
    return {
      putBlob: async (data: protocol.Value): Promise<protocol.BlobId> => {
        const result = await this.sendRequest('blobs/put', { data });
        return result.blob_id;
      },

      getBlob: async (blobId: protocol.BlobId): Promise<protocol.Value> => {
        const result = await this.sendRequest('blobs/get', { blob_id: blobId });
        return result.data;
      },

      evaluateFlow: async (flow: any, input: protocol.Value): Promise<any> => {
        const result = await this.sendRequest('flows/evaluate', { flow, input });
        return result.result;
      }
    };
  }
}