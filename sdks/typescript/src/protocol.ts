// Protocol definitions for StepFlow TypeScript SDK
// Based on the JSON schema in ../../schemas/protocol.json

// Note: Import from schemas when available
// import type { protocolSchema } from '@stepflow/schemas';

// === JSON-RPC Base Types ===
export type JsonRpcVersion = "2.0";
export type RequestId = string | number;

// === Method Types ===
export type Method = 
  | "initialize"
  | "initialized"
  | "components/list"
  | "components/info"
  | "components/execute"
  | "blobs/put"
  | "blobs/get"
  | "flows/evaluate";

// === Component and Value Types ===
export type Component = string; // e.g., "/builtin/eval", "/python/udf"
export type Value = any; // Any JSON value
export type BlobId = string; // SHA-256 hash

// === Request Parameter Types ===
export interface InitializeParams {
  runtime_protocol_version: number;
  protocol_prefix: string;
}

export interface ComponentExecuteParams {
  component: Component;
  input: Value;
}

export interface ComponentInfoParams {
  component: Component;
}

export interface ComponentListParams {
  // Empty for components/list
}

export interface GetBlobParams {
  blob_id: BlobId;
}

export interface PutBlobParams {
  data: Value;
}

export interface EvaluateFlowParams {
  flow: any; // Flow type from workflow schema
  input: Value;
}

export interface Initialized {
  // Empty for initialized notification
}

// === Response Result Types ===
export interface InitializeResult {
  server_protocol_version: number;
}

export interface ComponentInfo {
  component: Component;
  description?: string;
  input_schema?: any;
  output_schema?: any;
}

export interface ComponentExecuteResult {
  output: Value;
}

export interface ComponentInfoResult {
  info: ComponentInfo;
}

export interface ListComponentsResult {
  components: ComponentInfo[];
}

export interface GetBlobResult {
  data: Value;
}

export interface PutBlobResult {
  blob_id: BlobId;
}

export interface EvaluateFlowResult {
  result: any; // FlowResult from schema
}

// === Error Types ===
export interface RemoteError {
  code: number;
  message: string;
  data?: any;
}

// === Message Types ===
export interface MethodRequest {
  jsonrpc: JsonRpcVersion;
  id: RequestId;
  method: Method;
  params: InitializeParams | ComponentExecuteParams | ComponentInfoParams | 
          ComponentListParams | GetBlobParams | PutBlobParams | EvaluateFlowParams;
}

export interface MethodSuccess {
  jsonrpc: JsonRpcVersion;
  id: RequestId;
  result: InitializeResult | ComponentExecuteResult | ComponentInfoResult | 
          ListComponentsResult | GetBlobResult | PutBlobResult | EvaluateFlowResult;
}

export interface MethodError {
  jsonrpc: JsonRpcVersion;
  id: RequestId;
  error: RemoteError;
}

export interface Notification {
  jsonrpc: JsonRpcVersion;
  method: Method;
  params: Initialized;
}

export type Message = MethodRequest | MethodSuccess | MethodError | Notification;

// TODO: Import the protocol schema type from the schemas package when available
// export type ProtocolMessage = typeof protocolSchema;

// === Legacy Interfaces for Backward Compatibility ===
/** @deprecated Use ComponentInfoParams instead */
export interface ComponentInfoRequest extends ComponentInfoParams {}

/** @deprecated Use ComponentInfoResult instead */
export interface ComponentInfoResponse {
  input_schema: Record<string, any>;
  output_schema: Record<string, any>;
}

/** @deprecated Use ComponentExecuteParams instead */
export interface ComponentExecuteRequest extends ComponentExecuteParams {}

/** @deprecated Use ComponentExecuteResult instead */
export interface ComponentExecuteResponse {
  output: any;
}

/** @deprecated Use ComponentListParams instead */
export interface ListComponentsRequest extends ComponentListParams {}

/** @deprecated Use ListComponentsResult instead */
export interface ListComponentsResponse {
  components: string[];
}

/** @deprecated Use InitializeParams instead */
export interface InitializeRequest extends InitializeParams {}

/** @deprecated Use InitializeResult instead */
export interface InitializeResponse extends InitializeResult {}