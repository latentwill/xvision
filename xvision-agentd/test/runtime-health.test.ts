import { describe, expect, it } from "vitest"
import { handleRuntimeHealth } from "../src/methods/runtime-health.js"
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "../src/version.js"

describe("runtime.health", () => {
  it("returns protocol + sidecar + cline_sdk versions and status:ok", () => {
    const result = handleRuntimeHealth()
    expect(result.protocol_version).toBe(PROTOCOL_VERSION)
    expect(result.sidecar_version).toBe(SIDECAR_VERSION)
    expect(result.status).toBe("ok")
  })

  it("reports a resolved @cline/sdk semver", async () => {
    const res = await handleRuntimeHealth() as { cline_sdk_version: string }
    expect(res.cline_sdk_version).toMatch(/^\d+\.\d+\.\d+/)
    expect(res.cline_sdk_version).not.toBe("unbound")
  })
})
