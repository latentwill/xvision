import { describe, expect, it, afterEach } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import {
  RunSummaryError,
  parseRepeatedBrokerError,
  repeatedBrokerErrorHeadline,
} from "./RunSummary";

afterEach(cleanup);

describe("parseRepeatedBrokerError", () => {
  it("returns null when the prefix is missing — preserves the no-classified path", () => {
    expect(parseRepeatedBrokerError("[broker_auth] not permitted")).toBeNull();
    expect(
      parseRepeatedBrokerError("legacy unprefixed failure"),
    ).toBeNull();
  });

  it("parses the count + error_class from the executor's abort body", () => {
    const raw =
      "[repeated_broker_error] repeated_broker_error: aborted after 3 consecutive broker_min_order_size rejections; run_id=R decision_index=2 asset=BTC/USD last_error=cost basis must be >= minimal amount of order 10";
    expect(parseRepeatedBrokerError(raw)).toEqual({
      count: 3,
      errorClass: "broker_min_order_size",
    });
  });
});

describe("repeatedBrokerErrorHeadline", () => {
  it("formats the classified one-liner with count + class", () => {
    expect(repeatedBrokerErrorHeadline(3, "broker_min_order_size")).toBe(
      "Aborted after 3 consecutive broker_min_order_size rejections",
    );
  });

  it("falls back to a generic phrasing when parsing failed", () => {
    expect(repeatedBrokerErrorHeadline(null, null)).toMatch(
      /eval circuit breaker/i,
    );
  });
});

describe("RunSummaryError", () => {
  it("renders nothing when there's no error — null and empty-string both no-op", () => {
    const { container } = render(<RunSummaryError error={null} />);
    expect(container.firstChild).toBeNull();
    cleanup();
    const { container: empty } = render(<RunSummaryError error="" />);
    expect(empty.firstChild).toBeNull();
  });

  it("renders the raw error verbatim for unclassified failures (legacy path unchanged)", () => {
    render(<RunSummaryError error="legacy failure without class prefix" />);
    expect(
      screen.getByText("legacy failure without class prefix"),
    ).toBeTruthy();
    expect(
      screen.queryByTestId("run-summary-circuit-breaker-banner"),
    ).toBeNull();
  });

  it("renders the raw error for a non-circuit-breaker classified failure (e.g. broker_auth)", () => {
    // Regression: classified failures that aren't circuit-breaker
    // aborts must NOT trigger the banner. The original red error
    // block keeps its existing look.
    render(
      <RunSummaryError error="[broker_auth] paper eval submit_order failed: forbidden" />,
    );
    expect(
      screen.queryByTestId("run-summary-circuit-breaker-banner"),
    ).toBeNull();
    expect(
      screen.getByText(
        "[broker_auth] paper eval submit_order failed: forbidden",
      ),
    ).toBeTruthy();
  });

  it("surfaces a classified banner for repeated_broker_error aborts", () => {
    const raw =
      "[repeated_broker_error] repeated_broker_error: aborted after 3 consecutive broker_min_order_size rejections; run_id=R decision_index=2 asset=BTC/USD last_error=cost basis must be >= minimal amount of order 10";
    render(<RunSummaryError error={raw} />);
    const banner = screen.getByTestId("run-summary-circuit-breaker-banner");
    expect(banner.textContent).toContain(
      "Aborted after 3 consecutive broker_min_order_size rejections",
    );
    // Raw error still rendered below — operator can copy the full
    // chain for an incident postmortem.
    expect(screen.getByText(raw)).toBeTruthy();
  });
});
