import { afterEach, describe, expect, it } from "vitest";
import { isEnabled, isShadow, pollIntervalOverrideS } from "../src/modes/env.js";

const KEYS = ["AGENT_CONDUCTOR_SHADOW", "AGENT_CONDUCTOR_ENABLE", "AGENT_CONDUCTOR_POLL_S"];

afterEach(() => {
  for (const k of KEYS) delete process.env[k];
});

describe("env-driven mode flags", () => {
  it("shadow defaults off", () => {
    expect(isShadow()).toBe(false);
    process.env["AGENT_CONDUCTOR_SHADOW"] = "1";
    expect(isShadow()).toBe(true);
    process.env["AGENT_CONDUCTOR_SHADOW"] = "false";
    expect(isShadow()).toBe(false);
  });

  it("enabled defaults on", () => {
    expect(isEnabled()).toBe(true);
    process.env["AGENT_CONDUCTOR_ENABLE"] = "0";
    expect(isEnabled()).toBe(false);
    process.env["AGENT_CONDUCTOR_ENABLE"] = "yes";
    expect(isEnabled()).toBe(true);
  });

  it("poll override parses positive numbers only", () => {
    expect(pollIntervalOverrideS()).toBeNull();
    process.env["AGENT_CONDUCTOR_POLL_S"] = "0";
    expect(pollIntervalOverrideS()).toBeNull();
    process.env["AGENT_CONDUCTOR_POLL_S"] = "abc";
    expect(pollIntervalOverrideS()).toBeNull();
    process.env["AGENT_CONDUCTOR_POLL_S"] = "60";
    expect(pollIntervalOverrideS()).toBe(60);
  });
});
