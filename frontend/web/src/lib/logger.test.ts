import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  clearLogBuffer,
  dumpLogBuffer,
  getLogLevel,
  logDebug,
  logError,
  logInfo,
  sanitizeContext,
  setLogLevel,
} from "./logger";

describe("logger", () => {
  beforeEach(() => {
    clearLogBuffer();
    setLogLevel("debug");
    vi.restoreAllMocks();
  });

  it("redacts secret and content-like keys", () => {
    const out = sanitizeContext({
      api_key: "sk-test",
      authorization: "Bearer secret",
      message: "full user prompt",
      safe: "visible",
      nested: { api_secret_key: "secret" },
    });

    expect(out).toMatchObject({
      api_key: "[redacted]",
      authorization: "[redacted]",
      message: "[redacted]",
      safe: "visible",
      nested: { api_secret_key: "[redacted]" },
    });
  });

  it("honors level filtering", () => {
    const info = vi.spyOn(console, "info").mockImplementation(() => {});
    const debug = vi.spyOn(console, "debug").mockImplementation(() => {});
    setLogLevel("info");

    logDebug("app", "debug.hidden");
    logInfo("app", "info.visible");

    expect(debug).not.toHaveBeenCalled();
    expect(info).toHaveBeenCalledTimes(1);
    expect(dumpLogBuffer()).toHaveLength(1);
  });

  it("caps the ring buffer at 500 entries", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    setLogLevel("error");

    for (let i = 0; i < 510; i += 1) {
      logError("app", "entry", { i });
    }

    const entries = dumpLogBuffer();
    expect(entries).toHaveLength(500);
    expect(entries[0].ctx.i).toBe(10);
  });

  it("persists level changes", () => {
    setLogLevel("warn");

    expect(getLogLevel()).toBe("warn");
    expect(window.localStorage.getItem("xvn.log.level")).toBe("warn");
  });
});
