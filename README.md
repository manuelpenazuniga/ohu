# Ohu — Fase 0 (S0 + S1)

> Ohu = agentic cooperative procurement + parametric mutual en Casper Testnet.
> Este repositorio es la **Fase 0 (de-risk)**. S0 entregó el esqueleto
> reproducible; S1 añade `OhuVault`, el ladrillo base de custodia en `purse`.

## Layout

```
ohu/
├── contracts/       # Odra (Rust) — OhuVault (S1)
├── agents/          # TypeScript — Agregador, Tesorería, Mutual
├── web/             # TypeScript — dashboard (Fase 3)
├── infra/           # deploy scripts, .env.example, justfile/Makefile
├── docs/plan/       # plan de implementación
└── .github/workflows/  # CI
```

## Toolchain pineada

| Componente | Versión fijada |
|:-----------|:---------------|
| Rust toolchain | `nightly-2026-01-01` (requerido por Odra 2.8.2) |
| cargo-odra | `0.1.7` |
| odra / odra-test / odra-build | `2.8.2` |
| Node.js | `>= 20` (CI usa 22) |
| pnpm | `11.8.0` |
| casper-js-sdk | `5.0.12` |
| TypeScript | `6.0.3` |
| vitest | `4.1.9` |
| just | `1.40.0` |

## Setup

```bash
# 1. Rust + wasm32 target + cargo-odra
rustup target add wasm32-unknown-unknown
cargo install cargo-odra --version 0.1.7 --locked

# 2. Node / pnpm (asegúrate de usar Node 20+)
corepack enable   # expone pnpm si tu Node lo soporta
pnpm install

# 3. Configuración de despliegue (solo para `just deploy`)
cp infra/.env.example .env
# Edita .env con tus claves y endpoints de Testnet.
```

## Build & test

```bash
just build      # compila contratos (host) + TS
just test       # cargo odra test + tests TS
just lint       # clippy + type-check
```

Para generar los artefactos WASM optimizados (requiere `wasm-opt` de
Binaryen):

```bash
just build-wasm
```

El Makefile es un fallback si `just` no está instalado.

## Contratos

### `OhuVault` (`contracts/src/ohu_vault.rs`)

Custodia en `purse` del contrato (sin Addressable Entity):

- `deposit()` — recibe CSPR en el purse del contrato (`#[odra(payable)]`).
- `withdraw_to(recipient, amount)` — **temporalmente abierto** en S1 para el
  test E2E; S2 lo cerrará tras `caller == admin` + aprobación M-de-N.
- `balance()` — saldo actual del purse.
- Emite eventos `Deposit` y `Withdraw` (visibles en CSPR.cloud).

> Ver los `TODO(S2)` y `TODO(audit)` en el código para los próximos pasos.

## Deploy a Testnet

```bash
just deploy
```

Requiere `.env` configurado. El script de deploy despliega `OhuVault`; la
configuración de cuentas admin/agente para S2 está documentada en
`infra/scripts/deploy.sh`.

## CI

GitHub Actions corre:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo odra test`
- TypeScript type-check + tests

## Notas de seguridad / auditoría

- Ningún secreto se commitea: `.env` está en `.gitignore`.
- S1 mueve capital real en testnet: `OhuVault` custodia CSPR en su `purse`.
- `withdraw_to` está abierto temporalmente; S2 añade el gating de admin+M-de-N.
- Los invariantes INV-1…INV-6 se aplican desde S1 en adelante.

## S4 — Riel B (x402)

`agents/src/x402/` monta un cobro **x402 real** sobre Testnet (oráculo de
reputación pago-por-request) con `@make-software/casper-x402`. **x402 no es el
rail de settlement de escrow** (INV-4); ese vive en el contrato `OhuVault` +
atestaciones on-chain. Demo y detalles en [`agents/docs-x402.md`](agents/docs-x402.md);
targets `just x402-facilitator`, `just x402-resource`, `just x402-pay`.
