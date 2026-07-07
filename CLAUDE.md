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

## Design system — "El Almanaque" (web/)

Identidad visual de todo lo web (landing, onboarding, dashboard): **almanaque agrícola + libro de cuentas de cooperativa**, con acentos de **pixel art de huerto**. Papel, tinta y sellos de goma — no SaaS oscuro. Estos tokens tienen prioridad sobre cualquier default.

### Tokens

- **Paleta día (papel):** fondo `#F6F1E3` (papel crema), panel `#FDFAF1`, tinta `#26221A` (negro cálido), líneas `#D8CDB4`, texto secundario `#6B5F49`.
- **Colores vivos (máx. 3 en pantalla):** verde huerto `#3D6B35` (primario: marca, acciones, estados sanos), teja `#C65530` (sellos/attestations, alertas, CTA secundario), trigo `#E0A32E` (cosecha, éxito, recompensas).
- **Paleta noche (campo de noche):** fondo `#171512`, panel `#201D18`, tinta `#EDE6D6`, líneas `#3A342A`; los vivos se aclaran: verde `#8FBF6B`, teja `#E07A50`, trigo `#E8B44A`.
- **Tipografía:** display **Fraunces** (títulos, cifras hero — usar sus ejes ópticos/wonk); cuerpo **Instrument Sans**; datos on-chain (hashes, motes, direcciones) **IBM Plex Mono**; **Silkscreen** (píxel) solo en micro-etiquetas lúdicas ≤ 12px (badges, logros del onboarding) — nunca en párrafos ni datos.
- **Formas:** bordes 2px sólidos (tinta o línea), radius 6px. Nada de sombras difusas: los elementos interactivos usan sombra dura desplazada (`3px 3px 0` color tinta), estética de imprenta/grabado. Espaciado en escala de 8px.

### Motivos (usar el vocabulario del producto, no metáforas genéricas)

- **Attestations = sellos de goma:** círculo de borde dentado, tinta teja, rotación sutil (−3°…3°), fecha dentro. Un batch liquidado lleva sus sellos estampados.
- **Ciclo del batch = ciclo de cultivo:** Semilla (formación) → Brote (financiado) → Floración (en reparto) → Cosecha (entregado) → Granero (liquidado). Cada estado tiene sprite pixel art 24×24.
- **Progreso = surco que se planta** (huecos que se van llenando de brotes), no barras de progreso genéricas. Cantidades/lotes como etiquetas de caja de cosecha.

### Pixel art — reglas de uso

- Solo como **acento**: sprites de 16/24/32px escalados ×2–×3 con `image-rendering: pixelated`, paleta limitada a los tokens (≤ 8 colores), sin anti-aliasing.
- **Dónde sí:** hero de la landing (escena de huerto), onboarding (mascota + logros), iconos del ciclo del batch, estados vacíos.
- **Dónde no:** jamás UI completa en píxel, jamás en datos financieros/on-chain (ahí mandan el mono y los sellos).

### Prohibiciones

Nada de: gradientes morado/teal, glassmorphism, Inter/system-ui como display, emojis como iconografía, cards flotantes con sombra suave, dark-mode-por-defecto. **La UI es en inglés** (audiencia del buildathon), con léxico cooperativo/agrícola: crew, harvest, barn, ledger, y "ohu" como nombre propio ("Join the ohu").

Los prompts de generación de assets (Nano Banana / Stitch) viven en `docs/design/prompts-arte.md`.

## Conventions

- Documents are in **Spanish**; keep new docs in Spanish to match.
- Dates are absolute (`2026-06-25`); the spec is versioned in-document (currently **v2**) — bump the version line and note the change when you make a substantive edit.
