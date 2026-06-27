# Ohu — Fase 0 (S0 + S1 + S2)

> Ohu = agentic cooperative procurement + parametric mutual en Casper Testnet.
> Este repositorio es la **Fase 0 (de-risk)**. S0 entregó el esqueleto
> reproducible; S1 añadió `OhuVault`, el ladrillo base de custodia en `purse`;
> **S2 añade el modelo de seguridad "el agente no drena" (INV-1)**: cuenta
> admin (multisig nativo) + cuenta operator con entrypoint capado + releases
> grandes gateados por `caller==admin` + aprobación M-de-N on-chain.

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

Custodia en `purse` del contrato (sin Addressable Entity, INV-3) con el modelo
de seguridad de S2 (INV-1). Tres roles on-chain, todos **cuentas**:

- **`admin`** — ejecuta los releases grandes (`execute`; `caller == admin`).
  En Testnet es además un multisig nativo (associated keys + deployment
  threshold alto) → co-firma off-chain para siquiera submitir `execute`.
- **`operator`** (agente/LLM) — solo puede llamar `route_micropayment`, con
  **tope por llamada** (`micropayment_cap`). No puede proponer, aprobar ni
  ejecutar.
- **`approvers`** — firmantes M-de-N que `approve(id)` un release.

Entry points:

- `init(admin, operator, approvers, required_approvals, micropayment_cap)` —
  valida el setup (admin≠operator, approvers distintos y no contienen al
  operator, `required_approvals ∈ [1, len(approvers)]`, cap>0).
- `deposit()` — `#[odra(payable)]`; cualquiera puede fondear el vault.
- `route_micropayment(recipient, amount)` — **capado**: solo `operator`,
  `0 < amount ≤ micropayment_cap`. Emite `MicropaymentRouted`.
- `propose_withdraw(recipient, amount) -> u64` — solo admin o approvers (no el
  operator). No mueve capital. Emite `WithdrawProposed`.
- `approve(request_id)` — solo approvers; un approver no aprueba dos veces la
  misma solicitud (`AlreadyApproved`), garantizando aprobaciones **distintas**.
  Emite `WithdrawApproved`.
- `execute(request_id)` — **doble gate**: `caller == admin` **+**
  `approval_count ≥ required_approvals`. Aplica checks-effects-interactions
  (marca `request_executed` antes de la transferencia). Emite `WithdrawExecuted`.
- Getters: `balance`, `admin`, `operator`, `micropayment_cap`,
  `required_approvals`, `is_approver`, `approval_count`, `request_executed`,
  `request_recipient`, `request_amount`.

Eventos (`Deposit`, `MicropaymentRouted`, `WithdrawProposed`, `WithdrawApproved`,
`WithdrawExecuted`) visibles en CSPR.cloud.

> Defensa en profundidad: (1) multisig nativo del admin (off-chain) fuerza
> co-firma para deployar; (2) el contrato exige M aprobaciones **distintas**
> on-chain. Sobrevive aunque el admin sea una clave única.
> Ver los `TODO(audit)` en el código para los próximos pasos (S3: atestaciones
> EIP-712; purse secundario; indexación de eventos en CSPR.cloud).

## Deploy a Testnet

```bash
# 1. (Recomendado) configura la cuenta admin como multisig nativo
#    (associated keys + weights + deployment threshold). Plan + recipe en:
bash infra/scripts/setup_admin_account.sh

# 2. Deploya OhuVault con los init args de S2
just deploy
```

Requiere `.env` configurado. El deploy de `OhuVault` pasa los init args
(`admin`, `operator`, `approvers`, `required_approvals`, `micropayment_cap`)
definidos en `.env`; un setup inválido revierte on-chain con `Error::InvalidSetup`.

## CI

GitHub Actions corre:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo odra test`
- TypeScript type-check + tests

## Notas de seguridad / auditoría

- Ningún secreto se commitea: `.env` está en `.gitignore`.
- S1/S2 mueven capital real en testnet: `OhuVault` custodia CSPR en su `purse`.
- **S2 / INV-1**: el `operator` (agente) solo puede llamar `route_micropayment`
  (tope por llamada); los releases grandes exigen `caller == admin` + M
  aprobaciones **distintas** on-chain, y la cuenta `admin` es a su vez un
  multisig nativo (co-firma off-chain). Tests negativos en
  `contracts/src/ohu_vault.rs` prueban que el agente **no** puede drenar.
- Los invariantes INV-1…INV-6 se aplican desde S1 en adelante.

## S4 — Riel B (x402)

`agents/src/x402/` monta un cobro **x402 real** sobre Testnet (oráculo de
reputación pago-por-request) con `@make-software/casper-x402`. **x402 no es el
rail de settlement de escrow** (INV-4); ese vive en el contrato `OhuVault` +
atestaciones on-chain. Demo y detalles en [`agents/docs-x402.md`](agents/docs-x402.md);
targets `just x402-facilitator`, `just x402-resource`, `just x402-pay`.
