# Despliegues — Casper Testnet (`casper-test`)

Registro de despliegues reales de Ohu. Valores públicos (account-hashes, contract
hashes, tx). Las claves privadas viven fuera del repo (`~/.casper-keys/`).

## v2 (W2-4, 2026-07-06) — Semana 2 completa on-chain ✅

Lo mejor del código (tally paramétrico W2-1 + `MutualPool` W2-3 + `lock_lote`) desplegado
y ejercitado con un **lote fallido E2E que indemniza por regla**. Nodo:
`https://node.testnet.casper.network/rpc`.

| Contrato | Package hash | Deploy tx |
|---|---|---|
| **OhuVault v2** | `hash-94c4d7b466a035e0aac9bb60daeaa179432ad2df93de3dfe2759812676bf3b6c` | `ba99bb14f2c91ff38baa6bb8e1406e6c739a27b864798de71400ceb7a4f9c5d9` |
| **MutualPool** | `hash-2cbbd92b6b3b6ef3629da0330e7b63213a8a04c03b3721b0dbc2a2d73f685cb0` | `92b7578a42725f7fb23d7f9f3426788c444ea2a7b58a39c6f3a736ac1c68e26c` |

Config post-deploy (setters, firmados por admin): `set_mutual_pool` `23abe835…`,
`set_premium_bps(50)` `98a969d3…`, `set_indemnity_target_bps(8000)` `ff79613b…`.

### E2E del LOTE FALLIDO (hito Semana 2: "un lote que falla indemniza por regla")

Comprador = approver0 (Ed25519, atesta gasless). Producer = cuenta no-privilegiada.
`quorum_fail_bps=6000`, `attestation_window_ms=60000`, `indemnity_target_bps=8000`.

| # | Paso | Firmante / relayer | Tx |
|---|---|---|---|
| 1 | `open_lote(1, producer)` | admin | `74ca3193a018ebce629eeb4becc88040a974179e5c99004af8bb85f2beca8672` |
| 2 | `deposit_to_lote(1)` +10 CSPR | buyer (approver0) | `b413125a006818b8d6f9d5ba9d8215cb9b87f157d3989d25327b62db477ed320` |
| 3 | `post_bond(1)` +10 CSPR | producer | `cbbb0acb92f8940f3c1f391d018a06bdd3d962a3715d8f7b52161c8505cc42f3` |
| 4 | `lock_lote(1)` → FUNDED | admin | `296bd5513b793630a035ca4f69c618970b7cb9ba024356602a3f0c3f7573be94` |
| 5 | `verify_attestation(1, recibido=false)` — Ed25519 gasless, tally 100% | buyer firma / admin relaya | `a4d537e920951709e786f7d1f9f018f228b4dd34e1f7b8296a37d72dca5f9d2b` |
| 6 | `evaluate_lote(1)` → EVAL_FAIL (tally ≥ quórum) | admin | `12a1bb123f1a35804a69b46f2173b86da812ac65fa8ced2574fb45c906e7c9e5` |
| 7 | `settle_failure(1)` → SETTLED_FAIL (slash bono, indemnity=min(10,8)=8) | admin | `723d0e39a35df617ae96e572c74e9c055bf8b3ab0cfd8c6df75222c4be9be414` |
| 8 | `withdraw_settlement(1)` → refund 10 + indemnización 8 | buyer (approver0) | `12eb086fbd0d65d30a386ca0e32440246cd7f398e12220b4f65cef06456e8e2b` |

El settlement lo autorizó el **tally de atestaciones** (INV-2), no un humano ni M-de-N. La
indemnización salió del **bono slasheado** (§4.1, el que falla paga primero); el pool cobró la
prima y quedó como backstop (`tail=0` con `bond≥target`).

**Pendiente de P0-1:** re-correr el E2E FELIZ contra v2 (vía tally/silencio=recibido → EVAL_OK →
release) — el binario `livenet_e2e.rs` necesita actualizarse a `lock_lote` + `evaluate_lote`.

---

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

### Fase 2 — Lote feliz E2E (2026-06-30) ✅ COMPLETO ON-CHAIN

Ciclo completo de un lote, cada paso firmado por la cuenta nativa del rol
(`bin/livenet_e2e.rs`, multi-key via `set_caller`/`ODRA_CASPER_LIVENET_KEY_*`).
Producer fresco: `account-hash-33518b62a4434cb640d6239c86e86f1ed1c132df9ddc2d1cf6f629913ad1f1ba`.

| # | Paso | Firmante | Tx |
|---|---|---|---|
| 1 | `open_lote(1, producer)` | admin | `6114c4bf28a1aaf01d4fbe58c9c9804aabe4238d9fe3c30f041dfa332eb9aba4` |
| 2 | `deposit_to_lote(1)` +10 CSPR | buyer (deployer) | `3c4daa6ed760865ed9fdcee3775240e70b05f3b0c7bd0b4041f5776559fc1d31` |
| 3 | `post_bond(1)` +5 CSPR → FUNDED | producer | `bfd8a06c768ee23ebf3c0bb86650cae85a36ad3779e5627877a2f7aa68594880` |
| 4 | `propose_release(1)` | approver0 | `0a5ddb8d2fa44ec8a469a48dc45309bb738542bd93f773a1efad9474174d62a7` |
| 5a | `approve_release(1)` | approver0 | `80f617bb4bfd4c84aa91f2037b556392fde4e915797d4b822db4afcf12d4f767` |
| 5b | `approve_release(1)` | approver1 | `40553b0dfada526231ba58c08190026f5c35ce180c6535d8971611f4e8804cdf` |
| 6 | `release_to_producer(1)` → SETTLED_OK | admin | `70037193177fb33fce4e5f0034fb0d0c12ddd73ab530d0fab1811d0ad1882575` |

Escrow liberado al producer = `funded + bond` = 15 CSPR, solo tras `caller==admin ∧
approvals≥required(2)`. Multisig M-de-N nativo validado contra el nodo real.

**Reproducir E2E:** fondear gas a las cuentas (admin/approvers/producer) y
`cargo run --bin ohu_livenet_e2e --features livenet` (con `.env` sourceado).
