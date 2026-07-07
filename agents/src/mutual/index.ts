/**
 * Agente **Mutual / Riesgo** (P1-4) — el tercero del enjambre, liviano.
 *
 * Observa la solvencia de la `MutualPool` derivada del historial on-chain
 * (CSPR.cloud) y emite un **informe de solvencia** + una **recomendación de
 * prima como propuesta de gobernanza**. NO ejecuta nada: la prima la cambia el
 * admin (M-de-N) si corresponde. Números 100% deterministas; el LLM (si se
 * añade) solo redactaría la prosa, nunca decide.
 *
 * Uso:  tsx --env-file=../.env src/mutual/index.ts
 */

import { appendFileSync } from "node:fs";
import { loadCsprCloudConfig } from "../x402/reputation-source.js";
import { mutualReport } from "../x402/mutual-source.js";

async function main(): Promise<void> {
  const cfg = loadCsprCloudConfig();
  if (!cfg) {
    console.error(
      "Mutual/Riesgo: falta CSPRCLOUD_API_KEY / OHUVAULT_PACKAGE_HASH en el entorno.",
    );
    process.exit(1);
  }
  const params = {
    premiumBps: Number(process.env["OHUVAULT_PREMIUM_BPS"] ?? 50),
    indemnityTargetBps: Number(process.env["OHUVAULT_INDEMNITY_TARGET_BPS"] ?? 8000),
  };

  const report = await mutualReport(cfg, params);
  const s = report.state;

  console.log("═══ Informe de solvencia · agente Mutual/Riesgo ═══");
  console.log(
    `  reserva ${s.reserveCspr.toFixed(4)} CSPR · primas ${s.premiumsCspr.toFixed(4)} · ` +
      `cola pagada ${s.tailPaidCspr.toFixed(4)} · lotes ${s.lotesReleased} OK / ${s.lotesFailed} fallo · ` +
      `asOfBlock ${s.asOfBlock}`,
  );
  console.log(
    `  objetivo ${report.targetCspr.toFixed(4)} CSPR · ratio ` +
      `${Number.isFinite(report.ratio) ? report.ratio.toFixed(2) : "∞"} · ` +
      `${report.solvent ? "SOLVENTE ✅" : "BAJO OBJETIVO ⚠️"}`,
  );
  console.log(`  ${report.narrative}`);
  console.log(
    "  (propuesta, NO ejecución — INV-2: la prima la cambia el admin M-de-N si corresponde)",
  );

  // Registro estructurado para el dashboard / historial del enjambre.
  const logFile = process.env["SWARM_LOG_FILE"] ?? ".swarm-log.jsonl";
  const entry = {
    ts: new Date().toISOString(),
    role: "mutual",
    column: "INFORME",
    ...s,
    targetCspr: report.targetCspr,
    ratio: Number.isFinite(report.ratio) ? report.ratio : null,
    solvent: report.solvent,
    recommendedPremiumBps: report.recommendedPremiumBps,
    narrative: report.narrative,
  };
  appendFileSync(logFile, `${JSON.stringify(entry)}\n`);
  console.log(`\n  informe registrado en ${logFile}`);
}

main().catch((err) => {
  console.error("Mutual/Riesgo: error fatal:", err);
  process.exit(1);
});
