# Ohu — agentic cooperative procurement + a parametric mutual, on Casper

> Small buyers pool weekly demand; small producers bid and post a performance bond; delivery is
> confirmed by **gasless multi-party attestations**; settlement and indemnification are **arithmetic
> over weighted attestations, never human claims adjustment**. An LLM swarm orchestrates — **the
> contract authorizes. No capital ever moves on a model's judgment.**

![Odra tests](https://img.shields.io/badge/odra%20tests-206%20passing-brightgreen)
![Agent tests](https://img.shields.io/badge/agent%20tests-24%20passing-brightgreen)
![Network](https://img.shields.io/badge/live-Casper%20Testnet-blue)
![License](https://img.shields.io/badge/license-MIT-lightgrey)

Built for the **Casper Agentic Buildathon 2026**. Everything below is built on what is **live on
Casper Testnet today** — no dependency on unreleased tech.

---

## The problem

Small restaurants and producers who pool purchasing already exist (buying clubs, WhatsApp groups) and
die at the same three points — exactly the three Ohu puts on-chain, and *only* those:

1. **Who holds the money?** A human coordinator with the group's bank account is counterparty risk no
   small business accepts at scale. → escrow in a contract `purse`, **earmarked per batch**.
2. **What happens when the order doesn't arrive?** On a $4,000 batch, a claims adjuster or a lawsuit
   costs more than the batch. Nobody insures this — so it doesn't exist. → **parametric** settlement
   over weighted multi-party attestations: arithmetic, not adjudication.
3. **Why does the producer get paid in 30–60 days?** Because nobody guarantees instant payment against
   verified delivery. → vault settlement in seconds once the on-chain threshold is crossed.

Everything else (batch formation, RFQ, communication) stays **off-chain**, where it's cheap. The cannon
points only at the fly that is actually a tank: **enforcement between distrusting small parties who
can't afford traditional enforcement.**

---

## How it works

```
   buyers                producer            operator (agent)         admin (native multisig)
     │ deposit (share)     │ post_bond           │ evaluate_lote          │ release / settle
     ▼                     ▼                     ▼                        ▼
 ┌─────────────────────────────────────────────────────────────────────────────────┐
 │  OhuVault (Odra contract, purse escrow, earmarked per batch — INV-7)             │
 │    open_lote → deposit_to_lote → post_bond → lock_lote ─────────────┐            │
 │                                                                      ▼            │
 │  attestation window: buyers sign gasless (Ed25519), operator relays │            │
 │    verify_attestation  →  weighted tally (by share)                 │            │
 │                                                                      ▼            │
 │  evaluate_lote  →  EVAL_OK (silence = received)  or  EVAL_FAIL (≥ quorum negative)│
 │       │                                              │                            │
 │       ▼ release_to_producer                          ▼ settle_failure             │
 │   SETTLED_OK (producer paid − premium)      SETTLED_FAIL (refund + bond slashed)  │
 │       │ premium 0.5%                                  │ tail (deficit)             │
 │       ▼                                               ▼                            │
 │                         MutualPool (premiums in, tail-of-loss backstop)           │
 └─────────────────────────────────────────────────────────────────────────────────┘
```

**Batch state machine:**

```
OPEN ──lock_lote(operator/admin)──▶ FUNDED ──evaluate_lote(after window)──┬─▶ EVAL_OK ──release──▶ SETTLED_OK
                                                                          └─▶ EVAL_FAIL ─settle──▶ SETTLED_FAIL ──withdraw──▶ buyers
```

- **Attestations are gasless.** A buyer signs off-chain (Ed25519 + domain separation over
  `verifying_contract` + `chain_id` + `valid_before`) and the operator relays it on-chain, paying gas.
  The buyer never needs CSPR. *(This is the pre-agreed, on-chain-verified signature scheme;* ***EIP-712
  typed-data is on the roadmap***, *not yet implemented — see Honest scope.)*
- **Silence = received.** Not attesting counts as "delivered fine." Only an active, weighted negative
  attestation opens the claim path. This reflects reality and eliminates griefing-by-inaction.
- **The producer's bond is the primary payer of a failure.** It must cover the indemnity target
  (`bond ≥ target`, enforced at `lock_lote`). The mutual is a tail backstop — the one who failed pays
  first.

---

## Why an agent can't rug you

The whole point of an *agentic* system that moves money: a jailbroken LLM must not be able to touch
capital. Ohu enforces this on-chain, not by trust. The invariants:

| # | Invariant | How it's enforced |
|---|---|---|
| **INV-1** | The agent never moves relevant capital | The agent's account can only call **capped** entrypoints (bounded micropayments). Every real release requires `caller == admin` + native account multisig |
| **INV-2** | No capital release depends on the LLM's output | Settlement is authorized by an **on-chain condition** — the weighted attestation tally, not a human, not the M-of-N of a person |
| **INV-3** | No Casper Addressable Entity | Custody = contract `purse` + **native account associated-keys/thresholds** + in-contract M-of-N. All live on Testnet today |
| **INV-4** | x402 is only for HTTP services | Escrow settlement is a **contract transfer**, never an x402 flow |
| **INV-5** | Attestations are signed off-chain, verified on-chain (gasless) | Ed25519 + domain separation, anti-replay per `(lote, signer)`, expiry |
| **INV-6** | Closed-circuit data; settlement is arithmetic | No external price/oracle as truth; tally over weighted attestations |
| **INV-7** | Escrow is earmarked per batch | `reserved_lote_balance` — a batch's funds go only to its producer or back to its buyers, never across batches |

> **The LLM orchestrates; the contract authorizes.** This is the answer to the buildathon's core
> question: *how do you let autonomous agents operate real money without a jailbreak ruining anyone?*

---

## Live on Casper Testnet

Deployed v2 (RPC: `https://node.testnet.casper.network`, chain `casper-test`):

| Contract | Package hash |
|---|---|
| **OhuVault v2** | [`hash-94c4d7b466a035e0aac9bb60daeaa179432ad2df93de3dfe2759812676bf3b6c`](https://testnet.cspr.live/contract-package/hash-94c4d7b466a035e0aac9bb60daeaa179432ad2df93de3dfe2759812676bf3b6c) |
| **MutualPool** | [`hash-2cbbd92b6b3b6ef3629da0330e7b63213a8a04c03b3721b0dbc2a2d73f685cb0`](https://testnet.cspr.live/contract-package/hash-2cbbd92b6b3b6ef3629da0330e7b63213a8a04c03b3721b0dbc2a2d73f685cb0) |

Two full batch lifecycles were executed end-to-end on-chain:

- **Happy path (via tally):** `open → deposit → post_bond → lock_lote → [window, silence=received] →
  evaluate_lote = EVAL_OK → release_to_producer`. Producer paid `funded + bond − premium`; premium to
  the pool. Settlement authorized by the **tally**, not M-of-N.
- **Failure path (indemnifies by rule):** a buyer signs a **negative** attestation (Ed25519, gasless) →
  tally crosses quorum → `evaluate_lote = EVAL_FAIL → settle_failure` (bond slashed) →
  `withdraw_settlement` (buyer refunded + indemnified from the slashed bond). **No human evaluated a
  claim.**

All 14 transaction hashes are in [`infra/deployments/testnet.md`](infra/deployments/testnet.md).

---

## The agent swarm

Three agents, each with its own Casper account (on-chain identity). The split is deliberate:

| Agent | LLM does (well) | Deterministic / on-chain (authoritative) |
|---|---|---|
| **Agregador** | normalize fuzzy demand → structured spec; form batches; run RFQ dialogue; explain | RFQ clearing; spec validation |
| **Tesorería** | monitor windows; handle exceptions; narrate decisions | triggers `evaluate_lote` / `release_to_producer` / `settle_failure` — **all gated on-chain; if the agent lies, they revert** |
| **Mutual/Riesgo** | draft solvency reports; propose premium changes to governance | collect premium; slash bond; watch reserve |

> **Honest status:** the three agents are on the **roadmap** (next milestone). What exists today is the
> **contract layer** (fully deployed + exercised on Testnet) and the **x402 rail**. The security
> guarantee they rely on (INV-1/INV-2) is *already enforced on-chain*, independent of any agent.

---

## x402: the reputation oracle (Rail B)

A genuine [x402](https://www.casper.network/ai) pay-per-request service: producers' reputation sold
per HTTP request, using `@make-software/casper-x402` with a failover facilitator (hosted → local).
The server declares its **non-escrow semantics** at `/health` (INV-4): x402 charges for an HTTP
service, it *never* settles escrow. 24 tests cover the 402-shape, failover, idempotent settle, and the
non-escrow invariant. The oracle now derives each producer's score from **real on-chain settlement
history** (via CSPR.cloud: `open_lote` → producer, `release_to_producer` = OK, `settle_failure` = FAIL),
with a seed fallback only when no CSPR.cloud key is configured. Verified live: producer `33518b62…`
returns `4 lotes · 2 OK · 1 FAIL · score 73` `asOfBlock 8425485`.

---

## Quickstart

```bash
just setup     # Rust + wasm32 + cargo-odra, Node/pnpm
just build     # build contracts + agents
just test      # 206 Odra tests + 24 agent tests + 1 web
just lint      # clippy (-D warnings) + typecheck
```

Deploy + run the on-chain batch lifecycle (requires `casper-client`, `binaryen`/`wasm-opt`, a funded
Testnet account, and a `.env` — see `infra/.env.example`):

```bash
bash infra/scripts/deploy_testnet.sh                       # deploy OhuVault v2 + MutualPool (MVP-lowered wasm)
cargo run --bin ohu_livenet_e2e --features livenet         # happy path E2E
cargo run --bin ohu_livenet_e2e_fail --features livenet    # failure path E2E (indemnifies by rule)
```

**Repo layout:** `contracts/` (Odra/Rust — `OhuVault`, `MutualPool`, `attestation`) ·
`agents/` (TypeScript — x402 rail) · `web/` (dashboard, roadmap) · `infra/` (deploy, deployment
records) · `docs/` (product spec `ohu.md`, tech due-diligence `techs-specs.md`, state `docs/ESTADO.md`).

---

## Honest scope

What is **100% real** (deployed + exercised on Testnet): the contracts, the earmarked purse escrow, the
weighted-attestation parametric settlement (both happy and failure paths), the native account multisig
model, and the x402 oracle rail.

**Honestly bounded / on the roadmap:**

- **Attestations are Ed25519 + domain separation** (gasless, verified on-chain) — the pre-agreed scheme
  from the tech due-diligence. **EIP-712 typed-data is on the roadmap**, not yet implemented. (The x402
  rail *does* use real EIP-712 for its payment authorizations — that's a separate, correct thing.)
- **The 3 agents and the dashboard are on the roadmap.** The contract layer that makes them safe is
  already live.
- **`Reputation` and `CoopRegistry` contracts are on the roadmap.** Governance params currently live in
  `OhuVault::init`.
- The demo runs a small seeded panel of buyers/producers; delivery is represented by signed
  attestations — which is the *real* mechanism, not a shortcut.

---

## Roadmap

- **Live now:** the contract layer + on-chain E2E, the **Tesorería/Autorizador agents** (a batch settled
  hands-free on Testnet), the **Swarm Control Room dashboard**, and the **reputation oracle over real
  on-chain history** (CSPR.cloud).
- **Next:** the **Agregador** agent (natural-language demand → on-chain batch, LLM-normalized) and the
  **Mutual/Riesgo** agent; QR gasless mobile attestation.
- **Later:** EIP-712 typed-data attestations; `Reputation`/`CoopRegistry` contracts; Ohu as an **MCP
  server** (a market for other agents).

<!-- TODO(human): demo video link · DoraHacks BUIDL page · X / Telegram -->

---

Docs: [`ohu.md`](ohu.md) (product) · [`techs-specs.md`](techs-specs.md) (feasibility due-diligence) ·
[`docs/ESTADO.md`](docs/ESTADO.md) (build state). License: MIT.
