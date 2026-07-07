/**
 * Agente **Agregador** (P1-3) — donde el LLM aporta, sin decidir dinero.
 *
 * Flujo: 8 demandas en lenguaje natural → Gemini las normaliza a spec (SOLO
 * normaliza) → bin-packing determinista en lotes → RFQ con ofertas sembradas →
 * clearing determinista (mejor precio + reputación real de P1-1; el LLM NO elige)
 * → `open_lote` on-chain con el productor ganador (operator; no mueve capital).
 *
 * Uso:  tsx --env-file=../.env src/agregador/index.ts [--lote <id>]
 */

import { loadGeminiNormalizer } from "./gemini.js";
import { normalizeBatch } from "./demand-normalizer.js";
import { batchDemands } from "./batching.js";
import { clearRFQ } from "./rfq.js";
import { DEMANDS_RAW, OFFERS } from "./fixtures.js";
import { loadOperatorConfig } from "../casper/env.js";
import { loadOperatorKey } from "../casper/keys.js";
import { openLote } from "../casper/vault-client.js";
import { loadCsprCloudConfig, reputationHistory } from "../x402/reputation-source.js";

function parseLoteArg(argv: readonly string[]): number {
  const i = argv.indexOf("--lote");
  if (i >= 0 && argv[i + 1]) return Number.parseInt(argv[i + 1]!, 10);
  const env = process.env["AGREGADOR_LOTE_ID"];
  return env ? Number.parseInt(env, 10) : 5;
}

async function loadReputation(): Promise<Map<string, number>> {
  const cfg = loadCsprCloudConfig();
  if (!cfg) return new Map();
  try {
    const { scoreFor } = await import("../x402/reputation-source.js");
    const hist = await reputationHistory(cfg, Date.now());
    const rep = new Map<string, number>();
    for (const [prod, h] of hist) rep.set(prod, scoreFor(h));
    return rep;
  } catch {
    return new Map();
  }
}

async function main(): Promise<void> {
  const normalizer = loadGeminiNormalizer();
  if (!normalizer) {
    console.error("Agregador: falta GEMINI_API_KEY en el entorno.");
    process.exit(1);
  }
  const loteId = parseLoteArg(process.argv.slice(2));

  console.log("═══ Agregador · demanda en lenguaje natural → lote on-chain ═══");
  console.log(`Normalizando ${DEMANDS_RAW.length} demandas con Gemini en 1 llamada (el LLM solo normaliza)…\n`);

  const demands = await normalizeBatch(DEMANDS_RAW, normalizer);
  for (const d of demands) {
    console.log(`  ${d.buyerId}: "${d.raw.slice(0, 40)}…"`);
    console.log(`    → ${d.spec.producto} ×${d.spec.cantidad} ${d.spec.calidad} · ${d.spec.ventana} · ${d.spec.zona}` +
      (d.spec.topePrecioUnitario != null ? ` · tope ${d.spec.topePrecioUnitario}` : ""));
  }

  const lotes = batchDemands(demands);
  console.log(`\nBin-packing determinista → ${lotes.length} lote(s):`);
  for (const l of lotes) console.log(`  [${l.key}] ${l.cantidadTotal} u · ${l.buyers.length} compradores`);

  // Lote objetivo: el de mayor volumen (determinista, desempate por key).
  const target = [...lotes].sort((a, b) => b.cantidadTotal - a.cantidadTotal || a.key.localeCompare(b.key))[0]!;
  console.log(`\nRFQ para el lote [${target.key}] (${target.cantidadTotal} u):`);

  const reputation = await loadReputation();
  const clearing = clearRFQ(target, OFFERS, reputation);
  console.log(`  ofertas: ${clearing.consideredOffers} · elegibles: ${clearing.eligibleOffers}`);
  if (!clearing.winner) {
    console.error(`  ✗ ${clearing.reason}. No hay ganador — abortando (no se abre lote).`);
    process.exit(1);
  }
  console.log(`  🏆 ganador: ${clearing.winner.producer.slice(0, 24)}… — ${clearing.reason}`);
  console.log(`  (clearing 100% determinista — el LLM NO eligió; ver rfq.test.ts adversarial)`);

  // open_lote on-chain con el productor ganador (operator; NO mueve capital).
  const config = loadOperatorConfig();
  const key = loadOperatorKey(config.operatorSecretKeyPath);
  console.log(`\nAbriendo lote #${loteId} on-chain (operator) con el ganador…`);
  const res = await openLote(key, loteId, clearing.winner.producer, config);
  if (res.success) {
    console.log(`  ✅ open_lote(#${loteId}) OK · tx ${res.txHash}`);
    console.log(`  https://testnet.cspr.live/deploy/${res.txHash}`);
  } else if (res.pending) {
    console.log(`  ⏳ tx ${res.txHash} enviada, confirmación pendiente (revisar en el explorer).`);
  } else {
    console.error(`  ✗ open_lote revirtió (userError=${res.userError}, tx ${res.txHash}). ` +
      `Si es LoteAlreadyExists=40, usa otro --lote.`);
    process.exit(1);
  }
  console.log(`\nDe ${DEMANDS_RAW.length} mensajes en lenguaje natural salió un lote real en Testnet.`);
}

main().catch((err) => {
  console.error("Agregador: error fatal:", err);
  process.exit(1);
});
