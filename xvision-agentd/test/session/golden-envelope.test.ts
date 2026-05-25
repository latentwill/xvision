/**
 * §2-A review finding (a) — vitest half of the shared golden fixture check.
 *
 * The fixture `crates/xvision-engine/tests/fixtures/trajectory_golden_envelopes.json`
 * is the single source of truth for the `event.trajectory_frame` wire shape.
 * The Rust half (`crates/xvision-engine/tests/cline_eval_recording.rs ::
 * golden_envelopes_parse_on_rust_side`) asserts the production
 * `parse_trajectory_frame_notification` accepts every envelope.
 *
 * This test asserts the OTHER direction: the sidecar's `emitFrame` produces
 * envelopes whose shape matches the golden fixture — same envelope keys, same
 * per-variant frame field set. If `emit.ts` / `frame-recorder.ts` and the
 * fixture ever diverge, this fails, so the cross-language contract can't drift
 * silently in either direction.
 */
import { describe, it, expect } from "vitest"
import * as fs from "node:fs"
import * as path from "node:path"
import { fileURLToPath } from "node:url"
import { emitFrame, type TrajectoryFrameEnvelope } from "../../src/session/emit.js"
import * as eventClient from "../../src/transport/event-client.js"
import { vi } from "vitest"

const __dirname = path.dirname(fileURLToPath(import.meta.url))

function loadGolden(): {
  run_id: string
  slot_role: string
  envelopes: TrajectoryFrameEnvelope[]
} {
  // From xvision-agentd/test/session → repo root → engine fixture.
  const fixturePath = path.resolve(
    __dirname,
    "../../../crates/xvision-engine/tests/fixtures/trajectory_golden_envelopes.json",
  )
  const raw = fs.readFileSync(fixturePath, "utf8")
  return JSON.parse(raw)
}

describe("golden trajectory envelope (shared cross-language fixture)", () => {
  it("emitFrame produces the exact envelope shape in the golden fixture", () => {
    const golden = loadGolden()
    expect(golden.envelopes.length).toBe(7)

    // Spy emitNotification so we capture what emitFrame puts on the wire
    // without needing a socket.
    const spy = vi.spyOn(eventClient, "emitNotification").mockResolvedValue(undefined)
    try {
      for (const env of golden.envelopes) {
        emitFrame(env)
      }

      const emitted = spy.mock.calls
        .filter((c) => c[0] === "event.trajectory_frame")
        .map((c) => c[1] as TrajectoryFrameEnvelope)

      expect(emitted.length).toBe(golden.envelopes.length)

      for (let i = 0; i < emitted.length; i++) {
        const got = emitted[i]!
        const want = golden.envelopes[i]!
        // Envelope coordinate keys match exactly.
        expect(Object.keys(got).sort()).toEqual(
          ["frame", "frame_index", "run_id", "slot_role", "step_index"],
        )
        expect(got.run_id).toBe(want.run_id)
        expect(got.slot_role).toBe(want.slot_role)
        expect(got.step_index).toBe(want.step_index)
        expect(got.frame_index).toBe(want.frame_index)
        // The frame body round-trips byte-for-byte (same kind + fields).
        expect(got.frame).toEqual(want.frame)
      }
    } finally {
      spy.mockRestore()
    }
  })

  it("golden fixture covers every TrajectoryFrame variant in order", () => {
    const golden = loadGolden()
    const kinds = golden.envelopes.map((e) => (e.frame as { kind: string }).kind)
    expect(kinds).toEqual([
      "Request",
      "TextDelta",
      "ReasoningDelta",
      "ToolCallDelta",
      "ToolResult",
      "Usage",
      "Finish",
    ])
  })
})
