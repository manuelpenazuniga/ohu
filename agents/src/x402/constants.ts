/**
 * Riel B (x402) — constantes y declaración de alcance.
 *
 * INVARIANTE CRÍTICA (INV-4): x402 es un protocolo de pago por request HTTP.
 * NO es el rail de settlement de escrow. El rail A (settlement de escrow) es
 * una transferencia del contrato `OhuVault` autorizada por condiciones on-chain
 * (tally de atestaciones ponderadas) + firmas EIP-712 gasless — jamás un flujo
 * x402. Este riel B solo cobra por servicios HTTP (aquí: el oráculo de
 * reputación por request) asentado sobre un token CEP-18, sin tocar al contrato
 * `OhuVault` ni a su `purse`.
 */

/** CAIP-2 de Casper Testnet. */
export const NETWORK_CASPER_TESTNET = "casper:casper-test" as const;

/** Endpoint RPC público de Testnet (default si no se configura). */
export const TESTNET_RPC_URL = "https://node.testnet.casperlabs.io/rpc" as const;

/** Puerto por defecto del servidor de recursos (oráculo de reputación). */
export const RESOURCE_PORT_DEFAULT = 4021;

/** Puerto por defecto del facilitator local (fallback). */
export const FACILITATOR_PORT_DEFAULT = 4022;

/** Scheme x402 soportado. */
export const SCHEME_EXACT = "exact" as const;

/**
 * Marcadores que NUNCA deben aparecer en el riel B. Si alguno aparece en el
 * código/configuración, significa que x402 está siendo usado para mover fondos
 * de escrow, lo cual rompe INV-4. Los tests negativos barren el módulo x402 en
 * busca de cualquiera de estos tokens.
 */
export const ESCROW_FORBIDDEN_TOKENS: readonly string[] = [
  "OhuVault",
  "release_to_producer",
  "settle_failure",
  "release_to_admin",
  "slash_bond",
  "MutualPool",
  "indemnify",
  // Distinción rails: nunca aludir al escrow como "micropago x402".
  "x402 escrow",
  "escrow x402",
];

/**
 * Declaración explorícita embebida en código: el riel B nunca liquidará
 * settlement de escrow. Sirve de ancla para los tests negativos (debe existir)
 * y de documentación viva.
 */
export const RAIL_B_NON_ESCROW_DECLARATION =
  "RAIL_B_IS_NOT_ESCROW_SETTLEMENT: x402 cobra servicios HTTP (oráculo de " +
  "reputación) sobre un token CEP-18; el settlement de escrow vive en OhuVault " +
  "(rail A: transferencia del contrato + atestaciones on-chain), no aquí.";