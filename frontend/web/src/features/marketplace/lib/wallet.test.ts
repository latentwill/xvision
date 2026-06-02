import { renderHook, act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useWallet } from "./wallet";

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("useWallet", () => {
  describe("useWallet_no_ethereum_throws", () => {
    beforeEach(() => {
      Object.defineProperty(window, "ethereum", {
        value: undefined,
        writable: true,
        configurable: true,
      });
    });

    it("connect() rejects when window.ethereum is absent", async () => {
      const { result } = renderHook(() => useWallet());
      await expect(result.current.connect()).rejects.toThrow(
        "MetaMask (or compatible wallet) not detected. Install from metamask.io.",
      );
    });
  });

  describe("useWallet_persists_to_localStorage", () => {
    const MOCK_ADDRESS = "0xABCDEF1234567890abcdef1234567890ABCDEF12";

    beforeEach(() => {
      Object.defineProperty(window, "ethereum", {
        value: {
          request: vi.fn().mockResolvedValue([MOCK_ADDRESS]),
        },
        writable: true,
        configurable: true,
      });
    });

    it("connect() sets localStorage key xvn_wallet_address", async () => {
      const { result } = renderHook(() => useWallet());
      await act(async () => {
        await result.current.connect();
      });
      expect(localStorage.getItem("xvn_wallet_address")).toBe(MOCK_ADDRESS);
    });
  });
});
