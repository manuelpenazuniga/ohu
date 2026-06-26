import express, { type Application } from "express";
import { x402Facilitator } from "@x402/core/facilitator";
import { ExactCasperScheme } from "@make-software/casper-x402/exact/facilitator";
import {
  createFacilitatorCasperSigner,
  type FacilitatorCasperSigner,
} from "@make-software/casper-x402";
import { KeyAlgorithm } from "casper-js-sdk";
import type { Network } from "@x402/core/types";
import type { X402Config } from "./config.js";

/**
 * Construye la app Express del **facilitator local** (fallback) que verifica y
 * asienta pagos x402 sobre Casper Testnet.
 *
 * El facilitator local es matemáticamente equivalente al hosteado: verifica la
 * firma EIP-712 del cliente, monta un `transfer_with_authorization` contra el
 * token CEP-18, lo firma con su clave Ed25519 y lo envía a Testnet.
 *
 * INV-4: asienta un pago de token CEP-18, nunca un movimiento del `OhuVault`.
 */
export async function buildFacilitatorApp(cfg: X402Config): Promise<{
  app: Application;
  signer: FacilitatorCasperSigner;
}> {
  const algorithm: KeyAlgorithm =
    cfg.facilitatorKeyAlgo === "secp256k1"
      ? KeyAlgorithm.SECP256K1
      : KeyAlgorithm.ED25519;

  // createFacilitatorCasperSigner(pemPath, algorithm, rpcUrl) — claves Ed25519
  // por defecto. El facilitator tiene su propia identidad on-chain (cuenta
  // propia), independiente del contrato, requisito del invariante de agente.
  const signer = await createFacilitatorCasperSigner(
    cfg.facilitatorPemPath,
    algorithm,
    cfg.facilitatorRpcUrl,
  );

  const facilitator = new x402Facilitator();
  facilitator.register(
    cfg.chainID as Network,
    new ExactCasperScheme(signer, {
      limitedPaymentMotes: Number(cfg.facilitatorPaymentMotes),
    }),
  );

  const app: Application = express();
  app.disable("x-powered-by");
  app.use(express.json({ limit: "1mb" }));

  app.post("/verify", async (req, res) => {
    const { paymentPayload, paymentRequirements } = req.body ?? {};
    if (!paymentPayload || !paymentRequirements) {
      res.status(400).json({ error: "Missing paymentPayload or paymentRequirements" });
      return;
    }
    try {
      const response = await facilitator.verify(paymentPayload, paymentRequirements);
      res.json(response);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown error";
      res.status(500).json({ error: message });
    }
  });

  app.post("/settle", async (req, res) => {
    const { paymentPayload, paymentRequirements } = req.body ?? {};
    if (!paymentPayload || !paymentRequirements) {
      res.status(400).json({ error: "Missing paymentPayload or paymentRequirements" });
      return;
    }
    try {
      const response = await facilitator.settle(paymentPayload, paymentRequirements);
      res.json(response);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown error";
      if (message.includes("Settlement aborted:")) {
        res.json({
          success: false,
          errorReason: message.replace("Settlement aborted: ", ""),
          network: cfg.chainID,
        });
        return;
      }
      res.status(500).json({ error: message });
    }
  });

  app.get("/supported", (_req, res) => {
    res.json(facilitator.getSupported());
  });

  app.get("/health", (_req, res) => {
    res.json({ status: "ok", network: cfg.chainID, rpc: cfg.facilitatorRpcUrl });
  });

  return { app, signer };
}