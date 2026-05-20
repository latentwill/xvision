import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { CacheStatusBadge } from "./CacheStatusBadge";

describe("CacheStatusBadge", () => {
  it("renders fully cached status without a fetch button", () => {
    render(
      <CacheStatusBadge
        status={{ type: "FullyCached", bar_count: 12, fetched_at: "now" }}
      />,
    );

    expect(screen.getByText("Fully cached: 12 bars")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("renders partial and uncached fetch buttons", () => {
    const onFetch = vi.fn();
    const { rerender } = render(
      <CacheStatusBadge
        status={{ type: "PartiallyCached", fetched_count: 3, expected_count: 8 }}
        onFetch={onFetch}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Fetch bars" }));
    expect(onFetch).toHaveBeenCalledTimes(1);
    expect(screen.getByText("partial: 3/8")).toBeInTheDocument();

    rerender(
      <CacheStatusBadge
        status={{ type: "NotCached", expected_count: 8 }}
        onFetch={onFetch}
        fetchStatus="Fetching…"
        disabled
      />,
    );
    expect(screen.getByRole("button", { name: "Fetching…" })).toBeDisabled();
  });

  it("renders fetch status text when no fetch handler is supplied", () => {
    render(
      <CacheStatusBadge
        status={{ type: "NotCached", expected_count: 8 }}
        fetchStatus="queued"
      />,
    );

    expect(screen.getByText("queued")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
