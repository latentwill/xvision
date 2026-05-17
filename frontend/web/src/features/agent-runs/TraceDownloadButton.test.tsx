// frontend/web/src/features/agent-runs/TraceDownloadButton.test.tsx
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  TraceDownloadButton,
  filenameFromContentDisposition,
} from "./TraceDownloadButton";

const RUN_ID = "run_abc1234";
const EXPECTED_URL = `/api/agent-runs/${RUN_ID}/export.json`;

const originalFetch = global.fetch;
const originalCreate = URL.createObjectURL;
const originalRevoke = URL.revokeObjectURL;

beforeEach(() => {
  URL.createObjectURL = vi.fn(() => "blob:mock-url");
  URL.revokeObjectURL = vi.fn();
});

afterEach(() => {
  global.fetch = originalFetch;
  URL.createObjectURL = originalCreate;
  URL.revokeObjectURL = originalRevoke;
  vi.restoreAllMocks();
});

function mockFetchResponse({
  ok = true,
  status = 200,
  statusText = "OK",
  body = '{"run":{},"spans":[],"events":[]}',
  contentDisposition = null as string | null,
}: Partial<{
  ok: boolean;
  status: number;
  statusText: string;
  body: string;
  contentDisposition: string | null;
}>) {
  const fetchMock = vi.fn(
    async (input: RequestInfo | URL, init?: RequestInit) => {
      void input;
      void init;
      const headers = new Headers();
      if (contentDisposition) headers.set("content-disposition", contentDisposition);
      return new Response(ok ? body : "", { status, statusText, headers });
    },
  );
  global.fetch = fetchMock as unknown as typeof fetch;
  return fetchMock;
}

describe("filenameFromContentDisposition", () => {
  test("returns null when header absent", () => {
    expect(filenameFromContentDisposition(null)).toBeNull();
  });

  test("parses quoted filename", () => {
    expect(
      filenameFromContentDisposition('attachment; filename="xvn_run_abc.json"'),
    ).toBe("xvn_run_abc.json");
  });

  test("parses unquoted filename", () => {
    expect(
      filenameFromContentDisposition("attachment; filename=xvn_run_xyz.json"),
    ).toBe("xvn_run_xyz.json");
  });

  test("falls back to filename* when no plain filename", () => {
    expect(
      filenameFromContentDisposition(
        "attachment; filename*=UTF-8''xvn_run_%E2%9C%93.json",
      ),
    ).toBe("xvn_run_✓.json");
  });
});

describe("TraceDownloadButton", () => {
  test("renders a labelled button", () => {
    mockFetchResponse({});
    render(<TraceDownloadButton runId={RUN_ID} />);
    expect(screen.getByTestId("trace-download-button")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /download trace json/i }),
    ).toBeEnabled();
  });

  test("on click, fetches the export URL with credentials and triggers a download", async () => {
    const fetchMock = mockFetchResponse({
      contentDisposition: 'attachment; filename="server_supplied.json"',
    });

    render(<TraceDownloadButton runId={RUN_ID} />);
    await userEvent.click(screen.getByTestId("trace-download-button"));

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    const [calledUrl, calledInit] = fetchMock.mock.calls[0];
    expect(calledUrl).toBe(EXPECTED_URL);
    expect(calledInit).toMatchObject({ credentials: "include" });
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:mock-url");
  });

  test("uses default filename when Content-Disposition is missing", async () => {
    mockFetchResponse({ contentDisposition: null });
    const appendSpy = vi.spyOn(document.body, "appendChild");

    render(<TraceDownloadButton runId={RUN_ID} />);
    await userEvent.click(screen.getByTestId("trace-download-button"));

    await waitFor(() => expect(URL.createObjectURL).toHaveBeenCalled());
    const anchor = appendSpy.mock.calls
      .map((c) => c[0] as HTMLElement)
      .find((node): node is HTMLAnchorElement => node instanceof HTMLAnchorElement);
    expect(anchor).toBeDefined();
    expect(anchor?.getAttribute("download")).toBe(`xvn_run_${RUN_ID}.json`);
  });

  test("uses server-supplied filename when present", async () => {
    mockFetchResponse({
      contentDisposition: 'attachment; filename="xvn_run_custom.json"',
    });
    const appendSpy = vi.spyOn(document.body, "appendChild");

    render(<TraceDownloadButton runId={RUN_ID} />);
    await userEvent.click(screen.getByTestId("trace-download-button"));

    await waitFor(() => expect(URL.createObjectURL).toHaveBeenCalled());
    const anchor = appendSpy.mock.calls
      .map((c) => c[0] as HTMLElement)
      .find((node): node is HTMLAnchorElement => node instanceof HTMLAnchorElement);
    expect(anchor?.getAttribute("download")).toBe("xvn_run_custom.json");
  });

  test("on non-2xx response, warns via console and does not create a blob URL", async () => {
    mockFetchResponse({ ok: false, status: 500, statusText: "Internal Server Error" });
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    render(<TraceDownloadButton runId={RUN_ID} />);
    await userEvent.click(screen.getByTestId("trace-download-button"));

    await waitFor(() => expect(warnSpy).toHaveBeenCalled());
    expect(URL.createObjectURL).not.toHaveBeenCalled();
    expect(warnSpy.mock.calls[0][0]).toContain("trace-download failed");
  });

  test("on fetch rejection, warns and remains clickable", async () => {
    const fetchMock = vi.fn(async () => {
      throw new Error("network down");
    });
    global.fetch = fetchMock as unknown as typeof fetch;
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    render(<TraceDownloadButton runId={RUN_ID} />);
    await userEvent.click(screen.getByTestId("trace-download-button"));

    await waitFor(() => expect(warnSpy).toHaveBeenCalled());
    expect(URL.createObjectURL).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByTestId("trace-download-button")).toBeEnabled(),
    );
  });

  test("encodes run id with reserved characters in the URL", async () => {
    const weird = "run/with spaces";
    const fetchMock = mockFetchResponse({});

    render(<TraceDownloadButton runId={weird} />);
    await userEvent.click(screen.getByTestId("trace-download-button"));

    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    const [calledUrl] = fetchMock.mock.calls[0];
    expect(calledUrl).toBe(
      `/api/agent-runs/${encodeURIComponent(weird)}/export.json`,
    );
  });
});
