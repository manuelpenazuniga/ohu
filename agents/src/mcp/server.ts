/**
 * Ohu MCP Server (F9) — Ohu como INFRAESTRUCTURA para otros agentes.
 *
 * Servidor MCP de SOLO LECTURA sobre stdio que expone el estado on-chain de Ohu
 * como tools para que un agente externo (Claude Desktop/Code) lo consuma:
 *   - get_lote_status(loteId)      · estado del lote + productor
 *   - list_open_lotes()            · lotes OPEN
 *   - get_producer_reputation(p)   · reputación real (P1-1; en prod se cobra vía x402)
 *   - get_pool_solvency()          · solvencia de la MutualPool (P1-4)
 *
 * "Ohu no es una app, es un mercado para agentes."
 *
 * Uso (Claude Desktop/Code): añadir a la config de MCP servers:
 *   { "command": "pnpm", "args": ["--dir", "<repo>/agents", "mcp:server"] }
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import {
  loadCsprCloudConfig,
  reputationHistory,
  scoreFor,
  normalizeProducer,
  type CsprCloudConfig,
} from "../x402/reputation-source.js";
import { mutualReport } from "../x402/mutual-source.js";
import { loteStatus, openLotes } from "../x402/lote-status.js";

function cfgOrThrow(): CsprCloudConfig {
  const cfg = loadCsprCloudConfig();
  if (!cfg) throw new Error("MCP Ohu: falta CSPRCLOUD_API_KEY / OHUVAULT_PACKAGE_HASH en el entorno.");
  return cfg;
}

const asText = (o: unknown) => ({
  content: [{ type: "text" as const, text: JSON.stringify(o, null, 2) }],
});

const server = new McpServer({ name: "ohu", version: "1.0.0" });

server.tool(
  "get_lote_status",
  "Estado on-chain de un lote de Ohu (OPEN/FUNDED/EVAL/SETTLED_OK/SETTLED_FAIL) y su productor adjudicado.",
  { loteId: z.number().int().describe("ID del lote") },
  async ({ loteId }) => asText(await loteStatus(cfgOrThrow(), loteId)),
);

server.tool(
  "list_open_lotes",
  "Lista los lotes actualmente OPEN (abiertos, aún no fondeados) en Ohu.",
  {},
  async () => asText(await openLotes(cfgOrThrow())),
);

server.tool(
  "get_producer_reputation",
  "Reputación on-chain REAL de un productor (lotes OK/FAIL, score 0..100, asOfBlock). En producción este dato se cobra por request vía x402.",
  { producer: z.string().describe("account-hash del productor (con o sin prefijo account-hash-)") },
  async ({ producer }) => {
    const cfg = cfgOrThrow();
    const hist = await reputationHistory(cfg, Date.now());
    const h = hist.get(normalizeProducer(producer));
    return asText(
      h
        ? { producer, score: scoreFor(h), ...h }
        : { producer, score: 50, lotesAwarded: 0, settledOk: 0, settledFail: 0, note: "sin historial on-chain" },
    );
  },
);

server.tool(
  "get_pool_solvency",
  "Solvencia de la MutualPool: reserva, primas cobradas, cola pagada, ratio vs objetivo y recomendación de prima.",
  {},
  async () => {
    const cfg = cfgOrThrow();
    const params = {
      premiumBps: Number(process.env["OHUVAULT_PREMIUM_BPS"] ?? 50),
      indemnityTargetBps: Number(process.env["OHUVAULT_INDEMNITY_TARGET_BPS"] ?? 8000),
    };
    const r = await mutualReport(cfg, params);
    return asText({
      ...r.state,
      targetCspr: r.targetCspr,
      ratio: Number.isFinite(r.ratio) ? r.ratio : null,
      solvent: r.solvent,
      recommendedPremiumBps: r.recommendedPremiumBps,
    });
  },
);

const transport = new StdioServerTransport();
await server.connect(transport);
// stderr para no contaminar el canal MCP (stdout es el protocolo).
console.error("Ohu MCP server (read-only) escuchando en stdio · 4 tools.");
