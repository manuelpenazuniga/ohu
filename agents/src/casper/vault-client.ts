import {
  PrivateKey,
  HttpHandler,
  RpcClient,
  ContractCallBuilder,
  Args,
  CLValue,
} from "casper-js-sdk";
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
    .build();

  tx.sign(signer);

  const putResult = await rpc.putTransaction(tx);
  const txHash = putResult.transactionHash.toHex();

  let success = false;
  let userError: number | null = null;

  try {
    const info = await rpc.waitForTransaction(tx, 180000);
    const errorMessage =
      info.executionInfo?.executionResult?.errorMessage ?? undefined;
    if (errorMessage) {
      success = false;
      userError = parseUserError(errorMessage);
    } else {
      success = true;
    }
  } catch (waitErr) {
    success = false;
  }

  return { txHash, success, userError };
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
