// Equivalent to transport.py
// Contains the message transport classes

import { v4 as uuidv4 } from 'uuid';

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