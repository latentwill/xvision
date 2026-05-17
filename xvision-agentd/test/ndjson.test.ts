import { describe, expect, it } from "vitest"
import { encodeNdjson, NdjsonDecoder } from "../src/transport/ndjson.js"

describe("ndjson framing", () => {
  it("encodes an object as a single line", () => {
    const out = encodeNdjson({ a: 1 })
    expect(out).toBe('{"a":1}\n')
  })

  it("decodes a stream of two messages across chunk boundaries", () => {
    const dec = new NdjsonDecoder()
    const events: unknown[] = []
    dec.on("message", (m) => events.push(m))
    dec.push(Buffer.from('{"a":1}\n{"b":'))
    dec.push(Buffer.from('2}\n'))
    expect(events).toEqual([{ a: 1 }, { b: 2 }])
  })

  it("emits a parse error on invalid json", () => {
    const dec = new NdjsonDecoder()
    const errors: Error[] = []
    dec.on("error", (e) => errors.push(e))
    dec.push(Buffer.from("not json\n"))
    expect(errors).toHaveLength(1)
  })
})
