import { z } from 'zod';

// Common JSON-RPC 2.0 types
export const jsonRpcVersion = z.literal('2.0');
export const jsonRpcId = z.union([z.string(), z.number(), z.null()]);

// Error object
export const jsonRpcError = z.object({
  code: z.number(),
  message: z.string(),
  data: z.any().optional()
});

// Request/Response base
export const jsonRpcBase = z.object({
  jsonrpc: jsonRpcVersion,
  id: jsonRpcId
});

// Request method
export const requestMethod = z.union([
  z.literal('initialize'),
  z.literal('listComponents'),
  z.literal('componentInfo'),
  z.literal('componentExecute'),
  z.literal('stepArguments')
]);

// Request parameters
export const initializeRequest = z.object({
  clientInfo: z.object({
    name: z.string(),
    version: z.string()
  }).optional()
});

export const listComponentsRequest = z.object({
  // No specific parameters for listComponents
});

export const componentInfoRequest = z.object({
  component: z.string()
});

export const componentExecuteRequest = z.object({
  component: z.string(),
  input: z.record(z.any())
});

export const stepArgumentsRequest = z.object({
  step: z.object({
    id: z.string(),
    component: z.string(),
    input: z.record(z.any())
  })
});

// Request
const requestParams = z.union([
  initializeRequest,
  listComponentsRequest,
  componentInfoRequest,
  componentExecuteRequest,
  stepArgumentsRequest
]);

export const methodRequest = jsonRpcBase.extend({
  method: requestMethod,
  params: requestParams
}) as z.ZodType<{
  jsonrpc: '2.0';
  id: string | number | null;
  method: 'initialize' | 'listComponents' | 'componentInfo' | 'componentExecute' | 'stepArguments';
  params: z.infer<typeof requestParams>;
}>;

// Response results
export const initializeResponse = z.object({
  serverInfo: z.object({
    name: z.string(),
    version: z.string()
  })
});

export const listComponentsResponse = z.object({
  components: z.array(z.string())
});

export const componentInfoResponse = z.object({
  name: z.string(),
  description: z.string().optional(),
  inputSchema: z.any().optional(),
  outputSchema: z.any().optional()
});

export const componentExecuteResponse = z.object({
  result: z.any()
});

export const stepArgumentsResponse = z.object({
  input: z.record(z.any())
});

// Response
const responseResult = z.union([
  initializeResponse,
  listComponentsResponse,
  componentInfoResponse,
  componentExecuteResponse,
  stepArgumentsResponse
]);

export const methodResponseSuccess = jsonRpcBase.extend({
  result: responseResult
}) as z.ZodType<{
  jsonrpc: '2.0';
  id: string | number | null;
  result: z.infer<typeof responseResult>;
}>;

export const methodResponseError = jsonRpcBase.extend({
  error: jsonRpcError
}) as z.ZodType<{
  jsonrpc: '2.0';
  id: string | number | null;
  error: {
    code: number;
    message: string;
    data?: any;
  };
}>;

// Notification
export const initializedNotification = z.object({
  // No parameters for initialized notification
});

export const notification = z.object({
  jsonrpc: jsonRpcVersion,
  method: z.literal('initialized'),
  params: initializedNotification.optional()
});

// Protocol Schema
export const protocolSchema = z.union([
  methodRequest,
  methodResponseSuccess,
  methodResponseError,
  notification
]) as z.ZodType<
  | z.infer<typeof methodRequest>
  | z.infer<typeof methodResponseSuccess>
  | z.infer<typeof methodResponseError>
  | z.infer<typeof notification>
>;

export type ProtocolMessage = z.infer<typeof protocolSchema>;
