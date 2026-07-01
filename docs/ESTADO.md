# Ohu — Estado del proyecto y planificación

> **Qué es este documento:** contexto de arranque para Claude (y colaboradores). Resume **qué es
> Ohu**, **qué está construido hoy**, **cómo trabajamos**, y **el roadmap**. Léelo primero; luego
> profundiza en los documentos enlazados.
>
> **Última actualización:** 2026-07-01 · rama `main` @ `031a7ec` (pusheado) ·
> **Fase 0 CERRADA · Semana 1 CERRADA.** W1-0/W1-1/W1-2 + fix crítico de escrow-isolation (120 tests
> verdes) **+ W1-3: OhuVault DESPLEGADO en Casper Testnet y un lote feliz LIQUIDADO E2E on-chain**
> (deploy `b595b892…`, contrato `contract-833696c8…`; 5 cuentas nativas firmando el settlement M-de-N;
> ver `infra/deployments/testnet.md`). La **auditoría de cierre GPT-5.5** había hallado un CRÍTICO
> (purse compartido drenaba escrow earmarked) → **corregido y re-auditado en triple = PASA** (§7).
> **Semana 2 EN CURSO (4/5):** **W2-0** atestación ponderada (S3 #1/#2), **W2-1** disparador paramétrico
> — **INV-2 activada** (el tally, no M-de-N, autoriza el release), **W2-2** `SETTLED_FAIL` (refund+slash+
> indemnización pull), **W2-3** `MutualPool` (prima + cola). **W2-3 con TRIPLE audit Claude+Gemini+GPT-5.5
> = PASA tras 3 rondas de fix:** GPT (framing adversarial) halló un CRÍTICO económico —anillo con bono
> de 1 mote drenaba el pool— que 176 tests verdes + la conservación NO vieron; el fix cerró el drenaje
> pero introdujo un lock de fondos (ronda 2) y dejó vivo un griefing de FUNDED-ansiosa (ronda 3, cerrado
> con `lock_lote`: solo el operador cierra la ventana). Falta **W2-4** (deploy + E2E del lote fallido) —
> ⚠️ requiere **actualizar el tooling livenet** (`livenet_e2e.rs` debe llamar `lock_lote`; `livenet_deploy.rs`
> tiene init args viejos). **Pendiente menor:** multisig nativo admin (Parte B, no bloquea).
> **Entorno verificado en macOS** (Apple Silicon, arm64) tras clonar desde GitHub — ver §3.1.

---

## 0. TL;DR

- **Ohu** = procura cooperativa agéntica + mutual paramétrica sobre **Casper** (Buildathon 2026).
- **Fase 0 (de-risk) COMPLETA y verde**: scaffold, custodia en `purse`, multisig "el agente no
  drena", x402, y atestación gasless verificada on-chain. **55 tests Odra + 24 TS + 1 web**, clippy
  y typecheck limpios.
- **Próximo:** **Semana 1 — núcleo de liquidación** (modelo de lote + settlement happy-path en
  Testnet). Planeada en `docs/plan/semana-1.md`, aún sin implementar.
- **Modo de trabajo:** **Claude planifica en detalle y audita**; **opencode** implementa lo pesado
  (DeepSeek V4 Pro, Kimi K2.7 Code, MiniMax M3) y **agy/Antigravity** (Gemini 3.1 Pro high + 3.5 Flash)
  hace worker ligero + audit; **GPT-5.5** audita lo que toca fondos. Ver §5.

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

- **Rama activa:** `main` @ `e1d73a3` — **toda la Fase 0 está consolidada en `main` y pusheada a
  `origin/main`** (`github.com/manuelpenazuniga/ohu`). La rama `fase-0` ya no existe local; su
  historia (S0→S3 + planes) quedó en `main`. ✅ El remoto ya refleja el avance.
- **Cuentas GitHub:** identidad de commit fijada a **`manuelpenazuniga`**
  (`manuelpenazuniga@gmail.com` / noreply), no `fundacionrescatedemascotas`.

### 3.1 Entorno macOS (migración desde WSL2 — verificado 2026-06-29)
Repo clonado desde GitHub; el origen era **WSL2 (Linux x86_64)** y ahora corre en **macOS Apple
Silicon (arm64)**. Compatibilidad comprobada de punta a punta — **todo verde**:

| Check | Resultado en macOS arm64 |
|---|---|
| `cargo odra test` | ✅ **55/55** |
| `pnpm -r test` | ✅ agents **24/24**, web **1/1** |
| `cargo clippy --all-targets -D warnings` | ✅ limpio |
| Toolchain pinned (`contracts/rust-toolchain` = `nightly-2026-01-01`) | ✅ instalado como `-aarch64-apple-darwin` |
| `cargo-odra` 0.1.7 · Node v22 · pnpm 11.8.0 | ✅ presentes |
| `target/` recompilado (Mach-O arm64, **sin** restos ELF de WSL2) | ✅ paquetes regenerados |
| Scripts `infra/scripts/*.sh` (UTF-8, sin CRLF) | ✅ sin artefactos de Windows |

- **Único gap del toolchain:** **`casper-client` NO instalado** — sólo hace falta para el **deploy a
  Testnet (W1-3)**; nada del trabajo actual lo requiere. Instalar antes de W1-3.
- `contracts/wasm/` aún no generado (no se ha corrido `just build` del contrato todavía).

### 3.2 Qué existe en el contrato HOY (`OhuVault`)
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

### 3.3 Qué existe en agents (riel B x402)
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
**Dos pools, por escasez:** la **cuota de opencode (req/5h) es el recurso caro/escaso** → solo impl
pesada (DeepSeek V4 Pro, Kimi K2.7 Code, MiniMax M3). El **plan de agy/Antigravity (Gemini) es
generoso** → trabajo ligero + auditorías. **Fuera Qwen3.7 Max y GLM-5.2** (quemaron una cuenta en 1
día). **GPT-5.5** = audit premium de lo que toca fondos.

| Trabajo | Primario | Escalar | Auditar |
|---|---|---|---|
| **Contratos (fondos)** | **DeepSeek V4 Pro** / Kimi K2.7 Code *(opencode)* | **Gemini 3.1 Pro high** *(agy)* | **GPT-5.5 + Gemini 3.1 Pro high + Claude** |
| Agentes TS / x402 | MiniMax M3 ⚡ | Gemini 3.1 Pro high | — |
| Tests | Gemini 3.5 Flash (med) / MiniMax M2.7 | — | — |
| Infra / deploy / CI | Gemini 3.5 Flash (med) / M3 | — | Claude |
| Web / frontend | Gemini 3.5 Flash (high) | — | — |

**Herramientas:** **opencode** = impl pesada (cuota escasa) · **agy/Antigravity** = Gemini 3.1 Pro
high (auditor primario/escalación) + Gemini 3.5 Flash med/high (worker **solo tareas simples** — no
sofisticadas; plan generoso) · **GPT-5.5** = auditor premium · **Claude** = planifica + audita.

**Calibración real:** Kimi K2.7 Code → mejor contrato (S1) · DeepSeek V4 Pro → fix de S2 limpio →
**primario de contratos**. (Qwen3.7 Max/GLM-5.2 retirados por **costo**, no por calidad.)

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

### 🔄 Semana 1 — Núcleo de liquidación (EN CURSO — `docs/plan/semana-1.md`)
Del vault genérico al **modelo de LOTE**. Hito: **un lote feliz liquida E2E en Testnet**.
- **W1-0** ✅ micro-fix `chain_id==0` (cierra audit de S3). `30e51c1` — DeepSeek V4 Pro, audit Claude.
- **W1-1** ✅ modelo de lote + escrow **earmarked** (`open_lote`/`deposit_to_lote`/`post_bond`) — INV-7.
  `3141cf3` + fix `4c21e67` — DeepSeek V4 Pro, **audit dual Claude + Gemini 3.1 Pro High** (fix-round:
  `checked_add`/`Error::Overflow`, `post_bond` state==OPEN, `NotAdminNorOperator`). **89 tests verdes.**
- **W1-2** ✅ `release_to_producer` happy-path (M-de-N **lote-aware** interino:
  `propose_release`/`approve_release`/`release_to_producer`, estado `SETTLED_OK`, CEI estricto,
  paga `funded+bond` al productor). `fee21a7` — DeepSeek V4 Pro, **audit dual Claude + Gemini
  (PASA sin fixes)**. **111 tests verdes.**
- **W1-fix (crítico)** ✅ **aislamiento de escrow** — la auditoría de cierre **GPT-5.5** halló que el
  purse compartido permitía a `route_micropayment`/`execute` drenar el escrow earmarked (INV-1/INV-7).
  Fix: `reserved_lote_balance` (los outflows genéricos solo gastan `balance − reserved`) + epoch
  `saturating_sub` + producer ∉ {admin,operator,approvers}. `7e194fd` — DeepSeek V4 Pro,
  **re-audit triple Claude + Gemini + GPT-5.5 = PASA**. **120 tests verdes.**
- **W1-3** ✅ **deploy real a Testnet + lote feliz E2E on-chain.** OhuVault desplegado vía **Odra
  livenet** (`contract-833696c8…`, package `hash-6c1a1366…`, tx `b595b892…`). E2E completo firmado
  por 5 cuentas nativas distintas: `open_lote`(admin)→`deposit`(buyer,10)→`post_bond`(producer,5)=FUNDED
  →`propose`(approver0)→`approve`×2 (M-de-N)→`release`(admin)=SETTLED_OK; escrow `funded+bond`=15 CSPR
  liberado al producer. `2b9a928`+`68cd805`. Ver `infra/deployments/testnet.md` (8 tx hashes).
  Resueltos en el camino: `casper-client` instalado, RPC vivo (`node.testnet.casper.network`),
  **WASM lowering a MVP con wasm-opt** (bulk-memory/sign-ext), `deploy_testnet.sh` reproducible.
  ⏳ **Pendiente (Parte B, no bloquea W1):** `KEYS_MANAGER_WASM` + `setup_admin_account` real (multisig
  nativo de la cuenta admin — la capa M-de-N **on-chain** ya protege; ver §7).

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
| ~~**S3 #1** — `verify_attestation` no valida `signer ∈ compradores` ponderado~~ | ✅ **CERRADO** — gate `NotABuyer` + tally ponderado | **W2-0** |
| ~~**S3 #2** — atestación sin `valid_before` (expiry)~~ | ✅ **CERRADO** — `valid_before` dentro de la firma + `AttestationExpired` | **W2-0** |
| ~~Disparador paramétrico (tally) reemplaza gate M-de-N de `release`~~ | ✅ **CERRADO** — `evaluate_lote` (tally ≥ quórum) → EVAL_OK/FAIL; release exige EVAL_OK (**INV-2 activada**) | **W2-1** |
| ~~`chain_id==0` no validado en `init`~~ | ✅ **CERRADO** | **W1-0** (`30e51c1`) |
| ~~**CRÍTICO** — purse compartido: outflows genéricos (`route_micropayment`/`execute`) drenaban escrow earmarked (INV-1/INV-7)~~ | ✅ **CERRADO** (`reserved_lote_balance`, triple audit) | **fix `7e194fd`** |
| ~~**W1-1** — audit de cierre con **GPT-5.5**~~ | ✅ **HECHO** (halló el crítico ↑) | `codex exec` |
| ~~**W1-1** — transición a FUNDED **demasiado ansiosa** + griefing 1-mote~~ | ✅ **CERRADO** — `lock_lote`: la ventana la cierra solo admin/operator, no un depósito (triple audit) | **W2-3 ronda 3** |
| **Fondos atrapados** — refund de lote fallido | ✅ **CERRADO** vía `SETTLED_FAIL` + `withdraw_settlement` (pull). Falta refund de lote OPEN abandonado / FUNDED sin evaluar (timeout) | **W2-2** (resto Sem 2+) |
| Operator puede crear lotes basura (ID squatting, no mueve fondos) | 🔵 bajo | Sem 2+ |
| Rotación/recuperación de approvers (inmutables; pérdida de clave congela M) | 🟠 gobierno | Sem 2+ |
| Multisig **nativo** (associated keys) — capa 2, falta `KEYS_MANAGER_WASM` (hay que construirlo) | 🟠 (la capa on-chain ya protege) | **W1-3 Parte B / Sem 2** (no bloquea) |
| ~~`deploy.sh` stub + init args desactualizados~~ | ✅ **CERRADO** — deploy real vía livenet (`deploy_testnet.sh`, 8 args) | **W1-3** (`2b9a928`) |
| ~~`casper-client` sin instalar~~ | ✅ **CERRADO** (v5.0.1; el bloqueo era proxy Varnish del ISP) | **W1-3** |
| **WASM bulk-memory rechazado por la VM** (toolchain 2026 + std precompilada) | ✅ **CERRADO** — lowering MVP con `wasm-opt` en `deploy_testnet.sh` | **W1-3** |
| `placeholder.rs` huérfano · Migración EIP-712 completa | 🟢 | limpieza / Sem 2+ |

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
2. Estado: `git -C . log --oneline -6` en `main`; `cd contracts && cargo odra test` → **120 verdes**.
   Despliegue Testnet vivo en `infra/deployments/testnet.md` (contrato + 8 tx del E2E).
3. **Semana 1 CERRADA** (W1-0/1/2 + fix crítico + deploy real + lote E2E on-chain). **Próxima acción:
   Semana 2** (atestación + disparador paramétrico + `MutualPool` + camino `SETTLED_FAIL`; cierra los
   gates S3 #1/#2 — ver §7). **Opcional antes:** Parte B de W1-3 = **`KEYS_MANAGER_WASM`** (interfaz
   `add_key`/`set_thresholds` en `setup_admin_account.sh`) para el multisig nativo de la cuenta admin
   (no bloquea: la capa M-de-N on-chain ya protege). Deploy/E2E se reproducen con
   `bash infra/scripts/deploy_testnet.sh` y `cargo run --bin ohu_livenet_e2e --features livenet`
   (requieren `binaryen` + `.env` + cuentas fondeadas).
4. **Herramientas CLI** (validadas): implementa `opencode run --dir <wt> -m opencode-go/<modelo>`;
   audita Gemini `agy --model "Gemini 3.1 Pro (High)" --add-dir <wt> -p` y GPT-5.5
   `codex exec -s read-only -m gpt-5.5 -c mcp_servers="{}"`. Detalle en `model-routing.md`.
5. **Disciplina:** todo lo que toca fondos → **audit triple** (Claude + Gemini + GPT-5.5) y el pase
   holístico de cierre **antes de cualquier deploy** (cazó el crítico de escrow que el por-tarea no vio).
