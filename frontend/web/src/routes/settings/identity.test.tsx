import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { SettingsIdentityRoute } from "./identity";
import type { IdentityReport } from "@/api/types.gen";

vi.mock("@/api/settings", async () => {
  const actual = await vi.importActual<typeof import("@/api/settings")>(
    "@/api/settings",
  );
  return {
    ...actual,
    getIdentity: vi.fn(),
  };
});

const settingsApi = await import("@/api/settings");

function baseReport(overrides: Partial<IdentityReport> = {}): IdentityReport {
  return {
    feature_compiled_in: false,
    wallet: { rpc_url_set: false, wallet_key_set: false },
    note: "v1 read-only",
    agent_token_id: null,
    identity_registry: null,
    last_attestation_tx: null,
    mantlescan_base_url: "https://sepolia.mantlescan.xyz",
    ...overrides,
  };
}

function renderRoute() {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({
          defaultOptions: { queries: { retry: false } },
        })
      }
    >
      <SettingsIdentityRoute />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(settingsApi.getIdentity).mockResolvedValue(baseReport());
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SettingsIdentityRoute", () => {
  it("renders 'not configured' state when API returns no token_id or registry", async () => {
    vi.mocked(settingsApi.getIdentity).mockResolvedValue(
      baseReport({ agent_token_id: null, identity_registry: null }),
    );

    renderRoute();

    // The h3 heading contains "On-Chain Identity" — use role query to be precise
    await waitFor(() => {
      expect(
        screen.getByRole("heading", { name: /On-Chain Identity/i }),
      ).toBeInTheDocument();
    });

    // The "not configured" span is inside the heading
    expect(screen.getByText(/not configured/i)).toBeInTheDocument();
    // The env var hint paragraph is present
    expect(screen.getByText(/XVN_IDENTITY_REGISTRY/)).toBeInTheDocument();
  });

  it("renders token_id and a Mantlescan link when configured", async () => {
    vi.mocked(settingsApi.getIdentity).mockResolvedValue(
      baseReport({
        agent_token_id: BigInt(42) as unknown as bigint,
        identity_registry: "0x1DE1000000000000000000000000000000004Fe4",
        last_attestation_tx: null,
        mantlescan_base_url: "https://sepolia.mantlescan.xyz",
      }),
    );

    renderRoute();

    // Token ID renders
    await waitFor(() => {
      expect(screen.getByText("42")).toBeInTheDocument();
    });

    // Registry address renders truncated and is a link to Mantlescan
    const registryLink = screen.getByRole("link", { name: /0x1DE1.*4Fe4/i });
    expect(registryLink).toBeInTheDocument();
    expect(registryLink).toHaveAttribute(
      "href",
      "https://sepolia.mantlescan.xyz/address/0x1DE1000000000000000000000000000000004Fe4",
    );

    // Token link points to token page with address + id
    const tokenLink = screen.getByRole("link", { name: "42" });
    expect(tokenLink).toHaveAttribute(
      "href",
      "https://sepolia.mantlescan.xyz/token/0x1DE1000000000000000000000000000000004Fe4?a=42",
    );
  });

  it("renders a Mantlescan tx link when last_attestation_tx is set", async () => {
    const tx =
      "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    vi.mocked(settingsApi.getIdentity).mockResolvedValue(
      baseReport({
        identity_registry: "0x1DE1000000000000000000000000000000004Fe4",
        last_attestation_tx: tx,
      }),
    );

    renderRoute();

    const txLink = await screen.findByRole("link", { name: /0xabcdef/i });
    expect(txLink).toHaveAttribute(
      "href",
      `https://sepolia.mantlescan.xyz/tx/${tx}`,
    );
  });
});
