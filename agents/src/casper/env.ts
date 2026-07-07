import "dotenv/config";

/**
 * Configuración del riel A (escrow). Toda se carga desde `.env` en la raíz del
 * repo; ningún secreto se commitea. Reusa el patrón `required()` / `optional()`
 * establecido en `x402/config.ts`.
 */
export interface SwarmConfig {
  /** URL RPC del nodo Casper Testnet. */
  readonly nodeUrl: string;
  /** URL SSE `/events` para la mejora event-driven. */
  readonly eventsUrl: string;
  /** Nombre de la red (casper-test). */
  readonly chainName: string;
  /** Package hash del OhuVault (64 hex sin prefijo `hash-`). */
  readonly vaultPackageHash: string;
  /** Ventana de atestación en milisegundos. */
  readonly attestationWindowMs: number;
  /** Path al PEM Ed25519 del operator (Tesorería). */
  readonly operatorSecretKeyPath: string;
  /** Path al PEM Ed25519 del admin (Autorizador). */
  readonly adminSecretKeyPath: string;
  /** Cuenta-hash del operator para logging de identidad. */
  readonly operatorAccountHash: string;
  /** Cuenta-hash del admin para logging de identidad. */
  readonly adminAccountHash: string;
  /** Intervalo de sondeo en ms (default 15000). */
  readonly pollIntervalMs: number;
  /** Archivo de log (default agents/.swarm-log.jsonl). */
  readonly logFile: string;
  /** Lotes objetivo (CSV), p.ej. "1,2,3". */
  readonly targetLotes: string;
  /** Delay inicial del autorizador en ms (default 0). */
  readonly autorizadorStartDelayMs: number;
  /** Motos de gas por transacción (default 3_000_000_000). */
  readonly txPaymentMotes: number;
}

function required(key: string): string {
  const v = process.env[key];
  if (!v || v.trim() === "") {
    throw new Error(`swarm: variable requerida ausente: ${key}`);
  }
  return v.trim();
}

function optional(key: string, def: string): string {
  const v = process.env[key];
  return v && v.trim() !== "" ? v.trim() : def;
}

/**
 * Normaliza un package hash: elimina el prefijo `hash-` si existe.
 * @internal
 */
export function normalizePackageHash(raw: string): string {
  return raw.replace(/^hash-/, "");
}

/**
 * Carga y valida la configuración compartida del enjambre (Tesorería + Autorizador).
 * Lanza si faltan variables requeridas.
 */
export function loadSwarmConfig(): SwarmConfig {
  const nodeUrl = required("NODE_URL");
  const eventsUrl = required("ODRA_CASPER_LIVENET_EVENTS_URL");
  const chainName = required("CHAIN_NAME");
  const vaultPackageHash = normalizePackageHash(required("OHUVAULT_PACKAGE_HASH"));
  const attestationWindowMs = Number.parseInt(
    required("OHUVAULT_ATTESTATION_WINDOW_MS"),
    10,
  );
  const operatorSecretKeyPath = required("OPERATOR_SECRET_KEY_PATH");
  const adminSecretKeyPath = required("ADMIN_SECRET_KEY_PATH");
  const operatorAccountHash = required("OHUVAULT_OPERATOR_ACCOUNT_HASH");
  const adminAccountHash = required("OHUVAULT_ADMIN_ACCOUNT_HASH");

  const pollIntervalMs = Number.parseInt(
    optional("SWARM_POLL_INTERVAL_MS", "15000"),
    10,
  );
  const logFile = optional("SWARM_LOG_FILE", "agents/.swarm-log.jsonl");
  const targetLotes = optional("SWARM_TARGET_LOTES", "");
  const autorizadorStartDelayMs = Number.parseInt(
    optional("SWARM_AUTORIZADOR_START_DELAY_MS", "0"),
    10,
  );
  const txPaymentMotes = Number.parseInt(
    optional("TX_PAYMENT_MOTES", "3000000000"),
    10,
  );

  if (Number.isNaN(attestationWindowMs) || attestationWindowMs <= 0) {
    throw new Error(
      `swarm: OHUVAULT_ATTESTATION_WINDOW_MS debe ser un número positivo`,
    );
  }
  if (Number.isNaN(pollIntervalMs) || pollIntervalMs <= 0) {
    throw new Error(`swarm: SWARM_POLL_INTERVAL_MS debe ser un número positivo`);
  }
  if (Number.isNaN(txPaymentMotes) || txPaymentMotes <= 0) {
    throw new Error(`swarm: TX_PAYMENT_MOTES debe ser un número positivo`);
  }

  return {
    nodeUrl,
    eventsUrl,
    chainName,
    vaultPackageHash,
    attestationWindowMs,
    operatorSecretKeyPath,
    adminSecretKeyPath,
    operatorAccountHash,
    adminAccountHash,
    pollIntervalMs,
    logFile,
    targetLotes,
    autorizadorStartDelayMs,
    txPaymentMotes,
  };
}
