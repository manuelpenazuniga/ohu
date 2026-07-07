// casper-js-sdk es CJS: bajo node-ESM (tsx) sus named exports quedan `undefined`
// (todo vive en el default export). Se importan los VALORES vía el default y el
// TIPO `PrivateKey` vía `import type`. vitest interopera solo; node/tsx no.
import sdk from "casper-js-sdk";
import type { PrivateKey } from "casper-js-sdk";
const { HttpHandler, RpcClient, ContractCallBuilder, Args, CLValue } = sdk;
import { parseUserError } from "./errors.js";
import type { SwarmConfig } from "./env.js";

/**
 * Entrypoints permitidos para el riel A (escrow settlement). Cualquier otro
 * nombre de entrypoint causa un error inmediato. Esta es la barrera de
 * seguridad que impide que un agente llame entrypoints no previstos en la
 * matriz de autoridad on-chain.
 *
 * **INMUTABLE** — no modificar sin revisión de seguridad.
 */
const ALLOWED_ENTRYPOINTS: readonly string[] = [
  "evaluate_lote",
  "release_to_producer",
  "settle_failure",
];

/** Resultado de una llamada al OhuVault. */
export interface VaultCallResult {
  /** Hash de la transacción. */
  readonly txHash: string;
  /** `true` si la transacción ejecutó sin error. */
  readonly success: boolean;
  /** Código de error de Odra si la tx revirtió, o `null`. */
  readonly userError: number | null;
  /**
   * `true` si la tx se envió (`putTransaction` OK) pero no se confirmó dentro del
   * timeout (finality lag). El caller NO debe re-enviar: debe re-consultar o abortar.
   */
  readonly pending: boolean;
}

/** Timeout total de confirmación de una tx (el Testnet tarda en indexar). */
const CONFIRM_TIMEOUT_MS = 150_000;
/** Intervalo de sondeo de la confirmación. */
const CONFIRM_POLL_MS = 4_000;
/** Espera de asentamiento tras ver el resultado, para leer el error ya finalizado. */
const CONFIRM_SETTLE_MS = 6_000;

const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

/**
 * `true` si el error RPC es `-32014 "No such transaction"` — la tx aún no fue
 * indexada por el nodo. NO es un fallo: hay que seguir esperando la MISMA tx.
 */
function isNoSuchTransaction(err: unknown): boolean {
  const e = err as { code?: number; sourceErr?: { code?: number } } | null;
  return e?.code === -32014 || e?.sourceErr?.code === -32014;
}

/**
 * Construye, firma y despacha una llamada a un entrypoint del OhuVault.
 *
 * **Requisito de seguridad central:** `entryName` DEBE estar en el allowlist
 * `ALLOWED_ENTRYPOINTS`. El único argumento permitido es `lote_id: u64`.
 * Prohibido construir transfers nativos, deploys de sesión o entrypoints
 * dinámicos.
 *
 * @param signer Llave privada del rol que firma (operator o admin).
 * @param entryName Nombre del entrypoint (debe estar en el allowlist).
 * @param loteId ID del lote (u64).
 * @param config Configuración del enjambre.
 * @returns Resultado estructurado con txHash, éxito y código de error si lo hay.
 * @throws Si `entryName` no está en el allowlist.
 */
export async function callVaultEntrypoint(
  signer: PrivateKey,
  entryName: string,
  loteId: number,
  config: SwarmConfig,
): Promise<VaultCallResult> {
  if (!ALLOWED_ENTRYPOINTS.includes(entryName)) {
    throw new Error(
      `vault-client: entrypoint no permitido "${entryName}". ` +
        `Allowlist: ${ALLOWED_ENTRYPOINTS.join(", ")}`,
    );
  }

  const rpc = new RpcClient(new HttpHandler(config.nodeUrl, "fetch"));
  const pub = signer.publicKey;

  const args = Args.fromMap({
    lote_id: CLValue.newCLUint64(loteId),
  });

  const tx = new ContractCallBuilder()
    .byPackageHash(config.vaultPackageHash)
    .entryPoint(entryName)
    .runtimeArgs(args)
    .from(pub)
    .chainName(config.chainName)
    .payment(config.txPaymentMotes)
    // Testnet NO tiene AddressableEntity activado (hard-constraint del proyecto):
    // `.build()` produce una TransactionV1 nativa que el nodo rechaza con
    // "no such addressable entity". `.buildFor1_5()` emite el Deploy legacy
    // (modelo Contract/ContractPackage) que el testnet acepta — igual que los
    // binarios Rust (odra-livenet).
    .buildFor1_5();

  tx.sign(signer);

  const putResult = await rpc.putTransaction(tx);
  const txHash = putResult.transactionHash.toHex();

  // ── Confirmación robusta (E2E hardening) ───────────────────────────────
  // `putTransaction` ya envió la tx: NUNCA se re-envía. El nodo del Testnet
  // tarda en indexar la tx recién enviada y responde `-32014 "No such
  // transaction"` durante ~30-60s (finality lag). Se espera ESTA misma tx por
  // su hash, con backoff, tolerando -32014, hasta un timeout generoso.
  const deadline = Date.now() + CONFIRM_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      // `buildFor1_5` emite un Deploy legacy: su hash es un DEPLOY hash, hay que
      // consultarlo por deploy hash (no por transaction hash, que devolvería
      // -32014 aunque el deploy sí exista y haya ejecutado).
      const info = await rpc.getTransactionByDeployHash(txHash);
      if (info.executionInfo) {
        // El `errorMessage` (p.ej. "Out of gas error") puede poblarse un instante
        // DESPUÉS de que aparece `executionInfo`: leer aquí a secas reportaría un
        // éxito falso. Se re-lee tras un breve settle para tomar el resultado ya
        // finalizado como autoritativo.
        await sleep(CONFIRM_SETTLE_MS);
        const finalInfo = await rpc.getTransactionByDeployHash(txHash);
        const errorMessage =
          finalInfo.executionInfo?.executionResult?.errorMessage ??
          info.executionInfo.executionResult?.errorMessage ??
          undefined;
        if (errorMessage) {
          // Revert de Odra ("User error: N") o error de sistema (out of gas, etc.).
          return {
            txHash,
            success: false,
            userError: parseUserError(errorMessage),
            pending: false,
          };
        }
        return { txHash, success: true, userError: null, pending: false };
      }
      // Aceptada pero aún sin executionInfo: seguir esperando.
    } catch (err) {
      // -32014 = tx aún no visible → seguir esperando (NO re-enviar).
      // Cualquier otro error (red / RPC distinto) sí es terminal: propagar.
      if (!isNoSuchTransaction(err)) {
        throw err;
      }
    }
    await sleep(CONFIRM_POLL_MS);
  }

  // Enviada pero no confirmada en el timeout: pending. El caller NO re-envía.
  return { txHash, success: false, userError: null, pending: true };
}

/**
 * Evalúa un lote (operator). Llama a `evaluate_lote` en el OhuVault.
 * No mueve capital — el contrato solo lee el tally on-chain y fija
 * EVAL_OK / EVAL_FAIL.
 *
 * @param signer Llave privada del operator.
 * @param loteId ID del lote.
 * @param config Configuración del enjambre.
 */
export async function evaluateLote(
  signer: PrivateKey,
  loteId: number,
  config: SwarmConfig,
): Promise<VaultCallResult> {
  return callVaultEntrypoint(signer, "evaluate_lote", loteId, config);
}

/**
 * Libera el pago al productor (admin). Llama a `release_to_producer`.
 * Mueve capital — requiere llave admin.
 *
 * @param signer Llave privada del admin.
 * @param loteId ID del lote.
 * @param config Configuración del enjambre.
 */
export async function releaseToProducer(
  signer: PrivateKey,
  loteId: number,
  config: SwarmConfig,
): Promise<VaultCallResult> {
  return callVaultEntrypoint(signer, "release_to_producer", loteId, config);
}

/**
 * Liquida un lote fallido (admin). Llama a `settle_failure`.
 * Mueve capital: refund + slash + indemnización.
 *
 * @param signer Llave privada del admin.
 * @param loteId ID del lote.
 * @param config Configuración del enjambre.
 */
export async function settleFailure(
  signer: PrivateKey,
  loteId: number,
  config: SwarmConfig,
): Promise<VaultCallResult> {
  return callVaultEntrypoint(signer, "settle_failure", loteId, config);
}
