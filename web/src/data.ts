// Datos REALES del enjambre de Ohu en Casper Testnet (lote 4, liquidado por los
// agentes sin intervención humana el 2026-07-07). Cada hash es una transacción
// verificada on-chain (error_message=None). Ver infra/deployments/testnet.md §P1-2.
// La UI es en inglés (audiencia del buildathon) — ver CLAUDE.md.

export const NETWORK = "casper-test";
export const EXPLORER = "https://testnet.cspr.live/deploy/";
export const VAULT =
  "hash-94c4d7b466a035e0aac9bb60daeaa179432ad2df93de3dfe2759812676bf3b6c";

export const OPERATOR =
  "account-hash-9c28ba3e5c1154fa23085326c9e165de79a32a67b1145edce5e0a2b949f80186";
export const ADMIN =
  "account-hash-59d06759666ef90a065d023c4c2b6a77708c38945380a0b36380f07e71bd70b4";

export type StepKind = "setup" | "agent";
export type Column = "PROPOSES" | "AUTHORIZES";

export interface LoteStep {
  readonly n: number;
  readonly state: string;
  readonly entrypoint: string;
  readonly actor: string;
  readonly kind: StepKind;
  readonly tx: string;
  readonly column?: Column;
  readonly result?: string;
  /** Qué hace este paso, en cristiano — se muestra bajo el entrypoint. */
  readonly explain: string;
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
    {
      n: 1, state: "OPEN", entrypoint: "open_lote", actor: "admin", kind: "setup",
      tx: "66a2b8e5a945fb7c26628d203011528e409a8d743a16d67025524e37eaf9f03a",
      explain: "A furrow is opened in the ledger: target amount, delivery window, premium and quorum are fixed on-chain. Nobody can quietly change the rules afterwards.",
    },
    {
      n: 2, state: "OPEN", entrypoint: "deposit_to_lote", actor: "buyer", kind: "setup",
      tx: "fc46859a4e8dcc50dd5baff7bb8034c3b2988bd5ffb7c7a91da0ef80e3f2e139",
      explain: "Buyers sow their money: deposits are earmarked to THIS batch inside the vault's purse. They can only travel to its producer — or back home.",
    },
    {
      n: 3, state: "OPEN", entrypoint: "post_bond", actor: "producer", kind: "setup",
      tx: "700a14664c999789c0abbb2f0cfb9e0a3cf0f67a4da31b17abba9c3097bcd5bb",
      explain: "The producer stakes a performance bond ≥ the batch target. If delivery fails, this bond pays first — skin in the game before a single crate moves.",
    },
    {
      n: 4, state: "FUNDED", entrypoint: "lock_lote", actor: "admin", kind: "setup",
      tx: "0e102eab8504785fa6b9cde31d9e3adc53b89f8cf5a1f12aacb5c5f4a88902f4",
      explain: "The batch sprouts: funding target met and bond verified, so the state machine locks it. From here on, only attestations and arithmetic decide the outcome.",
    },
    {
      n: 5, state: "EVAL_OK", entrypoint: "evaluate_lote", actor: "operator", kind: "agent",
      column: "PROPOSES", result: "EVAL_OK",
      tx: "58d917305b1552dde941cab76c65ac7d635e55c288069ef6b4cc7ee9a7da21bc",
      explain: "The Treasury agent tallies the signed delivery attestations. It can only PROPOSE the verdict the numbers already say — it cannot touch a single mote.",
    },
    {
      n: 6, state: "SETTLED_OK", entrypoint: "release_to_producer", actor: "admin", kind: "agent",
      column: "AUTHORIZES", result: "SETTLED_OK",
      tx: "c1f374a2de8704391edb47de27681eef4c66ceb7b81f6a1965c9a4a065af4c95",
      explain: "A separate admin identity executes what the on-chain tally authorized: escrow → producer, bond back, premium → mutual. Harvest in the barn, hands-free.",
    },
  ],
};

/** F2 · red-team: ataques REALES a Testnet que el contrato rechazó on-chain. */
export interface RedTeamAttempt {
  readonly attack: string;
  readonly by: string;
  readonly entrypoint: string;
  readonly error: string;
  readonly protection: string;
  readonly tx: string;
}
export const REDTEAM: readonly RedTeamAttempt[] = [
  { attack: "Agent spends over its cap", by: "operator", entrypoint: "route_micropayment(5 CSPR)", error: "CapExceeded", protection: "spending cap (INV-1)", tx: "89354977b39d58c2b21403a7032e7ca10ef8a7e4c16105100e0d8d64c6e2b27f" },
  { attack: "Agent tries to execute a withdrawal", by: "operator", entrypoint: "execute()", error: "NotAdmin", protection: "role separation", tx: "67dc7eb30beff8d0f633da2d5cdfeed9dc9040f93e912d4e5dc81ea94a8da0b9" },
  { attack: "Settle a batch that didn't fail", by: "admin", entrypoint: "settle_failure(4)", error: "LoteNotFailable", protection: "state machine", tx: "979b6c3eda28f76e8f5a0cbc40667936e9a1c99d9443a3c6ce0a35d41e6fad9a" },
];

/** F10 · Trust: invariantes, lentes de auditoría y casos reales cazados. */
export const INVARIANTS: ReadonlyArray<{ id: string; text: string }> = [
  { id: "INV-1", text: "The agent never moves meaningful capital — capped entry points only" },
  { id: "INV-2", text: "No release depends on the LLM — the on-chain tally authorizes it" },
  { id: "INV-3", text: "No Addressable Entity — purse + native multisig + M-of-N in the contract" },
  { id: "INV-4", text: "x402 only for HTTP services — escrow settles as a contract transfer" },
  { id: "INV-5", text: "Ed25519 attestations signed off-chain, verified on-chain (gasless)" },
  { id: "INV-6", text: "Closed-circuit data — arithmetic settlement over weighted attestations" },
  { id: "INV-7", text: "Escrow earmarked per batch — funds go to its producer or back, nowhere else" },
];

export const AUDIT_LENSES: ReadonlyArray<{ model: string; lens: string }> = [
  { model: "Claude", lens: "conservation of funds" },
  { model: "Gemini 3.1 Pro", lens: "algebraic correctness" },
  { model: "GPT-5.5", lens: "adversarial · game theory" },
];

export const CASE_STUDIES: ReadonlyArray<{ bug: string; caughtBy: string; missedBy: string; fix: string }> = [
  { bug: "A 1-mote bond drains the mutual (economic ring)", caughtBy: "GPT-5.5 (adversarial)", missedBy: "176 green tests", fix: "bond ≥ target enforced in lock_lote" },
  { bug: "Config leaked the admin key path to the operator process", caughtBy: "GPT-5.5 (adversarial)", missedBy: "Gemini's correction pass", fix: "loadOperatorConfig / loadAdminConfig — structural separation" },
  { bug: "Agent reported SETTLED_OK on a reverted tx (Out of gas)", caughtBy: "the real on-chain E2E", missedBy: "typecheck + mocked tests", fix: "double-read confirmation (no premature success)" },
];

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
    name: "Treasury", role: "operator", account: OPERATOR, status: "live",
    does: "watches the delivery window; fires evaluate_lote when it closes",
    authority: "evaluate_lote only — it cannot move capital (INV-2)",
    lastAction: "evaluate_lote(4) → EVAL_OK",
    lastTx: "58d917305b1552dde941cab76c65ac7d635e55c288069ef6b4cc7ee9a7da21bc",
  },
  {
    name: "Authorizer", role: "admin", account: ADMIN, status: "live",
    does: "executes the capital move after the on-chain tally",
    authority: "release / settle (admin; native multisig next, INV-1)",
    lastAction: "release_to_producer(4) → SETTLED_OK",
    lastTx: "c1f374a2de8704391edb47de27681eef4c66ceb7b81f6a1965c9a4a065af4c95",
  },
  {
    name: "Aggregator", role: "operator", account: OPERATOR, status: "live",
    does: "natural-language demand → spec (Gemini) → bin-packing → RFQ → open_lote",
    authority: "open_lote; the LLM ONLY normalizes — clearing is deterministic (INV-2)",
    lastAction: "8 NL demands → batch #5 opened",
    lastTx: "ad202f9d9323cbe8be88f6ec92a202d23cfc1c95cd4b6f3c0d25ba5793add2f1",
  },
  {
    name: "Mutual / Risk", role: "observer", status: "live",
    does: "solvency report + premium proposal to governance (real on-chain numbers)",
    authority: "read-only observer — the premium is changed by the admin, not the model",
    lastAction: "reserve 0.1 CSPR · ratio 10 · SOLVENT",
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
    "bond ≥ target is enforced in lock_lote → whoever fails pays first, from their own bond; " +
    "the mutual is a tail-of-loss backstop that so far has never been touched (tail = 0).",
} as const;
