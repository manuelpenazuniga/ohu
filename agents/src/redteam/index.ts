/**
 * Red-team en vivo (F2) — "Try to drain the vault". Envía 3 ataques REALES a
 * Testnet; el contrato los RECHAZA on-chain (tres protecciones distintas):
 *   1. cap de gasto del agente        → CapExceeded
 *   2. separación de roles            → NotAdmin (el agente no puede ejecutar retiros)
 *   3. integridad de la máquina de estados → LoteNotFailable (ni el admin salta las reglas)
 * Todo revierte; jamás mueve capital. Es la respuesta escénica al pitch.
 *
 * Uso:  tsx --env-file=../.env src/redteam/index.ts
 */

import { appendFileSync } from "node:fs";
import sdk from "casper-js-sdk";
const { Args, CLValue, Key } = sdk;
import { loadOperatorConfig, loadAdminConfig } from "../casper/env.js";
import { loadOperatorKey, loadAdminKey } from "../casper/keys.js";
import { sendAttack } from "./attack-client.js";
import type { SwarmConfig } from "../casper/env.js";
import type { PrivateKey } from "casper-js-sdk";

const VICTIM = "account-hash-33518b62a4434cb640d6239c86e86f1ed1c132df9ddc2d1cf6f629913ad1f1ba";

interface Attack {
  readonly title: string;
  readonly role: "operator" | "admin";
  readonly entry: string;
  readonly args: ReturnType<typeof sdk.Args.fromMap>;
  readonly expectCode: number;
  readonly expectName: string;
  readonly story: string;
}

function attacks(): Attack[] {
  return [
    {
      title: "El agente intenta gastar sobre su cap",
      role: "operator", entry: "route_micropayment",
      args: Args.fromMap({
        recipient: CLValue.newCLKey(Key.newKey(VICTIM)),
        amount: CLValue.newCLUInt512(5_000_000_000),
      }),
      expectCode: 6, expectName: "CapExceeded",
      story: "route_micropayment de 5 CSPR con micropayment_cap de 1 CSPR",
    },
    {
      title: "El agente intenta ejecutar un retiro (no es admin/M-de-N)",
      role: "operator", entry: "execute",
      args: Args.fromMap({ request_id: CLValue.newCLUint64(0) }),
      expectCode: 3, expectName: "NotAdmin",
      story: "execute() firmado por el operator, no por el admin",
    },
    {
      title: "Liquidar un lote que la máquina de estados dice que NO falló",
      role: "admin", entry: "settle_failure",
      args: Args.fromMap({ lote_id: CLValue.newCLUint64(4) }),
      expectCode: 56, expectName: "LoteNotFailable",
      story: "settle_failure(4) con el lote 4 en SETTLED_OK (no EVAL_FAIL)",
    },
  ];
}

async function main(): Promise<void> {
  const opCfg = loadOperatorConfig();
  const opKey = loadOperatorKey(opCfg.operatorSecretKeyPath);
  const adCfg = loadAdminConfig();
  const adKey = loadAdminKey(adCfg.adminSecretKeyPath);

  console.log("═══ Red-team en vivo · Try to drain the vault ═══");
  console.log("Cada intento se ENVÍA a Casper Testnet y el contrato lo RECHAZA on-chain.\n");

  const logFile = process.env["SWARM_LOG_FILE"] ?? ".redteam-log.jsonl";
  let allBlocked = true;

  for (const a of attacks()) {
    const key: PrivateKey = a.role === "operator" ? opKey : adKey;
    const cfg: SwarmConfig = a.role === "operator" ? opCfg : adCfg;
    console.log(`▶ ${a.title}`);
    console.log(`   ${a.role} · ${a.story}`);
    const res = await sendAttack(key, a.entry, a.args, cfg);
    const blocked = !res.success && res.userError === a.expectCode;
    allBlocked &&= blocked;
    console.log(
      blocked
        ? `   ✅ RECHAZADO on-chain: ${a.expectName} (userError=${res.userError})`
        : `   ⚠️ INESPERADO: success=${res.success} userError=${res.userError} (esperaba ${a.expectCode})`,
    );
    console.log(`   tx: https://testnet.cspr.live/deploy/${res.txHash}\n`);
    appendFileSync(
      logFile,
      `${JSON.stringify({ ts: new Date().toISOString(), attack: a.title, entry: a.entry, expected: a.expectName, userError: res.userError, blocked, txHash: res.txHash })}\n`,
    );
  }

  console.log(
    allBlocked
      ? "Los 3 ataques revirtieron on-chain. El contrato autoriza; ni el agente ni el admin saltan las reglas."
      : "⚠️ Algún ataque no revirtió como se esperaba — revisar.",
  );
  if (!allBlocked) process.exit(1);
}

main().catch((err) => {
  console.error("Red-team: error fatal:", err);
  process.exit(1);
});
