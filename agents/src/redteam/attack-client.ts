/**
 * HERRAMIENTA DE RED-TEAM (F2). Construye y despacha llamadas ARBITRARIAS al
 * OhuVault para DEMOSTRAR que revierten on-chain. Deliberadamente NO usa el
 * allowlist del `vault-client` (esa es la vía segura); esto es el adversario
 * intentando saltársela. Todo aquí produce reverts — jamás mueve capital.
 */

import sdk from "casper-js-sdk";
import type { PrivateKey } from "casper-js-sdk";
const { HttpHandler, RpcClient, ContractCallBuilder } = sdk;
import { parseUserError } from "../casper/errors.js";
import type { SwarmConfig } from "../casper/env.js";

export interface AttackResult {
  readonly txHash: string;
  /** `true` si (inesperadamente) NO revirtió. En un ataque esperamos `false`. */
  readonly success: boolean;
  /** Código de error de Odra del revert (lo que queremos mostrar), o `null`. */
  readonly userError: number | null;
}

const CONFIRM_TIMEOUT_MS = 150_000;
const CONFIRM_POLL_MS = 4_000;
const CONFIRM_SETTLE_MS = 6_000;
const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));
const isNoSuchTx = (e: unknown): boolean => {
  const x = e as { code?: number; sourceErr?: { code?: number } } | null;
  return x?.code === -32014 || x?.sourceErr?.code === -32014;
};

/** Despacha un ataque (entrypoint + args arbitrarios) y confirma su revert. */
export async function sendAttack(
  signer: PrivateKey,
  entryName: string,
  args: ReturnType<typeof sdk.Args.fromMap>,
  config: SwarmConfig,
): Promise<AttackResult> {
  const rpc = new RpcClient(new HttpHandler(config.nodeUrl, "fetch"));
  const tx = new ContractCallBuilder()
    .byPackageHash(config.vaultPackageHash)
    .entryPoint(entryName)
    .runtimeArgs(args)
    .from(signer.publicKey)
    .chainName(config.chainName)
    .payment(config.txPaymentMotes)
    .buildFor1_5();
  tx.sign(signer);

  const putResult = await rpc.putTransaction(tx);
  const txHash = putResult.transactionHash.toHex();

  const deadline = Date.now() + CONFIRM_TIMEOUT_MS;
  while (Date.now() < deadline) {
    try {
      const info = await rpc.getTransactionByDeployHash(txHash);
      if (info.executionInfo) {
        await sleep(CONFIRM_SETTLE_MS);
        const finalInfo = await rpc.getTransactionByDeployHash(txHash);
        const errorMessage =
          finalInfo.executionInfo?.executionResult?.errorMessage ??
          info.executionInfo.executionResult?.errorMessage ??
          undefined;
        if (errorMessage) {
          return { txHash, success: false, userError: parseUserError(errorMessage) };
        }
        return { txHash, success: true, userError: null };
      }
    } catch (err) {
      if (!isNoSuchTx(err)) throw err;
    }
    await sleep(CONFIRM_POLL_MS);
  }
  return { txHash, success: false, userError: null };
}
