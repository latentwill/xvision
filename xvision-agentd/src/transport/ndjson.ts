import { EventEmitter } from "node:events"

export function encodeNdjson(value: unknown): string {
  return JSON.stringify(value) + "\n"
}

export class NdjsonDecoder extends EventEmitter {
  private buffer = ""

  push(chunk: Buffer | string): void {
    this.buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8")
    let idx: number
    while ((idx = this.buffer.indexOf("\n")) !== -1) {
      const line = this.buffer.slice(0, idx)
      this.buffer = this.buffer.slice(idx + 1)
      if (line.length === 0) continue
      try {
        this.emit("message", JSON.parse(line))
      } catch (err) {
        this.emit("error", err instanceof Error ? err : new Error(String(err)))
      }
    }
  }
}
