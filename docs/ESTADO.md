# Ohu — Estado del proyecto y planificación

> **Qué es este documento:** contexto de arranque para Claude (y colaboradores). Resume **qué es
> Ohu**, **qué está construido hoy**, **cómo trabajamos**, y **el roadmap**. Léelo primero; luego
> profundiza en los documentos enlazados.
>
> **Última actualización:** 2026-06-29 · rama de trabajo `fase-0` @ `a4ce714` · **Fase 0 CERRADA**,
> **Semana 1 planeada (sin empezar)**.

---

## 0. TL;DR

- **Ohu** = procura cooperativa agéntica + mutual paramétrica sobre **Casper** (Buildathon 2026).
- **Fase 0 (de-risk) COMPLETA y verde**: scaffold, custodia en `purse`, multisig "el agente no
  drena", x402, y atestación gasless verificada on-chain. **55 tests Odra + 24 TS + 1 web**, clippy
  y typecheck limpios.
- **Próximo:** **Semana 1 — núcleo de liquidación** (modelo de lote + settlement happy-path en
  Testnet). Planeada en `docs/plan/semana-1.md`, aún sin implementar.
- **Modo de trabajo:** **Claude planifica en detalle y audita**; **agentes opencode implementan**
  (no Anthropic — GLM-5.2, DeepSeek V4 Pro, Qwen3.7 Max, Kimi, MiniMax, etc.). Ver §5.

---

## 1. Qué es Ohu (producto)

Pequeños compradores (restaurantes) **agregan su demanda semanal**; productores chicos ofertan y
**depositan un bono de cumplimiento**; la entrega se confirma con **atestaciones gasless
multiparte (EIP-712/Ed25519)**; el settlement y la indemnización son **aritmética sobre
atestaciones ponderadas, nunca peritaje humano**. Un **enjambre de 3 agentes** orquesta; **el
contrato autoriza** — ningún capital se mueve por juicio de un LLM.

- **Fuente de verdad del producto:** **`ohu.md`** (problema, máquina de estados del lote, mutual
  anti-fraude, arquitectura, agentes, economía, guion del demo).
- **Due diligence técnica que gobierna la factibilidad:** **`techs-specs.md`** (manda sobre
  `ohu.md` en lo que es construible hoy).
- **Naming:** el proyecto se llamó "Mancomún" → renombrado a **Ohu** (māori: cuadrilla de trabajo
  comunal). Contrato núcleo = **`OhuVault`**.

> El nombre "Ohu" pasó una auditoría de marca (sin tilde, anglo-pronunciable, libre en
> fintech/food/cripto). Pendiente legal: dominio + registro de marca por clase.

---

## 2. Stack y layout (monorepo)

```
ohu/
├── contracts/        # Odra 2.8.2 (Rust) — OhuVault (+ módulo attestation)
├── agents/           # TypeScript — riel B x402 (casper-js-sdk, @make-software/casper-x402)
├── web/              # TypeScript — dashboard (placeholder; Fase 3)
├── infra/            # deploy Testnet, .env.example, justfile/Makefile, setup_admin_account.sh
└── docs/
    ├── plan/         # specs por fase (ver §8)
    ├── brainstorming/  # ideación (GIT-IGNORADA, no es verdad actual)
    └── ESTADO.md     # este documento
```
- **Toolchain:** Rust nightly-2026-01-01 + `cargo-odra` 0.1.7 + `casper-client`; Node 20+/pnpm
  11.8.0 + `casper-js-sdk`. Red objetivo: **Casper Testnet**.
- **Comandos:** `just build` · `just test` · `just lint` · `just deploy` · targets `x402-*`.
- **CI:** GitHub Actions (job Rust: fmt+clippy+`cargo odra test`; job TS: typecheck+vitest).

---

## 3. Estado del repositorio

- **Rama de trabajo:** `fase-0` (todo el trabajo vive aquí, **local, NO pusheado**).
- **`main`:** solo el commit inicial (`21cc72e`, spec + due diligence + CLAUDE.md) — **eso es lo
  único en GitHub** (`manuelpenazuniga/ohu`). ⚠️ Para reflejar el avance en el remoto hay que
  **pushear `fase-0`** (o mergear a `main`).
- **Verde verificado:** `cargo odra test` → **55/55**; `pnpm -r test` → agents **24/24**, web **1/1**;
  clippy `-D warnings` limpio; typecheck limpio.
- **Cuentas GitHub:** el push debe hacerse como **`manuelpenazuniga`** (no `fundacionrescatedemascotas`,
  que es la otra cuenta activa). La identidad de commit del repo está fijada localmente a
  `manuelpenazuniga` (noreply).

### 3.1 Qué existe en el contrato HOY (`OhuVault`)
Entrypoints (todos sin Addressable Entity; custodia en el `purse` del contrato):

| Entrypoint | Quién | Qué hace | Estado de seguridad |
|---|---|---|---|
| `init(admin, operator, approvers, required_approvals, micropayment_cap, epoch_cap, epoch_window_ms, chain_id)` | deployer | configura roles + topes + domain | valida roles∉contratos, admin∉approvers, operator∉approvers, ≤255 approvers, caps>0 |
| `deposit()` `payable` | cualquiera | fondea el purse | — |
| `route_micropayment(recipient, amount)` | **operator (agente)** | micropago **capado** | **doble tope: por-llamada + acumulado por epoch** (INV-1) · recipient debe ser cuenta |
| `propose_withdraw(recipient, amount)` | admin/approver | propone release grande | no mueve capital |
| `approve(id)` | approver | aprueba (firmantes **distintos**) | anti doble-aprobación |
| `execute(id)` | **admin** | ejecuta release | `caller==admin` **+ M-de-N** + CEI + anti doble-ejecución |
| `verify_attestation(lote_id, nonce, received, pk, sig)` | cualquiera (gasless) | verifica firma **Ed25519** on-chain + registra | anti-replay por `(lote,signer)` + domain separation (`verifyingContract`+`chain_id`). **⚠️ NO autorizada ni con expiry — ver §6** |

- Módulo `contracts/src/attestation.rs`: `verify_attestation_signature` (Ed25519 vía
  `casper_types::crypto::verify`), `build_attestation_message`. Typehashes EIP-712 documentados
  como **roadmap** (`#[allow(dead_code)]`).
- **Limpieza pendiente:** `contracts/src/placeholder.rs` (huérfano de S0, no expuesto en `lib.rs`).

### 3.2 Qué existe en agents (riel B x402)
`agents/src/x402/` — oráculo de reputación pago-por-request: `reputation-server`, `facilitator`
local (fallback), `FailoverFacilitatorClient`, cliente de pago. **INV-4 fail-closed** (no cobra
contra el escrow) e **idempotencia de settle** (no reintenta `settle` en el fallback).

---

## 4. Invariantes (lo que se audita SIEMPRE)

| ID | Invariante |
|---|---|
| **INV-1** | El **agente nunca mueve capital relevante**: solo `route_micropayment` capado (por-llamada **y** acumulado/epoch). Releases grandes = `admin` + M-de-N. |
| **INV-2** | Ningún release depende del **input del LLM**: lo autoriza una **condición on-chain** (hoy M-de-N; en Sem 2, tally de atestaciones ≥ umbral). |
| **INV-3** | **Sin Addressable Entity** (no activado en Casper). Custodia = `purse` + multisig nativo de cuenta + M-de-N en contrato. |
| **INV-4** | **x402 solo** para servicios HTTP (oráculo). El settlement de escrow es **transferencia del contrato**, no x402. |
| **INV-5** | Atestaciones = mensajes **firmados off-chain, verificados on-chain** (gasless). Silencio = recibido. |
| **INV-6** | **Datos de circuito cerrado**; liquidación por **aritmética multiparte**, nunca juicio humano. |
| **INV-7** *(Sem 1)* | **Escrow earmarked:** los fondos de un lote solo van a SU productor o vuelven a SUS compradores. **Nunca se cruzan entre lotes.** |

---

## 5. Modo de trabajo (cómo colaboramos)

1. **Claude (este asistente):** planifica en detalle, escribe los specs/briefs, y **audita** el
   código de los agentes. **No escribe el código de producción.**
2. **Agentes opencode:** implementan. Una **tarea = un branch** (idealmente en un **`git worktree`
   aislado** para evitar mezclas) y **commitean en su rama**.
3. **Loop por tarea:** brief → opencode implementa+commitea → **Claude audita** (gate) → si toca
   fondos, **audit DUAL** (dos modelos independientes + Claude, se diffean hallazgos) → fix-round
   si hace falta → **merge (ff) a `fase-0`**.
4. **Regla de oro:** nada que toca capital pasa sin cumplir los invariantes. Un "NO PASA" de la
   primera pasada de audit es el sistema funcionando, no fallo del implementador.
5. **Wrapper obligatorio:** todo brief se antepone con `docs/plan/_wrapper.md` (invariantes +
   layout + regla anti-alucinación + "commitea en tu rama").

### 5.1 Routing de modelos (resumen — detalle en `docs/plan/model-routing.md`)
La **cuota (peticiones/5h)** es el muro, no el $. Reservar T2 (GLM-5.2 880/5h, Qwen3.7 Max 950/5h)
para lo más duro; **MiniMax M3** (promo x3) de caballo de batalla.

| Trabajo | Primario | Escalar | Auditar |
|---|---|---|---|
| **Contratos (fondos)** | **DeepSeek V4 Pro** *(validado)* | GLM-5.2 | dual **Qwen3.7 Max + GLM-5.2** + Claude |
| Agentes TS / x402 | MiniMax M3 ⚡ | DeepSeek V4 Pro | — |
| Tests | MiniMax M2.7 | — | — |
| Infra / deploy / CI | MiniMax M3 | GLM-5.2 | Claude |
| Web / frontend | Qwen3.7 Plus | GLM-5.2 | — |

**Calibración real:** Kimi K2.7 Code → mejor contrato (S1) · GLM-5.2 → x402+fixes limpios ·
DeepSeek V4 Pro → fix de S2 limpio → **primario de contratos** (~4× cuota vs GLM).

---

## 6. Roadmap

### ✅ Fase 0 — De-risk (CERRADA)
Mató los 4 riesgos técnicos con spikes verdes en VM Odra:

| Spike | Resultado | Implementó |
|---|---|---|
| **S0** scaffold + toolchain + CI | ✅ | Kimi K2.7 Code |
| **S1** `OhuVault` custodia en `purse` | ✅ (el mejor de los 3) | Kimi K2.7 Code |
| **S2** multisig + entrypoint capado ("no drena") + **cap acumulado on-chain** | ✅ (audit dual + fix DeepSeek) | GLM-5.2 / fix DeepSeek V4 Pro |
| **S4** riel B x402 (oráculo reputación) + fail-closed + idempotencia | ✅ (+ fixes S4a/S4b) | GLM-5.2 |
| **S3** atestación gasless Ed25519 on-chain + anti-replay por lote + domain sep. | ✅ (audit dual + fix #3/#4) | DeepSeek V4 Pro |

### 🔜 Semana 1 — Núcleo de liquidación (PLANEADA — `docs/plan/semana-1.md`)
Del vault genérico al **modelo de LOTE**. Hito: **un lote feliz liquida E2E en Testnet**.
- **W1-0** micro-fix `chain_id==0` (cierra audit de S3).
- **W1-1** modelo de lote + escrow **earmarked** (`open_lote`/`deposit_to_lote`/`post_bond`) — INV-7.
- **W1-2** `release_to_producer` happy-path (gate admin M-de-N interino).
- **W1-3** **deploy real a Testnet** + E2E feliz + **multisig nativo real** (cierra el TODO de S2).

### 🗓️ Semanas 2-4 (de `ohu.md §11`)
- **Sem 2 — Atestación + mutual:** disparador **paramétrico** (reemplaza el gate M-de-N por
  **tally de atestaciones ≥ umbral**, INV-2) + camino `SETTLED_FAIL` (refund + slash +
  indemnización) + contrato `MutualPool` (prima 0.5%). **Aquí se cierran los gates diferidos de S3.**
- **Sem 3 — Agentes + RFQ + oráculo x402:** los 3 agentes (Agregador/Tesorería/Mutual) con cuenta
  propia; RFQ simple; `Reputation` expuesto como API x402; dashboard CSPR.cloud.
- **Sem 4 — Anclaje real + UX + demo:** onboarding gasless, vídeo, redes.

---

## 7. Gates diferidos y TODOs abiertos (no perder)

| Item | Severidad | Dónde se cierra |
|---|---|---|
| **S3 #1** — `verify_attestation` NO valida `signer ∈ compradores(lote)` (ponderado por share) | 🔴 crítico-en-diseño, **inerte hoy** (nada consume atestaciones) | **Sem 2** (ya con registro lote→compradores de W1-1) |
| **S3 #2** — atestación sin `valid_before` (expiry) | 🟠 alto | **Sem 2** |
| Disparador paramétrico (tally) reemplaza gate M-de-N de `release` | — | **Sem 2** |
| `chain_id==0` no validado en `init` (docstring miente) | 🟡 bajo | **W1-0** |
| Multisig **nativo** (associated keys) — capa 2, falta `KEYS_MANAGER_WASM` | 🟠 (la capa on-chain ya protege) | **W1-3** |
| `placeholder.rs` huérfano | 🟢 limpieza | cualquier toque de contracts |
| Migración EIP-712 completa (hoy ruta activa = Ed25519, permitida por spec) | 🟢 roadmap | Sem 2+ |
| Pushear `fase-0` al remoto / dominio + marca de "Ohu" | — | go-to-market |

---

## 8. Índice de documentos

| Documento | Qué es |
|---|---|
| `ohu.md` | **Spec del producto** (fuente de verdad del *qué*) |
| `techs-specs.md` | **Due diligence técnica** (manda en factibilidad) |
| `CLAUDE.md` | Guía de repo para Claude Code (convenciones, restricciones duras) |
| `docs/ESTADO.md` | **Este doc** — estado + roadmap + cómo retomar |
| `docs/plan/fase-0-derisk.md` | Spec de la Fase 0 (4 spikes) + invariantes §1 |
| `docs/plan/semana-1.md` | Spec de Semana 1 (núcleo de liquidación) |
| `docs/plan/model-routing.md` | Routing de modelos por tarea (derivado del bench) |
| `docs/plan/_wrapper.md` | Wrapper obligatorio para briefs de opencode |

---

## 9. Cómo retomar (para Claude)

1. Lee este doc + `CLAUDE.md` + la sección relevante de `ohu.md`/`techs-specs.md`.
2. Estado: `git -C . log --oneline -5` en `fase-0`; `cd contracts && cargo odra test` para
   confirmar verde.
3. Próxima acción concreta: **arrancar W1-1 (+W1-0)** — abrir un worktree aislado
   (`git worktree add ../ohu-w1 -b spike/w1-lote fase-0`), darle a **DeepSeek V4 Pro** el
   `_wrapper.md` + el brief de W1-1, y **auditar** el resultado (dual Qwen3.7 Max + GLM-5.2 + Claude)
   contra los criterios de `semana-1.md` y los invariantes (en especial **INV-7**).
4. Tras cada tarea de contrato: **audit dual** → merge ff a `fase-0`.
