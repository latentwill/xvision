import { describe, expect, it } from "vitest"
import { CLINE_SDK_VERSION, PROTOCOL_VERSION, SIDECAR_VERSION } from "../src/version.js"

describe("version constants", () => {
  it("exposes a protocol version", () => {
    expect(PROTOCOL_VERSION).toMatch(/^\d+\.\d+\.\d+$/)
  })
  it("exposes a sidecar version", () => {
    expect(SIDECAR_VERSION).toMatch(/^\d+\.\d+\.\d+$/)
  })
  it("reports a resolved @cline/sdk semver (not unbound)", () => {
    expect(CLINE_SDK_VERSION).toMatch(/^\d+\.\d+\.\d+/)
    expect(CLINE_SDK_VERSION).not.toBe("unbound")
    expect(CLINE_SDK_VERSION).not.toBe("unknown")
  })
})
