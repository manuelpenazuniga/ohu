import { describe, it, expect } from "vitest";
import request from "supertest";
import { buildReputationApp } from "../src/x402/reputation-server.js";
import { NETWORK_CASPER_TESTNET, SCHEME_EXACT } from "../src/x402/constants.js";
import { makeMockFacilitator, makeTestConfig, decodeB64Json, DEV_ASSET, DEV_PAYEE } from "./x402-fixtures.js";

describe("Rail B x402 — respuesta 402 y forma de los requisitos", () => {
  it("un GET impago responde 402 con PAYMENT-REQUIRED sobre el token CEP-18 (Rail B)", async () => {
    const cfg = makeTestConfig();
    const app = buildReputationApp(cfg, makeMockFacilitator(cfg));

    const res = await request(app).get("/reputation/acme-farm");

    expect(res.status).toBe(402);
    const header = res.headers["payment-required"];
    expect(header).toBeTruthy();
    const required = decodeB64Json(header as string) as {
      x402Version: number;
      accepts: Array<{
        scheme: string;
        network: string;
        asset: string;
        payTo: string;
        amount: string;
        extra: Record<string, unknown>;
      }>;
    };

    expect(required.x402Version).toBe(2);
    expect(required.accepts.length).toBeGreaterThanOrEqual(1);
    const a = required.accepts[0]!;
    // Riel B genuino: scheme exact sobre Casper Testnet.
    expect(a.scheme).toBe(SCHEME_EXACT);
    expect(a.network).toBe(NETWORK_CASPER_TESTNET);
    // El asset es el token CEP-18 configurado, NO el OhuVault.
    expect(a.asset).toBe(DEV_ASSET);
    expect(a.payTo).toBe(DEV_PAYEE);
    // El dominio EIP-712 del token (name/version) viaja en extra.
    expect(a.extra["name"]).toBe("WCSPR");
    expect(a.extra["version"]).toBe("1");
    expect(a.amount).toMatch(/^[0-9]+$/);
  });

  it("no sirve el recurso mientras no se pague (el viaje 402 no entrega reputación)", async () => {
    const cfg = makeTestConfig();
    const app = buildReputationApp(cfg, makeMockFacilitator(cfg));
    const res = await request(app).get("/reputation/acme-farm");
    expect(res.body).not.toHaveProperty("score");
  });

  it("/health documenta que Rail B NO es settlement de escrow", async () => {
    const cfg = makeTestConfig();
    const app = buildReputationApp(cfg, makeMockFacilitator(cfg));
    const res = await request(app).get("/health");
    expect(res.status).toBe(200);
    expect(res.body.assetIsOhuVault).toBe(false);
    expect(res.body.nonEscrowDeclaration).toContain("NOT_ESCROW_SETTLEMENT");
  });
});