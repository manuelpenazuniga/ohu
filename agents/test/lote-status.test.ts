import { describe, it, expect, vi, beforeEach } from "vitest";
import { loteStatuses, openLotes } from "../src/x402/lote-status.js";

const API = "https://api.testnet.cspr.cloud";

function stub(responses: Record<string, unknown>): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => ({ ok: true, json: async () => responses[url.replace(API, "")] })),
  );
}

const RESPONSES = {
  "/contract-packages/PKG/contracts?page_size=1": { data: [{ contract_hash: "CH" }], page_count: 1 },
  "/contracts/CH/entry-points?page_size=100": {
    data: [
      { id: 1, name: "open_lote" },
      { id: 2, name: "lock_lote" },
      { id: 3, name: "evaluate_lote" },
      { id: 4, name: "release_to_producer" },
    ],
    page_count: 1,
  },
  "/deploys?contract_package_hash=PKG&page_size=50&page=1": {
    data: [
      { entry_point_id: 1, args: { lote_id: { parsed: 7 }, producer: { parsed: "account-hash-P7" } }, block_height: 1 },
      { entry_point_id: 2, args: { lote_id: { parsed: 7 } }, block_height: 2 },
      { entry_point_id: 1, args: { lote_id: { parsed: 8 }, producer: { parsed: "account-hash-Q8" } }, block_height: 3 },
      { entry_point_id: 3, args: { lote_id: { parsed: 7 } }, block_height: 4 },
      { entry_point_id: 4, args: { lote_id: { parsed: 7 } }, block_height: 5 },
      // lote 9 abierto pero su lock revirtió → sigue OPEN
      { entry_point_id: 1, args: { lote_id: { parsed: 9 }, producer: { parsed: "account-hash-R9" } }, block_height: 6 },
      { entry_point_id: 2, args: { lote_id: { parsed: 9 } }, error_message: "User error: 63", block_height: 7 },
    ],
    page_count: 1,
  },
};

const cfg = { apiUrl: API, apiKey: "k", packageHash: "PKG" };

describe("loteStatuses (derivación de estado on-chain)", () => {
  beforeEach(() => stub(RESPONSES));

  it("toma la transición más avanzada de cada lote", async () => {
    const all = await loteStatuses(cfg);
    expect(all.get(7)).toMatchObject({ state: "SETTLED_OK", producer: "P7" });
    expect(all.get(8)).toMatchObject({ state: "OPEN", producer: "Q8" });
    expect(all.get(9)).toMatchObject({ state: "OPEN" }); // el lock revirtió
  });

  it("openLotes lista solo los OPEN, ordenados", async () => {
    const open = await openLotes(cfg);
    expect(open.map((l) => l.loteId)).toEqual([8, 9]);
  });
});
