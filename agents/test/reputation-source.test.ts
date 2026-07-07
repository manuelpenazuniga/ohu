import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  scoreFor,
  normalizeProducer,
  loadCsprCloudConfig,
  reputationHistory,
  _resetCache,
  type ProducerHistory,
} from "../src/x402/reputation-source.js";

const h = (ok: number, fail: number, lotes = ok + fail): ProducerHistory => ({
  lotesAwarded: lotes,
  settledOk: ok,
  settledFail: fail,
  asOfBlock: 1,
});

describe("scoreFor", () => {
  it("neutral (50) sin liquidaciones", () => {
    expect(scoreFor(h(0, 0, 1))).toBe(50);
  });
  it("2 ok / 1 fail = 73 (validado contra el historial on-chain real)", () => {
    expect(scoreFor(h(2, 1))).toBe(73);
  });
  it("todo ok = 100 (acotado)", () => {
    expect(scoreFor(h(3, 0))).toBe(100);
  });
  it("todo fail = 20", () => {
    expect(scoreFor(h(0, 2))).toBe(20);
  });
});

describe("normalizeProducer", () => {
  it("quita el prefijo account-hash- y baja a minúsculas", () => {
    expect(normalizeProducer("account-hash-AbC")).toBe("abc");
    expect(normalizeProducer("  DEF  ")).toBe("def");
  });
});

describe("loadCsprCloudConfig", () => {
  it("undefined sin API key", () => {
    expect(loadCsprCloudConfig({ OHUVAULT_PACKAGE_HASH: "hash-abc" })).toBeUndefined();
  });
  it("undefined sin package hash", () => {
    expect(loadCsprCloudConfig({ CSPRCLOUD_API_KEY: "k" })).toBeUndefined();
  });
  it("construye config y normaliza el prefijo hash-", () => {
    expect(
      loadCsprCloudConfig({ CSPRCLOUD_API_KEY: "k", OHUVAULT_PACKAGE_HASH: "hash-ABC" }),
    ).toEqual({ apiUrl: "https://api.testnet.cspr.cloud", apiKey: "k", packageHash: "ABC" });
  });
});

describe("reputationHistory (fetch mockeado, sin red)", () => {
  beforeEach(() => _resetCache());

  it("deriva el productor del historial: open_lote→producer, release=OK, settle=FAIL", async () => {
    const API = "https://api.testnet.cspr.cloud";
    const responses: Record<string, unknown> = {
      "/contract-packages/PKG/contracts?page_size=1": { data: [{ contract_hash: "CH" }], page_count: 1 },
      "/contracts/CH/entry-points?page_size=100": {
        data: [
          { id: 1, name: "open_lote" },
          { id: 2, name: "release_to_producer" },
          { id: 3, name: "settle_failure" },
        ],
        page_count: 1,
      },
      "/deploys?contract_package_hash=PKG&page_size=50&page=1": {
        data: [
          { entry_point_id: 1, args: { lote_id: { parsed: 1 }, producer: { parsed: "account-hash-PROD" } }, block_height: 10 },
          { entry_point_id: 1, args: { lote_id: { parsed: 2 }, producer: { parsed: "account-hash-PROD" } }, block_height: 11 },
          { entry_point_id: 3, args: { lote_id: { parsed: 1 } }, block_height: 12 },
          { entry_point_id: 2, args: { lote_id: { parsed: 2 } }, block_height: 13 },
        ],
        page_count: 1,
      },
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => ({
        ok: true,
        json: async () => responses[url.replace(API, "")],
      })),
    );

    const cfg = { apiUrl: API, apiKey: "k", packageHash: "PKG" };
    const hist = await reputationHistory(cfg, 1000);
    const rec = hist.get("prod");
    expect(rec).toEqual({ lotesAwarded: 2, settledOk: 1, settledFail: 1, asOfBlock: 13 });
    expect(scoreFor(rec!)).toBe(60);
  });

  it("ignora deploys con error_message", async () => {
    const API = "https://api.testnet.cspr.cloud";
    const responses: Record<string, unknown> = {
      "/contract-packages/PKG/contracts?page_size=1": { data: [{ contract_hash: "CH" }], page_count: 1 },
      "/contracts/CH/entry-points?page_size=100": { data: [{ id: 2, name: "release_to_producer" }, { id: 1, name: "open_lote" }], page_count: 1 },
      "/deploys?contract_package_hash=PKG&page_size=50&page=1": {
        data: [
          { entry_point_id: 1, args: { lote_id: { parsed: 5 }, producer: { parsed: "account-hash-X" } }, block_height: 1 },
          { entry_point_id: 2, args: { lote_id: { parsed: 5 } }, error_message: "Out of gas error", block_height: 2 },
        ],
        page_count: 1,
      },
    };
    vi.stubGlobal("fetch", vi.fn(async (url: string) => ({ ok: true, json: async () => responses[url.replace(API, "")] })));
    const hist = await reputationHistory({ apiUrl: API, apiKey: "k", packageHash: "PKG" }, 2000);
    // el release falló (out of gas) → no cuenta como OK
    expect(hist.get("x")).toEqual({ lotesAwarded: 1, settledOk: 0, settledFail: 0, asOfBlock: 2 });
  });
});
