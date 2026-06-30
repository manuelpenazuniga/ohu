# Plan — Ohu · Semana 2 (Atestación gasless + mutual paramétrica)

> Diseño gobernado por `ohu.md §4` (mutual anti-fraude) y `§5` (arquitectura). Feasibilidad por
> `techs-specs.md`. Construye sobre Semana 1 (lote desplegado y liquidado E2E en Testnet — ver
> `infra/deployments/testnet.md`).

## Objetivo
Del settlement **happy-path interino** (gate admin + M-de-N de W1-2) al **disparador PARAMÉTRICO real**:
el settlement se autoriza por un **tally ponderado de atestaciones ≥ umbral** (INV-2), y un lote que
**falla** ejecuta `SETTLED_FAIL` (refund de escrow + slash del bono + indemnización acotada), con un
contrato **`MutualPool`** que cobra prima en los éxitos y paga la **cola** de la indemnización.

**Hito:** *un lote que falla **indemniza por regla**, on-chain, sin liquidador humano.*

## Invariantes (vigentes + activadas esta semana)
- **INV-1** (agente nunca mueve capital relevante) · **INV-7** (escrow earmarked por lote) — **se preservan**.
- **INV-2** (ningún capital por juicio del LLM; **una condición on-chain autoriza**) — **se ACTIVA**: el
  tally de atestaciones reemplaza el gate M-de-N como disparador de settlement.
- **Nuevas reglas económicas (de §4):**
  - El **bono del productor es el pagador primario**; la mutual solo absorbe la **cola** (§4.1, §4.7).
  - La indemnización deja **íntegro, nunca enriquece**: refund = dinero propio del comprador en escrow;
    indemnización de disrupción **acotada y fija < valor de la mercadería** (§4.2). No drena el pool.
  - **Quórum** de no-recepción (p.ej. **≥60% de la share** del lote) para disparar fail/slash (§4.3).
  - **Silencio = recepción** (default anti-griefing/anti-gas) (§4.5).

## Tareas

### W2-0 — Atestación ponderada y autorizada (cierra S3 #1 + S3 #2)
- **Objetivo:** `verify_attestation` deja de ser inerte: valida **`signer ∈ compradores(lote)`** (existe
  `lote_share[(lote,signer)] > 0` desde W1-1) y **`valid_before`** (expiry; revert si `now ≥ valid_before`).
  Registra el **veredicto** (recibido / no-recibido) ponderado por la **share** del firmante. Anti-replay
  por `(lote_id, signer)` ya existe (fix #3).
- **Entregable:** `verify_attestation` autorizada + con expiry; mapping de veredicto-por-firmante;
  errores nuevos (`NotABuyer`, `AttestationExpired`); tests (firmante no-comprador revierte, expirada
  revierte, doble-atestación revierte, ponderación correcta).
- **Aceptación:** una atestación de un no-comprador o expirada **revierte**; una válida queda registrada
  con su peso = share. **No** mueve fondos todavía (solo siembra el tally de W2-1).
- **Dependencias:** ninguna (base de la semana). **Modelo:** contrato → DeepSeek V4 Pro; **audita:**
  Claude + Gemini 3.1 Pro (High) vía agy.

### W2-1 — Disparador paramétrico (tally ponderado) — activa INV-2
- **Objetivo:** acumular `tally_negativo[lote]` y `tally_positivo[lote]` (suma de shares de firmantes por
  veredicto). Reglas de transición desde **FUNDED**:
  - `tally_negativo ≥ quorum_fail · funded` (p.ej. 60%) → lote **elegible a `SETTLED_FAIL`**.
  - en cierre de ventana, **silencio = recibido**: `funded − tally_negativo ≥ umbral_ok` → elegible a
    **`release_to_producer`**. (El default favorece release; el fail exige quórum activo.)
  - El **disparo** lo hace un entrypoint determinista (`evaluate_lote(lote)`), no el juicio del agente.
- **Entregable:** tally por lote alimentado desde W2-0; `evaluate_lote` que fija el resultado; el gate
  de `release_to_producer` pasa de `admin + M-de-N` a **`tally ≥ umbral`** (M-de-N queda como
  salvaguarda de emergencia, no como disparador normal). Parámetros (`quorum_fail`, ventana) en init.
- **Aceptación:** con ≥60% de share atestando no-recibido, el lote queda en ruta FAIL; con silencio/
  positivo, en ruta OK — **sin input del agente**. Tests de los umbrales (borde 59% vs 60%).
- **Dependencias:** W2-0. **Modelo:** contrato → DeepSeek V4 Pro; **audita:** Claude + Gemini 3.1 Pro (High).

### W2-2 — Camino `SETTLED_FAIL` (refund + slash + indemnización)
- **Objetivo:** `settle_failure(lote)`: **FUNDED → SETTLED_FAIL**. (a) **refund** a cada comprador su
  `lote_share` (su propio escrow — NO drena pool, INV-7); (b) **slash** del bono del productor (pagador
  primario); (c) **indemnización** de disrupción **acotada** a los compradores, financiada **primero por
  el bono slasheado**, y la **cola** por `MutualPool` (W2-3). CEI estricto + `checked_*`. Cierra el gate
  diferido de **"fondos atrapados"** (refund real).
- **Entregable:** `settle_failure` con la aritmética de §4.1/§4.2; libera el `reserved_lote_balance` del
  lote correctamente (igual que `release_to_producer`); eventos; tests (refund exacto, slash, tope de
  indemnización respetado, no cruza fondos entre lotes).
- **Aceptación:** un lote fallido devuelve a cada comprador su share, slashea el bono, paga indemnización
  ≤ tope, y el `reserved_lote_balance` baja exactamente en lo liberado. **Triple audit** (toca fondos).
- **Dependencias:** W2-1. **Modelo:** contrato → DeepSeek V4 Pro; **audita:** Claude + Gemini 3.1 Pro
  (High) **+ GPT-5.5 (codex) cuando vuelva la cuota** — ver nota de cuotas abajo.

### W2-3 — Contrato `MutualPool` (prima + cola de indemnización)
- **Objetivo:** nuevo contrato Odra con purse. **Cobra prima** (≈0.5%) al liberar un lote feliz; **paga la
  cola** de indemnización cuando el bono slasheado quedó corto; lleva **reserva** (objetivo ≥1.5× cola
  anual esperada, §4.7). Solo `OhuVault` (o el admin) puede gatillar el pago de cola; nadie drena el pool
  por juicio del agente.
- **Entregable:** `MutualPool` (Odra.toml lo registra); entrypoints `collect_premium`, `pay_tail`,
  `reserve()`; integración OhuVault↔MutualPool (cross-contract o vía Tesorería/Mutual con autorización
  de contrato). Tests del pool aislado + integración.
- **Aceptación:** una liberación feliz capitaliza el pool; un fail con bono corto saca **solo la cola**
  del pool (acotada); el pool nunca paga más que su reserva. **Triple audit** (toca fondos).
- **Dependencias:** W2-2. **Modelo:** contrato → DeepSeek V4 Pro; **audita:** Claude + Gemini + GPT-5.5 (cuota).

### W2-4 — Deploy a Testnet + E2E de lote FALLIDO (indemniza por regla)
- **Objetivo:** desplegar `OhuVault` (actualizado) + `MutualPool` a Testnet y correr un **lote que falla
  E2E real**: compradores depositan → productor pone bono → ventana → **≥60% atesta no-recibido** →
  `evaluate_lote` → `settle_failure` → refund + slash + indemnización (cola desde `MutualPool`),
  visible on-chain.
- **Entregable:** extender `infra/scripts/deploy_testnet.sh` (2 contratos) + un `bin/livenet_e2e_fail.rs`
  análogo al feliz (multi-key: compradores firman atestaciones negativas); registro en
  `infra/deployments/testnet.md`.
- **Aceptación:** en Testnet, un lote fallido **indemniza por regla** sin liquidador, on-chain. **Hito
  de la semana.** Reusar el lowering MVP de `wasm-opt` y el patrón livenet de W1-3.
- **Dependencias:** W2-0..W2-3. **Modelo:** infra → MiniMax M3 / Gemini Flash (High); **audita:** Claude.

## Modelos y auditoría — nota de CUOTAS
- **Routing** (ver `docs/plan/model-routing.md` + `docs/tips/agy-cli.md`): contratos = **DeepSeek V4 Pro**
  (opencode-go); auditor primario = **Gemini 3.1 Pro (High)** vía `agy`; tareas simples = **Gemini 3.5
  Flash** / **MiniMax M3**.
- **GPT-5.5 (codex) sin cuota por unas horas (2026-06-30).** La tercera pata del **triple audit** de lo
  que toca fondos (W2-2, W2-3) queda **diferida**: mergear con **dual Claude + Gemini**, y **pasar el
  cierre holístico GPT-5.5 antes de W2-4 (deploy)** — la disciplina de "pase holístico de familia distinta
  antes de cualquier deploy" cazó el crítico de escrow en Sem1; **no saltarla**.

## Hito Semana 2 (Definition of Done)
Un **lote que falla liquida `SETTLED_FAIL` EN TESTNET**: ≥60% de share atesta no-recibido →
`evaluate_lote` (paramétrico, sin juicio del agente) → refund del escrow a cada comprador + slash del
bono + indemnización acotada (cola desde `MutualPool`), **visible on-chain**. + S3 #1/#2 cerrados + INV-2 activada.

## Entra en Semana 3 (NO Semana 2)
- Los 3 agentes con cuenta propia orquestando (Agregador/Tesorería/Mutual); RFQ simple.
- `Reputation` on-chain expuesto como **API x402** (Riel B) + un cobro x402 real en Testnet.
- Dashboard CSPR.cloud. Diferidos de diseño que siguen abiertos: micro-bono de atestación (§4.4),
  ruta `DISPUTED` del reclamante aislado (§4.3), rotación de approvers, multisig nativo admin (Parte B de W1-3).
