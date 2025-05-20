// Equivalent to protocol.py
// Contains the data structures for the protocol

export interface InitializeRequest {
  runtime_protocol_version: number;
}

export interface InitializeResponse {
  server_protocol_version: number;
}

export interface Initialized {
  // Empty interface for initialized notification
}

export interface ListComponentsRequest {
  // Empty interface for list components request
}

export interface ListComponentsResponse {
  components: string[];
}

export interface ComponentInfoRequest {
  component: string;
  input_schema?: Record<string, any>;
}

export interface ComponentInfoResponse {
  input_schema: Record<string, any>;
  output_schema: Record<string, any>;
}

export interface ComponentExecuteRequest {
  component: string;
  input: any;
}

export interface ComponentExecuteResponse {
  output: any;
}