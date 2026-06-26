# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repo is

A **specification / planning repository** (Spanish-language) for **Ohu** — the project being built for the **Casper Agentic Buildathon 2026**. There is **no application code yet**: no build system, no tests, no `package.json`/`Cargo.toml`. The deliverable so far is the product design and its technical due diligence. When implementation starts it will be **Odra (Rust) smart contracts on Casper testnet** plus off-chain agents; add build/test commands here when that scaffold lands.

> The project was renamed from **"Mancomún" → "Ohu"** (Māori: a communal work party). The core contract is named **`OhuVault`**. If you find any lingering `Mancomún`/`MancomunVault`, it's a stray — normalize it.

## The two canonical documents (and how they relate)

- **`ohu.md`** — the deep product spec and **source of truth for the product**: the problem, the batch state machine, the anti-fraud mutual design, contract architecture, the three agents, unit economics, demo script. Read this first to understand *what* is being built.
- **`techs-specs.md`** — the **technical due diligence that governs `ohu.md`**. It is authoritative on *what is buildable today* and lists concrete corrections applied to the product (`ohu.md` is already "v2" after these). When the product spec and the tech spec appear to conflict, **`techs-specs.md` wins on feasibility**; update `ohu.md` to match rather than the reverse.

`docs/brainstorming/` holds the ideation history (6-idea shortlist, model-vs-model brainstorms, the hackathon brief, the semi-final diff). It is **git-ignored** and not maintained — use it for background only, don't treat it as current truth.

## Architecture the product describes (big picture)

Ohu = **agentic cooperative procurement + a parametric mutual** on Casper. Small buyers (restaurants) pool weekly demand; small producers bid and post a performance bond; delivery is confirmed by **multi-party gasless attestations**; settlement and indemnification are **arithmetic over weighted attestations**, never human claims adjustment.

- **Four Odra contracts:** `OhuVault` (per-batch escrow + producer bonds in a `purse`), `MutualPool` (premiums + tail-of-loss indemnity), `Reputation` (on-chain score, also exposed as an x402 service), `CoopRegistry` (membership + governance params).
- **Three agents, each with its own Casper account (on-chain identity):** Agregador (normalizes demand, forms batches, runs RFQ clearing), Tesorería (triggers `release_to_producer`/`settle_failure`), Mutual/Riesgo (premiums, bond slashing, indemnity). **The LLM orchestrates; the contract authorizes — no capital moves on model judgment.**
- **Two value rails:** Rail A = escrow settlement via the contract + **EIP-712** gasless authorizations (this is a contract transfer, *not* x402). Rail B = genuine **x402** for the reputation/demand oracle sold per-request and for agents consuming external services.

## Hard constraints (these are non-negotiable design rules — see `casper-hackathon-quality-bar` memory + `techs-specs.md`)

1. **Do not depend on Casper's Addressable Entity / "contract-as-account" model** — it is *not activated* on mainnet or testnet. Custody = contract `purse` + **native account-level multisig** + **M-of-N approval in the contract**. The agent has its own account and may only call **capped** entry points; it is never a sub-key of the contract.
2. **x402 is an HTTP-resource payment protocol, not a universal transfer rail.** Don't describe escrow releases as "x402 micropayments." Keep x402 to the oracle-as-a-service and agent service consumption.
3. **Never put unreleased/immature tech on the critical path** — build only on what is live on testnet *today* (Odra 1.0+, casper-eip-712, native associated keys, x402 via `make-software/casper-x402` with a local-facilitator fallback, CSPR.cloud). Verify status before assuming.
4. **Closed-circuit data only** (no external price/oracle as truth); **parametric settlement, never appraisal**; value ≥ operational complexity; agents only do what agents do well.

## Conventions

- Documents are in **Spanish**; keep new docs in Spanish to match.
- Dates are absolute (`2026-06-25`); the spec is versioned in-document (currently **v2**) — bump the version line and note the change when you make a substantive edit.
