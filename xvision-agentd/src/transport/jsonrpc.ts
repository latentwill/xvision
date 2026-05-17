export interface JsonRpcRequest {
  jsonrpc: "2.0"
  id: number | string
  method: string
  params?: unknown
}

export interface JsonRpcSuccess<T = unknown> {
  jsonrpc: "2.0"
  id: number | string
  result: T
}

export interface JsonRpcError {
  jsonrpc: "2.0"
  id: number | string | null
  error: { code: number; message: string; data?: unknown }
}

export interface JsonRpcNotification {
  jsonrpc: "2.0"
  method: string
  params?: unknown
}

export type JsonRpcResponse<T = unknown> = JsonRpcSuccess<T> | JsonRpcError

export const RPC_ERROR_CODES = {
  ParseError: -32700,
  InvalidRequest: -32600,
  MethodNotFound: -32601,
  InvalidParams: -32602,
  InternalError: -32603,
  // xvision-agentd custom range -32000 to -32099
  IncompatibleVersion: -32000,
  ToolError: -32001,
  Cancelled: -32002,
} as const
