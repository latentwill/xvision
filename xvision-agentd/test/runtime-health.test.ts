import { describe, expect, it } from "vitest"
import { handleRuntimeHealth } from "../src/methods/runtime-health.js"
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "../src/version.js"

describe("runtime.health", () => {
  it("returns protocol + sidecar + cline_sdk versions and status:ok", () => {
    const result = handleRuntimeHealth()
    expect(result).toEqual({
      protocol_version: PROTOCOL_VERSION,
      sidecar_version: SIDECAR_VERSION,
      cline_sdk_version: "unbound",
      status: "ok",
    })
  })
})
