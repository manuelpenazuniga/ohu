# Despliegues — Casper Testnet (`casper-test`)

Registro de despliegues reales de Ohu. Valores públicos (account-hashes, contract
hashes, tx). Las claves privadas viven fuera del repo (`~/.casper-keys/`).

## OhuVault — v1 (W1-3, 2026-06-30)

| Campo | Valor |
|---|---|
| **Contract package** | `hash-6c1a13664c1070035cde62fa927255d649fcb929a5f5cf823d031c314c80d47f` |
| **Contract hash (v1)** | `contract-833696c886b9844046fc3978fe832336ef3fb3b4572eded530090ba6846350c0` |
| **Deploy tx** | `b595b892e61d9fce4197d151ecca7685a16dddd260f43e3778ffeeeaec3f97c9` |
| **Nodo RPC** | `https://node.testnet.casper.network/rpc` (API 2.0.0) |
| **Eventos SSE** | `https://node.testnet.casper.network/events` |
| **Deployer** | cuenta del usuario (secp256k1), fondeada vía faucet |
| **WASM** | 415 KB, MVP-limpio (bulk-memory + sign-ext lowered con wasm-opt) |

**Explorer:** https://testnet.cspr.live/contract-package/hash-6c1a13664c1070035cde62fa927255d649fcb929a5f5cf823d031c314c80d47f

### Init args (las identidades son cuentas generadas, no el deployer)

| Arg | Valor |
|---|---|
| `admin` | `account-hash-59d06759666ef90a065d023c4c2b6a77708c38945380a0b36380f07e71bd70b4` |
| `operator` | `account-hash-9c28ba3e5c1154fa23085326c9e165de79a32a67b1145edce5e0a2b949f80186` |
| `approvers` | `account-hash-763cd35e3124e8a4f871277300ab395687829c9ffacffa7fc166c6e096dbecfe`, `account-hash-5719fa9d691382075f57827094d32e1667dd01a2dd9c1946d684fe7f53ec2ff8` |
| `required_approvals` | 2 |
| `micropayment_cap` | 1_000_000_000 motes (1 CSPR) |
| `epoch_cap` | 5_000_000_000 motes (5 CSPR) |
| `epoch_window_ms` | 3_600_000 (1 h) |
| `chain_id` | 1 |

**Reproducir:** `bash infra/scripts/deploy_testnet.sh` (requiere binaryen + .env + deployer fondeado).

### Pendiente (Fase 2)
Lote feliz E2E: repartir gas a las 5 cuentas → `open_lote` → `deposit_to_lote` →
`post_bond` (FUNDED) → `propose_release` → `approve_release` ×2 (M-de-N) →
`release_to_producer` (SETTLED_OK), visible on-chain.
