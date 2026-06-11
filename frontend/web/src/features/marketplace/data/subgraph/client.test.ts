// src/features/marketplace/data/subgraph/client.test.ts
import { describe, expect, it, vi } from "vitest";
import { createSubgraphClient, SubgraphError } from "./client";

type FakeRes = {
  ok: boolean;
  status: number;
  json: () => Promise<unknown>;
};
const res = (status: number, body: unknown): FakeRes => ({
  ok: status >= 200 && status < 300,
  status,
  json: async () => body,
});

const URL = "https://example.test/subgraph";

describe("createSubgraphClient", () => {
  it("POSTs query + variables and returns data", async () => {
    const fetchImpl = vi.fn(async () =>
      res(200, { data: { listings: [{ id: "1" }] } }),
    ) as unknown as typeof fetch;
    const client = createSubgraphClient(URL, fetchImpl);

    const data = await client.query<{ listings: { id: string }[] }>(
      "query Q($first: Int!) { listings(first: $first) { id } }",
      { first: 10 },
    );

    expect(data.listings[0].id).toBe("1");
    const call = (fetchImpl as unknown as ReturnType<typeof vi.fn>).mock
      .calls[0];
    expect(call[0]).toBe(URL);
    expect(call[1].method).toBe("POST");
    const sent = JSON.parse(call[1].body as string);
    expect(sent.variables).toEqual({ first: 10 });
    expect(sent.query).toContain("listings");
  });

  it("throws SubgraphError on non-2xx", async () => {
    const fetchImpl = vi.fn(async () => res(502, {})) as unknown as typeof fetch;
    const client = createSubgraphClient(URL, fetchImpl);
    await expect(client.query("{ x }")).rejects.toThrow(SubgraphError);
    await expect(client.query("{ x }")).rejects.toThrow(/HTTP 502/);
  });

  it("throws on a GraphQL errors payload", async () => {
    const fetchImpl = vi.fn(async () =>
      res(200, { errors: [{ message: "bad field" }] }),
    ) as unknown as typeof fetch;
    const client = createSubgraphClient(URL, fetchImpl);
    await expect(client.query("{ x }")).rejects.toThrow(/bad field/);
  });

  it("throws when data is absent", async () => {
    const fetchImpl = vi.fn(async () => res(200, {})) as unknown as typeof fetch;
    const client = createSubgraphClient(URL, fetchImpl);
    await expect(client.query("{ x }")).rejects.toThrow(/no data/);
  });

  it("wraps a network failure", async () => {
    const fetchImpl = vi.fn(async () => {
      throw new Error("ECONNREFUSED");
    }) as unknown as typeof fetch;
    const client = createSubgraphClient(URL, fetchImpl);
    await expect(client.query("{ x }")).rejects.toThrow(/request failed/);
  });
});
