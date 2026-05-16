import { PROTOCOL_VERSION, SIDECAR_VERSION } from "../version.js"
import { registerMethod } from "./index.js"

export interface RuntimeHealthResult {
  protocol_version: string
  sidecar_version: string
  cline_sdk_version: string
  status: "ok"
}

export function handleRuntimeHealth(): RuntimeHealthResult {
  return {
    protocol_version: PROTOCOL_VERSION,
    sidecar_version: SIDECAR_VERSION,
    cline_sdk_version: "unbound",
    status: "ok",
  }
}

registerMethod("runtime.health", () => handleRuntimeHealth())
