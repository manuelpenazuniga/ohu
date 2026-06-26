import "dotenv/config";
import { x402Client } from "@x402/core/client";
import { wrapFetchWithPayment, type PaymentRequirements } from "@x402/fetch";
import { createClientCasperSigner } from "@make-software/casper-x402";
import { ExactCasperScheme } from "@make-software/casper-x402/exact/client";
import { KeyAlgorithm } from "casper-js-sdk";

/**
 * Cliente pagador x402 del riel B. Paga por un request al oráculo de
 * reputación: GET → 402 (con `PaymentRequirements`) → construye y firma una
 * autorización EIP-712 con la clave Ed25519 del cliente → reintenta con
 * `Payment-Signature` → recibe el recurso.
 *
 * La firma es gasless para el cliente (off-chain); el facilitator asienta
 * on-chain. Esto demuestra el riel B genuino, separado del settlement de
 * escrow (rail A).
 */

export interface PayOptions {
  readonly serverUrl: string;
  readonly endpoint: string; // ej. "/reputation/<producer>"
  readonly clientPemPath: string;
  readonly clientKeyAlgo: "ed25519" | "secp256k1";
  readonly preferNetwork?: string; // ej. "casper:"
}

export interface PayResult {
  readonly status: number;
  readonly body: unknown;
  readonly settle?: unknown;
}

/** Selector: devuelve la primera opción mutuamente soportada, preferida por red. */
function select(prefer: string | undefined) {
  return (_x402Version: number, options: PaymentRequirements[]): PaymentRequirements => {
    if (prefer) {
      const match = options.find((o) => o.network.startsWith(prefer));
      if (match) return match;
    }
    return options[0]!;
  };
}

export async function payForReputation(opts: PayOptions): Promise<PayResult> {
  const algorithm: KeyAlgorithm =
    opts.clientKeyAlgo === "secp256k1" ? KeyAlgorithm.SECP256K1 : KeyAlgorithm.ED25519;
  const casperSigner = await createClientCasperSigner(opts.clientPemPath, algorithm);
  const client = new x402Client(select(opts.preferNetwork)).register(
    "casper:*",
    new ExactCasperScheme(casperSigner),
  );
  const fetchWithPayment = wrapFetchWithPayment(fetch, client);

  const url = `${opts.serverUrl.replace(/\/$/, "")}${opts.endpoint}`;
  const response = await fetchWithPayment(url, { method: "GET" });
  const body = await response.json();

  // extrae el settlement response header si vino (settle exitoso on-chain)
  let settle: unknown;
  try {
    const header = response.headers.get("PAYMENT-RESPONSE");
    settle = header ? JSON.parse(Buffer.from(header, "base64").toString("utf-8")) : undefined;
  } catch {
    settle = undefined;
  }
  return { status: response.status, body, settle };
}