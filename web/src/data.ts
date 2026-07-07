// Datos REALES del enjambre de Ohu en Casper Testnet (lote 4, liquidado por los
// agentes sin intervención humana el 2026-07-07). Cada hash es una transacción
// verificada on-chain (error_message=None). Ver infra/deployments/testnet.md §P1-2.

export const NETWORK = "casper-test";
export const EXPLORER = "https://testnet.cspr.live/deploy/";
export const VAULT =
  "hash-94c4d7b466a035e0aac9bb60daeaa179432ad2df93de3dfe2759812676bf3b6c";

export const OPERATOR =
  "account-hash-9c28ba3e5c1154fa23085326c9e165de79a32a67b1145edce5e0a2b949f80186";
export const ADMIN =
  "account-hash-59d06759666ef90a065d023c4c2b6a77708c38945380a0b36380f07e71bd70b4";

export type StepKind = "setup" | "agent";
export type Column = "PROPONE" | "AUTORIZA";

export interface LoteStep {
  readonly n: number;
  readonly state: string;
  readonly entrypoint: string;
  readonly actor: string;
  readonly kind: StepKind;
  readonly tx: string;
  readonly column?: Column;
  readonly result?: string;
}

export interface Lote {
  readonly id: number;
  readonly funded: string;
  readonly bond: string;
  readonly premiumBps: number;
  readonly quorumFailBps: number;
  readonly steps: readonly LoteStep[];
}

export const LOTE: Lote = {
  id: 4,
  funded: "10 CSPR",
  bond: "10 CSPR",
  premiumBps: 50,
  quorumFailBps: 6000,
  steps: [
    { n: 1, state: "OPEN", entrypoint: "open_lote", actor: "admin", kind: "setup", tx: "66a2b8e5a945fb7c26628d203011528e409a8d743a16d67025524e37eaf9f03a" },
    { n: 2, state: "OPEN", entrypoint: "deposit_to_lote", actor: "buyer", kind: "setup", tx: "fc46859a4e8dcc50dd5baff7bb8034c3b2988bd5ffb7c7a91da0ef80e3f2e139" },
    { n: 3, state: "OPEN", entrypoint: "post_bond", actor: "producer", kind: "setup", tx: "700a14664c999789c0abbb2f0cfb9e0a3cf0f67a4da31b17abba9c3097bcd5bb" },
    { n: 4, state: "FUNDED", entrypoint: "lock_lote", actor: "admin", kind: "setup", tx: "0e102eab8504785fa6b9cde31d9e3adc53b89f8cf5a1f12aacb5c5f4a88902f4" },
    { n: 5, state: "EVAL_OK", entrypoint: "evaluate_lote", actor: "operator", kind: "agent", column: "PROPONE", result: "EVAL_OK", tx: "58d917305b1552dde941cab76c65ac7d635e55c288069ef6b4cc7ee9a7da21bc" },
    { n: 6, state: "SETTLED_OK", entrypoint: "release_to_producer", actor: "admin", kind: "agent", column: "AUTORIZA", result: "SETTLED_OK", tx: "c1f374a2de8704391edb47de27681eef4c66ceb7b81f6a1965c9a4a065af4c95" },
  ],
};

export const shortHash = (h: string, n = 10): string =>
  h.length > n * 2 ? `${h.slice(0, n)}…${h.slice(-4)}` : h;

export const explorerUrl = (tx: string): string => `${EXPLORER}${tx}`;

// ── Fase b: el enjambre (tarjetas de agente) ──────────────────────────────
export type AgentStatus = "live" | "roadmap";
export interface Agent {
  readonly name: string;
  readonly role: string;
  readonly account?: string;
  readonly status: AgentStatus;
  readonly does: string; // qué hace (donde aporta)
  readonly authority: string; // su límite on-chain
  readonly lastAction?: string;
  readonly lastTx?: string;
}
export const AGENTS: readonly Agent[] = [
  {
    name: "Tesorería", role: "operator", account: OPERATOR, status: "live",
    does: "observa la ventana; dispara evaluate_lote",
    authority: "solo evaluate_lote — no puede mover capital (INV-2)",
    lastAction: "evaluate_lote(4) → EVAL_OK",
    lastTx: "58d917305b1552dde941cab76c65ac7d635e55c288069ef6b4cc7ee9a7da21bc",
  },
  {
    name: "Autorizador", role: "admin", account: ADMIN, status: "live",
    does: "ejecuta el movimiento de capital tras la evaluación",
    authority: "release / settle (admin; futuro multisig nativo, INV-1)",
    lastAction: "release_to_producer(4) → SETTLED_OK",
    lastTx: "c1f374a2de8704391edb47de27681eef4c66ceb7b81f6a1965c9a4a065af4c95",
  },
  {
    name: "Agregador", role: "operator", account: OPERATOR, status: "live",
    does: "demanda en lenguaje natural → spec (Gemini) → bin-packing → RFQ → open_lote",
    authority: "open_lote; el LLM SOLO normaliza, el clearing es determinista (INV-2)",
    lastAction: "8 demandas NL → lote #5 abierto",
    lastTx: "ad202f9d9323cbe8be88f6ec92a202d23cfc1c95cd4b6f3c0d25ba5793add2f1",
  },
  {
    name: "Mutual / Riesgo", role: "observador", status: "live",
    does: "informe de solvencia + propuesta de prima a gobernanza (números on-chain reales)",
    authority: "solo observa (read-only); no ejecuta — la prima la cambia el admin",
    lastAction: "reserva 0.1 CSPR · ratio 10 · SOLVENTE",
  },
];

// ── Fase c: la mutual (gauge) ── derivado del historial on-chain ──────────
export const MUTUAL = {
  pool: "hash-2cbbd92b6b3b6ef3629da0330e7b63213a8a04c03b3721b0dbc2a2d73f685cb0",
  premiumsCspr: 0.1, // 0.5% de 2 releases (lotes 2 y 4, funded 10 CSPR c/u)
  tailPaidCspr: 0, // bond ≥ target ⇒ el pool nunca paga cola
  reserveCspr: 0.1, // primas acumuladas, sin pérdidas
  premiumEvents: [
    { lote: 2, cspr: 0.05 },
    { lote: 4, cspr: 0.05 },
  ],
  note:
    "bond ≥ target se exige en lock_lote → el que falla paga primero desde su bono; " +
    "la mutual es un backstop de cola que hasta hoy no se ha tocado (tail = 0).",
} as const;
