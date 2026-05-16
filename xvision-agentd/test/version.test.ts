import { describe, expect, it } from "vitest"
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "../src/version.js"

describe("version constants", () => {
  it("exposes a protocol version", () => {
    expect(PROTOCOL_VERSION).toMatch(/^\d+\.\d+\.\d+$/)
  })
  it("exposes a sidecar version", () => {
    expect(SIDECAR_VERSION).toMatch(/^\d+\.\d+\.\d+$/)
  })
})
