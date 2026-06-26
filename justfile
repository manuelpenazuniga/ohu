set dotenv-load := true

_default:
    @just --list

# Install pinned toolchain dependencies (run once per clean clone).
setup:
    rustup target add wasm32-unknown-unknown
    rustup target add wasm32-unknown-unknown --toolchain nightly-2026-01-01
    cargo install cargo-odra --version 0.1.7 --locked
    corepack enable 2>/dev/null || true
    pnpm install

# Build all packages (contracts host compile + TypeScript).
build:
    @echo "==> Building Odra contracts"
    cd contracts && cargo build --release
    @echo "==> Building TypeScript packages"
    pnpm install --frozen-lockfile
    pnpm run build

# Build optimized WASM artifacts (requires wasm-opt / binaryen).
build-wasm:
    @echo "==> Building Odra WASM artifacts"
    cd contracts && cargo odra build

# Run all tests (Odra VM + TypeScript).
test:
    @echo "==> Testing Odra contracts"
    cd contracts && cargo odra test
    @echo "==> Testing TypeScript packages"
    pnpm install --frozen-lockfile
    pnpm run test

# Lint / type-check all packages.
lint:
    @echo "==> Clippy on contracts"
    cd contracts && cargo clippy --all-targets --all-features -- -D warnings
    @echo "==> TypeScript type-check"
    pnpm install --frozen-lockfile
    pnpm run typecheck

# Deploy to Casper Testnet (requires `.env` filled).
deploy:
    @echo "==> Deploying to Casper Testnet"
    bash infra/scripts/deploy.sh

# =============================================================================
# S4 — Riel B (x402): oráculo de reputación pago-por-request.
# Recordatorio INV-4: x402 cobra servicios HTTP; NO es el rail de settlement
# de escrow (ese vive en el contrato OhuVault + atestaciones on-chain).
# =============================================================================

# Arranca el facilitator LOCAL (fallback) que firma deploys contra Testnet.
x402-facilitator:
    pnpm --filter @ohu/agents run serve:facilitator

# Arranca el servidor de recursos (oráculo de reputación) protegido por x402.
x402-resource:
    pnpm --filter @ohu/agents run serve:resource

# Cliente pagador: paga por un request y recibe el recurso de reputación.
x402-pay:
    pnpm --filter @ohu/agents run pay:get

# Demo live completa contra Testnet:
#   Terminal 1: just x402-facilitator   (inicia primero)
#   Terminal 2: just x402-resource
#   Terminal 3: just x402-pay
# Requiere `.env` con ASSET_PACKAGE, PAYEE_ADDRESS, FACILITATOR_PEM_PATH y
# CLIENT_PRIVATE_KEY_PATH (cuentas Ed25519 fondeadas en Testnet) y un token
# CEP-18 desplegado. El settle se ve on-chain (transfer_with_authorization) en
# CSPR.cloud. Esto NO mueve fondos del OhuVault (escrow).
