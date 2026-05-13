import { afterEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ChatRail } from "./ChatRail";

const defaultStorage = globalThis.localStorage;

function renderRail() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter>
      <QueryClientProvider client={client}>
        <ChatRail />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("ChatRail", () => {
  afterEach(() => {
    Object.defineProperty(globalThis, "localStorage", {
      value: defaultStorage,
      writable: true,
      configurable: true,
    });
    Object.defineProperty(window, "localStorage", {
      value: defaultStorage,
      writable: true,
      configurable: true,
    });
  });

  it("does not block app startup when Safari storage access is unavailable", () => {
    const blockedStorage = {
      getItem() {
        throw new DOMException("Blocked", "SecurityError");
      },
      setItem() {
        throw new DOMException("Blocked", "SecurityError");
      },
      removeItem() {
        throw new DOMException("Blocked", "SecurityError");
      },
    };
    Object.defineProperty(globalThis, "localStorage", {
      value: blockedStorage,
      writable: true,
      configurable: true,
    });
    Object.defineProperty(window, "localStorage", {
      value: blockedStorage,
      writable: true,
      configurable: true,
    });

    renderRail();

    expect(screen.getByLabelText("Chat rail")).toBeInTheDocument();
  });
});
