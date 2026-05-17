import { registerMethod } from "./index.js"
import { CLINE_SDK_VERSION, PROTOCOL_VERSION, SIDECAR_VERSION } from "../version.js"

interface RuntimeHealthResult {
  protocol_version: string
  sidecar_version: string
  cline_sdk_version: string
  status: "ok"
}

export function handleRuntimeHealth(): RuntimeHealthResult {
  return {
    protocol_version: PROTOCOL_VERSION,
    sidecar_version: SIDECAR_VERSION,
    cline_sdk_version: CLINE_SDK_VERSION,
    status: "ok",
  }
}

registerMethod("runtime.health", () => handleRuntimeHealth())
