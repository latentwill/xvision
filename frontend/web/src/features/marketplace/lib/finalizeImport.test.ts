// finalizeImport.test.ts — bounded retry on license-not-yet-visible.
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/client";
import { SealedGateError } from "./sealed";
import {
  finalizeImportWithRetry,
  isLicenseNotYetVisible,
} from "./finalizeImport";

const noSleep = async () => {};

function api403() {
  return new ApiError(403, "forbidden", "No license held for this wallet.");
}

describe("isLicenseNotYetVisible", () => {
  it("true for an ApiError 403", () => {
    expect(isLicenseNotYetVisible(api403())).toBe(true);
  });
  it("false for an ApiError 409 (hash mismatch is terminal)", () => {
    expect(
      isLicenseNotYetVisible(new ApiError(409, "conflict", "hash mismatch")),
    ).toBe(false);
  });
  it("true for a SealedGateError naming the license", () => {
    expect(
      isLicenseNotYetVisible(new SealedGateError("no license held")),
    ).toBe(true);
  });
  it("false for a generic SealedGateError (e.g. bad sig)", () => {
    expect(
      isLicenseNotYetVisible(new SealedGateError("expired signature")),
    ).toBe(false);
  });
  it("false for a plain Error / network error", () => {
    expect(isLicenseNotYetVisible(new Error("network down"))).toBe(false);
  });
});

describe("finalizeImportWithRetry", () => {
  it("resolves on the first attempt without retrying", async () => {
    const importFn = vi.fn(async () => ({ agent_id: "NEW" }));
    const out = await finalizeImportWithRetry(importFn, { sleep: noSleep });
    expect(out).toEqual({ agent_id: "NEW" });
    expect(importFn).toHaveBeenCalledTimes(1);
  });

  it("retries on 403 twice then resolves", async () => {
    const importFn = vi
      .fn<() => Promise<{ agent_id: string }>>()
      .mockRejectedValueOnce(api403())
      .mockRejectedValueOnce(api403())
      .mockResolvedValueOnce({ agent_id: "NEW" });
    const out = await finalizeImportWithRetry(importFn, {
      attempts: 5,
      sleep: noSleep,
    });
    expect(out).toEqual({ agent_id: "NEW" });
    expect(importFn).toHaveBeenCalledTimes(3);
  });

  it("rejects after exhausting attempts on a permanent 403", async () => {
    const importFn = vi.fn(async () => {
      throw api403();
    });
    await expect(
      finalizeImportWithRetry(importFn, { attempts: 3, sleep: noSleep }),
    ).rejects.toBeInstanceOf(ApiError);
    expect(importFn).toHaveBeenCalledTimes(3);
  });

  it("does NOT retry a non-retryable error (409) — fails fast", async () => {
    const importFn = vi.fn(async () => {
      throw new ApiError(409, "conflict", "hash mismatch");
    });
    await expect(
      finalizeImportWithRetry(importFn, { attempts: 5, sleep: noSleep }),
    ).rejects.toThrow(/hash mismatch/);
    expect(importFn).toHaveBeenCalledTimes(1);
  });
});
