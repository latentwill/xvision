// src/features/marketplace/routes/InstallSteps.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/client";
import { RECEIPTS } from "@/features/marketplace/data/fixtures/receipts";
import { InstallSteps } from "./InstallSteps";

vi.mock("../lib/chain", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/chain")>();
  return { ...actual, currentAddress: vi.fn(), getPublicGateway: vi.fn() };
});
vi.mock("@/api/client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/client")>();
  return { ...actual, apiFetch: vi.fn() };
});
// The sealed-tier path pulls in fetchBundle + importSealedListing from the data
// module; mock them at the boundary so the stepper doesn't touch the chain/Lit.
vi.mock("@/features/marketplace/data/ApiMarketplaceData", () => ({
  fetchBundle: vi.fn(),
  importSealedListing: vi.fn(),
}));

import { apiFetch } from "@/api/client";
import { currentAddress, getPublicGateway } from "../lib/chain";
import {
  fetchBundle,
  importSealedListing,
} from "@/features/marketplace/data/ApiMarketplaceData";

const mockedAddress = vi.mocked(currentAddress);
const mockedGateway = vi.mocked(getPublicGateway);
const mockedApiFetch = vi.mocked(apiFetch);
const mockedFetchBundle = vi.mocked(fetchBundle);
const mockedImportSealed = vi.mocked(importSealedListing);

const receipt = RECEIPTS["0xdemo-tx"];

// Default: every test treats the receipt as OPEN tier unless it overrides the
// bundle fetch. Resolved (not rejected) so the open stepper renders normally.
function bundleOpen() {
  mockedFetchBundle.mockResolvedValue({
    listing_id: Number(receipt.listing.id) || 0,
    content_uri: "",
    encrypted: false,
  });
}
function bundleSealed() {
  mockedFetchBundle.mockResolvedValue({
    listing_id: Number(receipt.listing.id) || 0,
    content_uri: "",
    encrypted: true,
    ciphertext: "CIPHER",
    content_hash: "ab".repeat(32),
  });
}

function wrap(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  bundleOpen();
  // Default: the status route surfaces a configured self-hosted gateway.
  mockedGateway.mockResolvedValue("https://ipfs.mynode.example");
});

describe("InstallSteps", () => {
  it("renders all step titles when the receipt carries a bundle CID", () => {
    wrap(<InstallSteps receipt={receipt} />);
    expect(screen.getByText(/Fetch strategy bundle/i)).toBeInTheDocument();
    expect(screen.getByText(/Install missing ingredients/i)).toBeInTheDocument();
    expect(screen.getByText(/Add to your Strategies/i)).toBeInTheDocument();
  });

  it("does not render the 'XVN install detected' step (QA #10)", () => {
    wrap(<InstallSteps receipt={receipt} />);
    expect(screen.queryByText(/XVN install detected/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/XVN not detected/i)).not.toBeInTheDocument();
  });

  it("step 3 renders ingredient chips for all ingredients", () => {
    wrap(<InstallSteps receipt={receipt} />);
    for (const ing of receipt.install.ingredients) {
      expect(screen.getByText(ing.name)).toBeInTheDocument();
    }
  });

  it("installed ingredients show a different tone to missing ones", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const installed = receipt.install.ingredients.filter((i) => i.installed);
    const missing   = receipt.install.ingredients.filter((i) => !i.installed);
    // Installed chips carry data-installed="true" for test accessibility
    expect(
      screen.getAllByTestId("ingredient-chip").filter(
        (el) => el.getAttribute("data-installed") === "true"
      )
    ).toHaveLength(installed.length);
    expect(
      screen.getAllByTestId("ingredient-chip").filter(
        (el) => el.getAttribute("data-installed") === "false"
      )
    ).toHaveLength(missing.length);
  });

  it("step 3 action chip shows count of missing ingredients", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const missingCount = receipt.install.ingredients.filter((i) => !i.installed).length;
    expect(screen.getByText(new RegExp(`Install missing \\(${missingCount}\\)`))).toBeInTheDocument();
  });

  // ── bundle step (IPFS open-tier) ──────────────────────────────────────────
  describe("bundle step", () => {
    it("links 'Open bundle' to the config-driven public gateway for the receipt's CID", async () => {
      wrap(<InstallSteps receipt={receipt} />);
      const link = await screen.findByRole("link", { name: /open bundle/i });
      // The href is config-driven from the status route's public_gateway.
      await vi.waitFor(() =>
        expect(link).toHaveAttribute(
          "href",
          `https://ipfs.mynode.example/ipfs/${receipt.license.bundleCid}`,
        ),
      );
      // No vendor gateway is baked in.
      expect(link.getAttribute("href")).not.toContain("pinata");
      // never offer a fake decrypt action
      expect(screen.queryByText(/decrypt/i)).not.toBeInTheDocument();
    });

    it("falls back to the vendor-neutral default gateway when status lacks one", async () => {
      mockedGateway.mockResolvedValue("https://dweb.link");
      wrap(<InstallSteps receipt={receipt} />);
      const link = await screen.findByRole("link", { name: /open bundle/i });
      await vi.waitFor(() =>
        expect(link).toHaveAttribute(
          "href",
          `https://dweb.link/ipfs/${receipt.license.bundleCid}`,
        ),
      );
    });

    it("is hidden entirely when the receipt has no bundle CID", () => {
      const noCid = {
        ...receipt,
        license: { ...receipt.license, bundleCid: "" },
      };
      wrap(<InstallSteps receipt={noCid} />);
      expect(screen.queryByText(/Fetch strategy bundle/i)).not.toBeInTheDocument();
      expect(screen.queryByRole("link", { name: /open bundle/i })).not.toBeInTheDocument();
      // remaining steps still render
      expect(screen.getByText(/Add to your Strategies/i)).toBeInTheDocument();
    });
  });

  // ── import flow (Add to strategies) ───────────────────────────────────────
  describe("add to strategies", () => {
    it("shows an inline wallet error when no wallet is connected", async () => {
      mockedAddress.mockResolvedValue(null);
      wrap(<InstallSteps receipt={receipt} />);

      await userEvent.click(screen.getByRole("button", { name: /add to strategies/i }));

      expect(await screen.findByTestId("import-error")).toHaveTextContent(
        /connect wallet first/i,
      );
      expect(mockedApiFetch).not.toHaveBeenCalled();
    });

    it("POSTs the import and replaces the button with an 'Open in strategies' link", async () => {
      mockedAddress.mockResolvedValue("0x7c2e000000000000000000000000000000000007");
      mockedApiFetch.mockResolvedValue({ agent_id: "01HNEWULID" });
      wrap(<InstallSteps receipt={receipt} />);

      await userEvent.click(screen.getByRole("button", { name: /add to strategies/i }));

      const link = await screen.findByRole("link", { name: /open in strategies/i });
      expect(link).toHaveAttribute("href", "/authoring/01HNEWULID");
      expect(screen.queryByRole("button", { name: /add to strategies/i })).not.toBeInTheDocument();
      expect(mockedApiFetch).toHaveBeenCalledWith(
        `/api/marketplace/listings/${receipt.listing.id}/import`,
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({ address: "0x7c2e000000000000000000000000000000000007" }),
        }),
      );
    });

    it("shows a pending state while the import is in flight", async () => {
      mockedAddress.mockResolvedValue("0x7c2e000000000000000000000000000000000007");
      mockedApiFetch.mockReturnValue(new Promise(() => {})); // never resolves
      wrap(<InstallSteps receipt={receipt} />);

      await userEvent.click(screen.getByRole("button", { name: /add to strategies/i }));

      expect(await screen.findByText(/importing/i)).toBeInTheDocument();
    });

    it("maps a 403 to the no-license inline error", async () => {
      mockedAddress.mockResolvedValue("0x7c2e000000000000000000000000000000000007");
      mockedApiFetch.mockRejectedValue(
        new ApiError(403, "forbidden", "no license for 0x7c2e… on listing 3"),
      );
      wrap(<InstallSteps receipt={receipt} />);

      await userEvent.click(screen.getByRole("button", { name: /add to strategies/i }));

      expect(await screen.findByTestId("import-error")).toHaveTextContent(
        /no license held for this wallet/i,
      );
      // button stays so the user can retry with another wallet
      expect(screen.getByRole("button", { name: /add to strategies/i })).toBeInTheDocument();
    });

    it("surfaces other API errors verbatim", async () => {
      mockedAddress.mockResolvedValue("0x7c2e000000000000000000000000000000000007");
      mockedApiFetch.mockRejectedValue(
        new ApiError(503, "service_unavailable", "license gating not configured"),
      );
      wrap(<InstallSteps receipt={receipt} />);

      await userEvent.click(screen.getByRole("button", { name: /add to strategies/i }));

      expect(await screen.findByTestId("import-error")).toHaveTextContent(
        /license gating not configured/i,
      );
    });
  });

  // ── sealed-tier decrypt + import ──────────────────────────────────────────
  describe("sealed tier", () => {
    it("revives a 'Decrypt & import sealed bundle' step for encrypted bundles", async () => {
      bundleSealed();
      wrap(<InstallSteps receipt={receipt} />);
      expect(await screen.findByText(/Decrypt & import sealed bundle/i)).toBeInTheDocument();
      // the open-tier 'Add to strategies' title is replaced
      expect(screen.queryByText(/Add to your Strategies/i)).not.toBeInTheDocument();
      // and the open IPFS 'Open bundle' link is not offered (ciphertext only)
      expect(screen.queryByRole("link", { name: /open bundle/i })).not.toBeInTheDocument();
    });

    it("decrypts + imports and swaps in an 'Open in strategies' link", async () => {
      bundleSealed();
      mockedImportSealed.mockResolvedValue({ agent_id: "01HSEALEDULID" });
      wrap(<InstallSteps receipt={receipt} />);

      const btn = await screen.findByRole("button", { name: /decrypt & import/i });
      await userEvent.click(btn);

      const link = await screen.findByRole("link", { name: /open in strategies/i });
      expect(link).toHaveAttribute("href", "/authoring/01HSEALEDULID");
      expect(mockedImportSealed).toHaveBeenCalledWith(receipt.listing.id);
    });

    it("shows a 'Decrypting…' pending state while in flight", async () => {
      bundleSealed();
      mockedImportSealed.mockReturnValue(new Promise(() => {}));
      wrap(<InstallSteps receipt={receipt} />);

      await userEvent.click(await screen.findByRole("button", { name: /decrypt & import/i }));
      expect(await screen.findByText(/decrypting/i)).toBeInTheDocument();
    });

    it("maps a 403 to a no-license inline error and keeps the button", async () => {
      bundleSealed();
      mockedImportSealed.mockRejectedValue(new ApiError(403, "forbidden", "no license"));
      wrap(<InstallSteps receipt={receipt} />);

      await userEvent.click(await screen.findByRole("button", { name: /decrypt & import/i }));
      expect(await screen.findByTestId("import-error")).toHaveTextContent(
        /no license held for this wallet/i,
      );
      expect(screen.getByRole("button", { name: /decrypt & import/i })).toBeInTheDocument();
    });

    it("maps a 409 to a content-hash mismatch inline error", async () => {
      bundleSealed();
      mockedImportSealed.mockRejectedValue(new ApiError(409, "conflict", "hash mismatch"));
      wrap(<InstallSteps receipt={receipt} />);

      await userEvent.click(await screen.findByRole("button", { name: /decrypt & import/i }));
      expect(await screen.findByTestId("import-error")).toHaveTextContent(
        /content hash mismatch/i,
      );
    });

    it("falls back to the open-tier stepper when the bundle route fails", async () => {
      mockedFetchBundle.mockRejectedValue(new Error("unreachable"));
      wrap(<InstallSteps receipt={receipt} />);
      // open-tier 'Add to strategies' step is present; no sealed step
      expect(await screen.findByText(/Add to your Strategies/i)).toBeInTheDocument();
      expect(screen.queryByText(/Decrypt & import sealed bundle/i)).not.toBeInTheDocument();
    });
  });

  // ── receipt requirements from the verified bundle (real receipts) ─────────
  describe("bundle-derived requirements", () => {
    const realReceipt = {
      ...receipt,
      listing: { ...receipt.listing, id: "3" },
      install: { xvnDetected: false, xvnEndpoint: "", ingredients: [] },
    };
    const bundleOut = {
      listing_id: 3,
      content_uri: "ipfs://bafytestcid",
      verified: true,
      manifest: {
        manifest: {
          display_name: "BTC Momentum",
          plain_summary: "Buys dips.",
          creator: "@ed",
          attested_with: ["claude-haiku-4.5"],
          required_tools: ["birdeye-mcp"],
        },
      },
    };

    it("fetches the bundle for a numeric listing id and renders neutral required chips", async () => {
      mockedApiFetch.mockResolvedValue(bundleOut);
      wrap(<InstallSteps receipt={realReceipt} />);

      expect(await screen.findByTestId("receipt-requirements")).toBeInTheDocument();
      expect(mockedApiFetch).toHaveBeenCalledWith("/api/marketplace/listings/3/bundle");

      // attested models + required tools render as requirement chips
      expect(screen.getByText("claude-haiku-4.5")).toBeInTheDocument();
      expect(screen.getByText("birdeye-mcp")).toBeInTheDocument();
      const chips = screen.getAllByTestId("requirement-chip");
      expect(chips).toHaveLength(2);

      // honest: no installed/missing claims
      expect(screen.getByText(/installed state unknown/i)).toBeInTheDocument();
      expect(screen.queryByTestId("ingredient-chip")).not.toBeInTheDocument();
      expect(screen.queryByText(/install missing/i)).not.toBeInTheDocument();
    });

    it("does not fetch the bundle for fixture (slug) receipts", async () => {
      wrap(<InstallSteps receipt={receipt} />);
      expect(screen.queryByTestId("receipt-requirements")).not.toBeInTheDocument();
      expect(mockedApiFetch).not.toHaveBeenCalledWith(
        expect.stringMatching(/\/bundle$/),
      );
    });

    it("renders the plain ingredients step when the bundle fetch fails", async () => {
      mockedApiFetch.mockRejectedValue(
        new ApiError(404, "not_found", "listing 3 not in indexed snapshot"),
      );
      wrap(<InstallSteps receipt={realReceipt} />);
      // falls back to the (empty) local ingredient step — nothing invented
      expect(screen.getByText(/Install missing ingredients/i)).toBeInTheDocument();
      expect(screen.queryByTestId("requirement-chip")).not.toBeInTheDocument();
    });
  });
});
