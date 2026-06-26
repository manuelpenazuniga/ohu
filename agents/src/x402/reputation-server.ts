import cors from "cors";
import express, { type Application } from "express";
import { paymentMiddleware, x402ResourceServer } from "@x402/express";
import { ExactCasperScheme } from "@make-software/casper-x402/exact/server";
import type { AssetAmount, Network } from "@x402/core/types";
import type { FacilitatorClient } from "@x402/core/server";
import type { X402Config } from "./config.js";
import {
  ESCROW_FORBIDDEN_TOKENS,
  RAIL_B_NON_ESCROW_DECLARATION,
  SCHEME_EXACT,
} from "./constants.js";

/**
 * Recurso servido por el riel B: el futuro **oráculo de reputación** por
 * request. Es deliberadamente un dato de reputación (no estado de escrow) para
 * evidenciar que x402 cobra por un servicio HTTP, no por liquidar un lote.
 */
export interface ReputationRecord {
  readonly producer: string;
  readonly score: number;
  readonly deliveries: number;
  readonly positiveAttestations: number;
  readonly negativeAttestations: number;
  readonly asOfBlock?: number;
  readonly disclaimer: string;
}

function fakeReputationFor(producer: string): ReputationRecord {
  // En producción esto vendría del contrato `Reputation` (o de CSPR.cloud); el
  // spike devuelve un valor derivable determinista para validar el flujo 402.
  const seed = [...producer].reduce((acc, ch) => acc + ch.charCodeAt(0), 0);
  const score = 40 + (seed % 60);
  return {
    producer,
    score,
    deliveries: seed % 25,
    positiveAttestations: seed % 20,
    negativeAttestations: seed % 3,
    disclaimer: "Rail B x402 — pago por request. No es settlement de escrow.",
  };
}

/**
 * Construye un `AssetAmount` fijo para el money parser del scheme exact. El
 * `asset` es el token CEP-18 configurado (NO el OhuVault — validado en config).
 */
function assetAmountFor(cfg: X402Config): AssetAmount {
  return {
    asset: cfg.assetPackage,
    amount: "0",
    extra: {
      name: cfg.assetName,
      symbol: cfg.assetSymbol,
      version: cfg.assetVersion,
      decimals: String(cfg.assetDecimals),
    },
  };
}

/**
 * Construye la app Express del servidor de recursos (oráculo de reputación)
 * protegido con `paymentMiddleware` del esquema `exact` de Casper.
 *
 * @param cfg      Configuración del riel B.
 * @param facilitator Cliente facilitator (puede ser un `FailoverFacilitatorClient`
 *   con primario hosteado + fallback local). En tests se inyecta un mock.
 */
export function buildReputationApp(cfg: X402Config, facilitator: FacilitatorClient): Application {
  const chainID = cfg.chainID as Network;
  const asset = assetAmountFor(cfg);

  const casperScheme = new ExactCasperScheme()
    .registerAsset(chainID, cfg.assetPackage, cfg.assetDecimals)
    .registerMoneyParser(() => Promise.resolve(asset));

  const resourceServer = new x402ResourceServer(facilitator).register(chainID, casperScheme);

  const app = express();
  app.disable("x-powered-by");
  app.use(
    cors({
      origin: "*",
      methods: ["GET", "OPTIONS"],
      allowedHeaders: ["Accept", "Content-Type", "Origin", "Payment-Signature"],
      exposedHeaders: ["PAYMENT-REQUIRED", "PAYMENT-RESPONSE"],
      maxAge: 24 * 60 * 60,
    }),
  );

  // syncFacilitatorOnStart = true (default): el `x402ResourceServer` necesita
  // conocer los `SupportedKind` del facilitator para construir los
  // `PaymentRequirements`. Con `FailoverFacilitatorClient`, el primario caído se
  // ignora y el local (fallback) responde los kinds → arranca aunque el host
  // esté abajo. El facilitator solo se contacta de nuevo ante un pago (verify/settle).
  app.use(
    paymentMiddleware(
      {
        "GET /reputation/:producer": {
          accepts: [
            {
              scheme: SCHEME_EXACT,
              price: cfg.price,
              network: chainID,
              payTo: cfg.payeeAddress,
            },
          ],
          description: "Oráculo de reputación de productor (pago por request, riel B x402).",
          mimeType: "application/json",
        },
      },
      resourceServer,
    ) as unknown as express.RequestHandler,
  );

  app.get("/reputation/:producer", (req, res) => {
    const producer = (req.params["producer"] ?? "").trim();
    if (!producer) {
      res.status(400).json({ error: "producer requerido" });
      return;
    }
    res.json(fakeReputationFor(producer));
  });

  // Endpoint de sanity que documenta INV-4 en el propio servicio.
  app.get("/health", (_req, res) => {
    res.json({
      status: "ok",
      rail: "B (x402)",
      nonEscrowDeclaration: RAIL_B_NON_ESCROW_DECLARATION,
      forbiddenEscrowTokensPresent: false,
      escrowForbiddenTokens: ESCROW_FORBIDDEN_TOKENS,
      assetIsOhuVault: false,
    });
  });

  return app;
}