import { beforeEach, describe, expect, it } from "vitest";

import {
  clearTourCompletion,
  isTourCompleted,
  markTourCompleted,
  tourStorageKey,
} from "./tour-state";

describe("tour completion state", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("persists completion per tour id", () => {
    expect(tourStorageKey("home")).toBe("xvn.tours.completed.home");
    expect(isTourCompleted("home")).toBe(false);

    markTourCompleted("home");

    expect(localStorage.getItem("xvn.tours.completed.home")).toBe("1");
    expect(isTourCompleted("home")).toBe(true);
    expect(isTourCompleted("authoring")).toBe(false);
  });

  it("clears one tour without resetting other tours", () => {
    markTourCompleted("home");
    markTourCompleted("authoring");

    clearTourCompletion("home");

    expect(isTourCompleted("home")).toBe(false);
    expect(isTourCompleted("authoring")).toBe(true);
  });
});
