import { describe, it, expect } from "vitest";
import request from "supertest";
import { buildReputationApp } from "../src/x402/reputation-server.js";
import {
  makeMockFacilitator,
  makeTestConfig,
  decodeB64Json,
  buildStubPaymentPayload,
  DEV_PAYEE,
} from "./x402-fixtures.js";
import { ESCROW_FORBIDDEN_TOKENS } from "../src/x402/constants.js";

/**
 * Flujo pago del Rail B: 402 → firma (stub) → settle (mock CEP-18) → recurso.
 * Aquí validamos la *forma* del flujo: el servidor entrega reputación tras
 * settle, y el settle resultante es un `transfer_with_authorization` de CEP-18,
 * NUNCA un movimiento del OhuVault. La firma real (EIP-712 Ed25519) y el settle
 * on-chain se validan en la corrida live contra Testnet (README + TODO(audit)).
 */
describe("Rail B x402 — flujo pago contra facilitator mock (CEP-18)", () => {
  it("paga y recibe el recurso de reputación, con settle de CEP-18 (no escrow)", async () => {
    const cfg = makeTestConfig();
    const facilitator = makeMockFacilitator(cfg);
    const app = buildReputationApp(cfg, facilitator);

    const first = await request(app).get("/reputation/acme-farm");
    expect(first.status).toBe(402);
    const accepted = (decodeB64Json(first.headers["payment-required"] as string) as {
      accepts: PaymentRequirementsLike[];
    }).accepts[0]!;

    const payload = buildStubPaymentPayload(accepted);
    const sigHeader = Buffer.from(JSON.stringify(payload), "utf-8").toString("base64");

    const res = await request(app)
      .get("/reputation/acme-farm")
      .set("Payment-Signature", sigHeader);

    expect(res.status).toBe(200);
    expect(res.body.producer).toBe("acme-farm");
    expect(typeof res.body.score).toBe("number");
    expect(res.body.disclaimer).toContain("No es settlement de escrow");

    const settleHeader = res.headers["payment-response"];
    expect(settleHeader).toBeTruthy();
    const settle = decodeB64Json(settleHeader as string) as {
      success: boolean;
      transaction: string;
      network: string;
      extra?: Record<string, unknown>;
    };
    expect(settle.success).toBe(true);
    expect(settle.transaction).toBe("deploy-hash-cep18-transfer-with-authorization");
    expect(settle.extra?.["entryPoint"]).toBe("transfer_with_authorization");

    // Invariante negativa (INV-4): el settle on-chain del Rail B es
    // transfer_with_authorization del token CEP-18 — nada que toque el escrow.
    const settleSerialized = JSON.stringify(settle);
    for (const forbidden of ESCROW_FORBIDDEN_TOKENS) {
      if (forbidden === "x402 escrow" || forbidden === "escrow x402") continue;
      expect(settleSerialized.toLowerCase()).not.toContain(forbidden.toLowerCase());
    }
  });

  it("una firma manipulada ( payload inválido ) no produce recurso cuando el facilitator la rechaza", async () => {
    const cfg = makeTestConfig();
    const rejectingFacilitator = makeMockFacilitator(cfg);
    // Sustituimos verify para que rechace (isValid=false), simulando una firma
    // inválida detectada por el facilitator real.
    const rejecting = {
      ...rejectingFacilitator,
      verify: async () => ({ isValid: false, invalidReason: "invalid_signature" }),
      settle: async () => {
        throw new Error("Settlement aborted: invalid_signature");
      },
    };
    const app = buildReputationApp(cfg, rejecting);

    const first = await request(app).get("/reputation/acme-farm");
    const accepted = (decodeB64Json(first.headers["payment-required"] as string) as {
      accepts: PaymentRequirementsLike[];
    }).accepts[0]!;
    const sigHeader = Buffer.from(
      JSON.stringify(buildStubPaymentPayload(accepted)),
      "utf-8",
    ).toString("base64");

    const res = await request(app)
      .get("/reputation/acme-farm")
      .set("Payment-Signature", sigHeader);

    // Firma inválida => nunca se sirve el recurso de reputación.
    expect(res.status).toBeGreaterThanOrEqual(400);
    expect(res.body).not.toHaveProperty("score");
  });
});

type PaymentRequirementsLike = {
  scheme: string;
  network: string;
  asset: string;
  amount?: string;
  payTo: string;
  [k: string]: unknown;
};