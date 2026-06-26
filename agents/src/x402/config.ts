import "dotenv/config";
import {
  NETWORK_CASPER_TESTNET,
  TESTNET_RPC_URL,
  RESOURCE_PORT_DEFAULT,
  FACILITATOR_PORT_DEFAULT,
  ESCROW_FORBIDDEN_TOKENS,
} from "./constants.js";
import { isValidAddress, isValidContractPackageHash } from "@make-software/casper-x402";

/**
 * Configuración del riel B (x402). Toda se carga desde `.env` (ver
 * `agents/.env.example` / `infra/.env.example`); ningún secreto se commitea.
 *
 * Modelo: el servidor de recursos cobra por request sobre un token CEP-18
 * (`ASSET_PACKAGE`) que NO es el contrato `OhuVault`. El facilitator local firma
 * deploys contra Testnet (`FACILITATOR_RPC_URL`); si el facilitator hosteado
 * (`FACILITATOR_HOSTED_URL`) falla, el servidor falla al local.
 */
export interface X402Config {
  /** CAIP-2, por defecto Testnet. */
  readonly chainID: string;
  /** Cuenta-hash (66 hex, prefijo `00`) que cobra por el oráculo. */
  readonly payeeAddress: string;
  /** Hash de paquete (64 hex) del token CEP-18 usado para pagar. */
  readonly assetPackage: string;
  readonly assetName: string;
  readonly assetSymbol: string;
  readonly assetVersion: string;
  readonly assetDecimals: number;
  /** Precio en formato money de x402, ej. "$0.001". */
  readonly price: string;

  /** URL del facilitator hosteado (primario). Vacío => solo local. */
  readonly facilitatorHostedUrl: string;
  /** URL del facilitator local (fallback). */
  readonly facilitatorLocalUrl: string;
  readonly facilitatorApiKey: string;

  /** Material de firma del facilitator local. */
  readonly facilitatorPemPath: string;
  readonly facilitatorKeyAlgo: "ed25519" | "secp256k1";
  readonly facilitatorRpcUrl: string;
  /** Motos de gas por deploy del facilitator (testnet). */
  readonly facilitatorPaymentMotes: string;

  readonly resourcePort: number;
  readonly facilitatorPort: number;

  /**
   * Hash de paquete del contrato `OhuVault` (escrow), si se conoce. Se usa
   * SOLO como centinela: si `ASSET_PACKAGE` coincide con este valor, la carga
   * falla — x402 jamás cobra contra el contrato de escrow (INV-4).
   */
  readonly ohuVaultPackage?: string;
}

function required(key: string): string {
  const v = process.env[key];
  if (!v || v.trim() === "") {
    throw new Error(`x402: variable requerida ausente: ${key}`);
  }
  return v.trim();
}

function optional(key: string, def: string): string {
  const v = process.env[key];
  return v && v.trim() !== "" ? v.trim() : def;
}

/**
 * Carga y valida la configuración del riel B.
 *
 * @remarks Lanza (no retorna) si la configuración mezcla el riel B con el
 * escrow: si `ASSET_PACKAGE` apunta al `OhuVault` o si alguno de los tokens
 * prohibidos aparece en los campos de configuración.
 */
export function loadX402Config(): X402Config {
  const chainID = optional("CAIP2_CHAIN_ID", NETWORK_CASPER_TESTNET);
  const payeeAddress = required("PAYEE_ADDRESS");
  const assetPackageRaw = required("ASSET_PACKAGE");
  const assetPackage = assetPackageRaw.replace(/^hash-/, "");
  const ohuVaultPackage = process.env["OHU_VAULT_PACKAGE"]?.trim() || undefined;

  if (!isValidAddress(payeeAddress)) {
    throw new Error(`x402: PAYEE_ADDRESS inválido (se espera cuenta-hash 00-prefijo 66 hex): ${payeeAddress}`);
  }
  if (!isValidContractPackageHash(assetPackage)) {
    throw new Error(`x402: ASSET_PACKAGE inválido (se espera hash de paquete 64 hex): ${assetPackage}`);
  }

  // INV-4: el activo del riel B es un token CEP-18, NUNCA el OhuVault.
  if (ohuVaultPackage && assetPackage.replace(/^hash-/, "") === ohuVaultPackage.replace(/^hash-/, "")) {
    throw new Error(
      "x402: INV-4 violado — ASSET_PACKAGE apunta al OhuVault (escrow). " +
        "x402 cobra servicios HTTP sobre un token CEP-18, no el settlement de escrow.",
    );
  }

  const config: X402Config = {
    chainID,
    payeeAddress,
    assetPackage,
    assetName: required("ASSET_NAME"),
    assetSymbol: optional("ASSET_SYMBOL", "WCSPR"),
    assetVersion: optional("ASSET_VERSION", "1"),
    assetDecimals: Number.parseInt(optional("ASSET_DECIMALS", "9"), 10),
    price: required("X402_PRICE"),
    facilitatorHostedUrl: optional("FACILITATOR_HOSTED_URL", ""),
    facilitatorLocalUrl: optional("FACILITATOR_LOCAL_URL", `http://localhost:${FACILITATOR_PORT_DEFAULT}`),
    facilitatorApiKey: optional("FACILITATOR_API_KEY", ""),
    facilitatorPemPath: required("FACILITATOR_PEM_PATH"),
    facilitatorKeyAlgo: optional("FACILITATOR_KEY_ALGO", "ed25519") === "secp256k1" ? "secp256k1" : "ed25519",
    facilitatorRpcUrl: optional("FACILITATOR_RPC_URL", TESTNET_RPC_URL),
    facilitatorPaymentMotes: optional("FACILITATOR_PAYMENT_MOTES", "1000000000"),
    resourcePort: Number.parseInt(optional("RESOURCE_PORT", String(RESOURCE_PORT_DEFAULT)), 10),
    facilitatorPort: Number.parseInt(optional("FACILITATOR_PORT", String(FACILITATOR_PORT_DEFAULT)), 10),
    ohuVaultPackage,
  };

  // INV-4: ninguno de los tokens prohibidos aparece en los valores de config.
  const serialized = JSON.stringify(config).toLowerCase();
  for (const forbidden of ESCROW_FORBIDDEN_TOKENS) {
    if (serialized.includes(forbidden.toLowerCase())) {
      throw new Error(`x402: INV-4 — token prohibido en config del riel B: "${forbidden}"`);
    }
  }
  return config;
}