// src/features/marketplace/routes/InstallSteps.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/client";
import { RECEIPTS } from "@/features/marketplace/data/fixtures/receipts";
import { InstallSteps } from "./InstallSteps";

vi.mock("../lib/chain", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/chain")>();
  return { ...actual, currentAddress: vi.fn() };
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
import { currentAddress } from "../lib/chain";
import {
  fetchBundle,
  importSealedListing,
} from "@/features/marketplace/data/ApiMarketplaceData";

const mockedAddress = vi.mocked(currentAddress);
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
  return render(<MemoryRouter>{ui}</MemoryRouter>);
}

beforeEach(() => {
  vi.clearAllMocks();
  bundleOpen();
});

describe("InstallSteps", () => {
  it("renders all step titles when the receipt carries a bundle CID", () => {
    wrap(<InstallSteps receipt={receipt} />);
    expect(screen.getByText(/XVN install detected/i)).toBeInTheDocument();
    expect(screen.getByText(/Fetch strategy bundle/i)).toBeInTheDocument();
    expect(screen.getByText(/Install missing ingredients/i)).toBeInTheDocument();
    expect(screen.getByText(/Add to your Strategies/i)).toBeInTheDocument();
  });

  it("step 1 renders as done (struck-through) when xvnDetected is true", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const step1title = screen.getByText(/XVN install detected/i);
    // done steps get line-through decoration
    expect(step1title.className).toMatch(/line-through/);
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

  it("shows xvnEndpoint in step 1 description when detected", () => {
    wrap(<InstallSteps receipt={receipt} />);
    expect(screen.getByText(/localhost:3000/)).toBeInTheDocument();
  });

  it("shows 'not detected' message in step 1 when xvnDetected is false", () => {
    const noXvn = {
      ...receipt,
      install: { ...receipt.install, xvnDetected: false },
    };
    wrap(<InstallSteps receipt={noXvn} />);
    expect(screen.getByText(/not detected/i)).toBeInTheDocument();
  });

  it("step 3 action chip shows count of missing ingredients", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const missingCount = receipt.install.ingredients.filter((i) => !i.installed).length;
    expect(screen.getByText(new RegExp(`Install missing \\(${missingCount}\\)`))).toBeInTheDocument();
  });

  // ── bundle step (IPFS open-tier) ──────────────────────────────────────────
  describe("bundle step", () => {
    it("links 'Open bundle' to the IPFS gateway for the receipt's CID", () => {
      wrap(<InstallSteps receipt={receipt} />);
      const link = screen.getByRole("link", { name: /open bundle/i });
      expect(link).toHaveAttribute(
        "href",
        `https://gateway.pinata.cloud/ipfs/${receipt.license.bundleCid}`,
      );
      // never offer a fake decrypt action
      expect(screen.queryByText(/decrypt/i)).not.toBeInTheDocument();
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
});
