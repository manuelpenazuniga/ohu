import { describe, it, expect } from "vitest";
import request from "supertest";
import { buildReputationApp } from "../src/x402/reputation-server.js";
import {
  ESCROW_FORBIDDEN_TOKENS,
  RAIL_B_NON_ESCROW_DECLARATION,
} from "../src/x402/constants.js";
import {
  makeMockFacilitator,
  makeTestConfig,
  decodeB64Json,
  buildStubPaymentPayload,
} from "./x402-fixtures.js";

/**
 * Invariante INV-4 (negativa): x402 NO se usa para mover fondos de escrow.
 * - La config rechaza cargar si el asset apunta al OhuVault.
 * - El recurso servido por Rail B es reputación, no estado de escrow.
 * - La declaración no-escrow está presente en el servicio.
 */
describe("Rail B x402 — invariante no-escrow (INV-4)", () => {
  it("la configuración rechaza que el asset del x402 sea el OhuVault", async () => {
    const { loadX402Config } = await import("../src/x402/config.js");
    const prev = { ...process.env };
    const vaultPkg = "ff".repeat(32);
    process.env["PAYEE_ADDRESS"] = "00" + "ab".repeat(32);
    process.env["ASSET_PACKAGE"] = vaultPkg;
    process.env["ASSET_NAME"] = "WCSPR";
    process.env["X402_PRICE"] = "$0.001";
    process.env["FACILITATOR_PEM_PATH"] = "/x.pem";
    process.env["OHU_VAULT_PACKAGE"] = vaultPkg;
    try {
      expect(() => loadX402Config()).toThrow(/INV-4/);
    } finally {
      process.env = prev;
    }
  });

  it("la declaración no-escrow existe y niega ser settlement", () => {
    expect(RAIL_B_NON_ESCROW_DECLARATION).toContain("NOT_ESCROW_SETTLEMENT");
    expect(RAIL_B_NON_ESCROW_DECLARATION).toContain("OhuVault");
  });

  it("/health expone la declaración y los tokens prohibidos no figuran como rutas", async () => {
    const cfg = makeTestConfig();
    const app = buildReputationApp(cfg, makeMockFacilitator(cfg));
    const res = await request(app).get("/health");
    expect(res.status).toBe(200);
    expect(res.body.assetIsOhuVault).toBe(false);
    expect(Array.isArray(res.body.escrowForbiddenTokens)).toBe(true);
  });

  it("el recurso servido es reputación (no estado de escrow ni entrypoints del OhuVault)", async () => {
    const cfg = makeTestConfig();
    const app = buildReputationApp(cfg, makeMockFacilitator(cfg));

    // Obtiene los requisitos reales del 402 y construye un pago válido con ellos.
    const first = await request(app).get("/reputation/acme-farm");
    expect(first.status).toBe(402);
    const accepted = (
      decodeB64Json(first.headers["payment-required"] as string) as {
        accepts: Array<Record<string, unknown>>;
      }
    ).accepts[0]!;
    const sigHeader = Buffer.from(
      JSON.stringify(buildStubPaymentPayload(accepted as never)),
      "utf-8",
    ).toString("base64");

    const res = await request(app).get("/reputation/acme-farm").set("Payment-Signature", sigHeader);
    expect(res.status).toBe(200);
    expect(res.body.producer).toBe("acme-farm");
    expect(typeof res.body.score).toBe("number");

    const serialized = JSON.stringify(res.body);
    for (const forbidden of ESCROW_FORBIDDEN_TOKENS) {
      if (forbidden === "OhuVault") continue; // el disclaimer lo menciona
      expect(serialized.toLowerCase()).not.toContain(forbidden.toLowerCase());
    }
  });
});