# Plan — Ohu · Semana 1 (Núcleo de liquidación)

> Fuente: `ohu.md §3` (máquina de estados), `§5` (contratos), `§11` (plan). Continúa la **Fase 0**
> (`docs/plan/fase-0-derisk.md`, cerrada @ aba8d8a). **Hito:** un **lote feliz liquida
> end-to-end en Testnet**.
>
> **Rol/auditoría/invariantes:** `fase-0-derisk.md §1` + `model-routing.md`. Contratos =
> **DeepSeek V4 Pro / Kimi K2.7 Code** (opencode) → escalación **Gemini 3.1 Pro high** (agy);
> **audit triple GPT-5.5 + Gemini 3.1 Pro high + Claude**. Tests → Gemini 3.5 Flash (med).
> Infra/deploy → Gemini 3.5 Flash (med) o MiniMax M3.

## Objetivo
Pasar del vault genérico de Fase 0 al **modelo de LOTE**: los compradores depositan su parte
**earmarked** a un lote, el productor pone bono, y un settlement happy-path libera al productor +
devuelve el bono. Esto construye el **registro `lote → compradores`** del que dependen la
autorización de atestaciones (S3 #1) y la liquidación paramétrica — ambas de **Semana 2**.

## Invariantes (vigentes + nuevo)
- **INV-1/2/3** (Fase 0): el agente no drena; releases gateados on-chain + M-de-N; sin Addressable Entity.
- **INV-7 (NUEVO — escrow earmarked):** los fondos de un lote SOLO se liberan al productor de ESE
  lote o se devuelven a SUS compradores. **Jamás se cruzan fondos entre lotes.**
- **INV-1 reforzado:** `release_to_producer` mueve capital → gateado. **Sem 1: admin + M-de-N
  (interino). Sem 2: tally de atestaciones ≥ umbral (INV-2).**

## Tareas

### W1-0 — Micro-fix `chain_id == 0` (cierre del audit de S3)
- **Entregable:** `if chain_id == 0 { revert InvalidSetup }` en `init` (tras el check de
  `epoch_window_ms`) + test `init_reverts_when_zero_chain_id`.
- **Aceptación:** deploy con `chain_id=0` revierte; la docstring (ohu_vault.rs:226) deja de mentir.
- **Modelo:** DeepSeek V4 Pro (trivial). **Audita:** Claude.

### W1-1 — Modelo de lote + escrow earmarked
- **Objetivo:** el vault rastrea **por lote**: productor, estado, `comprador → share` depositada,
  total fondeado, bono del productor.
- **Entregable:** `open_lote(lote_id, producer, …)` (admin/operator), `deposit_to_lote(lote_id)`
  `#[odra(payable)]` (registra la share del comprador), `post_bond(lote_id)` (el productor
  deposita su bono), getters de estado + eventos. Estados: `OPEN → FUNDED`.
- **Aceptación:** N compradores depositan al lote L → `funded(L) == Σ shares`; depositar a un lote
  inexistente/cerrado revierte; el bono queda registrado; **INV-7:** tests que prueban que los
  fondos de L no afectan a L'.
- **Dependencias:** Fase 0. **Modelo:** DeepSeek V4 Pro / Kimi K2.7 Code → Gemini 3.1 Pro high. **Audita (triple):** GPT-5.5 + Gemini 3.1 Pro high + Claude.
- **Auditoría:** contabilidad por-lote sin mezclar el purse global; overflow U512 en sumas; ¿un
  comprador puede inflar su share o retirar antes de tiempo?; control de acceso de `open_lote`.

### W1-2 — Settlement happy-path (`release_to_producer`)
- **Objetivo:** liberar el escrow del lote al productor + devolver bono; **estado → `SETTLED_OK`**.
- **Entregable:** `release_to_producer(lote_id)` gateado por `caller == admin` **+ M-de-N**
  (reusa el patrón `propose/approve/execute` de S2, lote-aware); transfiere `funded(L)` al
  productor; devuelve el bono; marca `SETTLED_OK`; emite evento (hook de reputación).
- **MARCADOR Sem 2:** `// TODO(Sem2): reemplazar el gate admin-M-de-N por el disparador
  paramétrico (tally de atestaciones ≥ umbral, INV-2). Aquí se conecta verify_attestation —
  recién entonces se le añade autorización (S3 #1) + valid_before (S3 #2).`
- **Aceptación:** lote `FUNDED` + M-de-N → el productor recibe `funded(L)`, bono devuelto, estado
  `SETTLED_OK`; sin M-de-N revierte; no se liquida dos veces; no se liquida un lote no-`FUNDED`.
- **Dependencias:** W1-1. **Modelo:** DeepSeek V4 Pro / Kimi K2.7 Code → Gemini 3.1 Pro high. **Audita (triple):** GPT-5.5 + Gemini 3.1 Pro high + Claude.
- **Auditoría:** CEI/reentrancy; ¿se puede pagar a otro productor?; doble settlement; bono devuelto
  al productor correcto; **INV-7** (no cruzar lotes).

### W1-3 — Deploy a Testnet + E2E feliz + multisig nativo (cierra el TODO de S2)
- **Objetivo:** desplegar `OhuVault` en **Casper Testnet** y correr un **lote feliz E2E real** (no
  solo odra-test VM); configurar la cuenta `admin` con **associated keys + threshold** — esto
  **cierra el TODO de multisig nativo de S2** (la capa 2 que quedó en pausa por falta de WASM).
- **Entregable:** `infra/scripts/deploy.sh` funcional (deploy real a Testnet);
  `setup_admin_account.sh` ejecutable (resolver el `KEYS_MANAGER_WASM` faltante); script/E2E:
  N compradores depositan → fondean lote → admin **co-firma nativa** → `release_to_producer` →
  el productor recibe, **visible en CSPR.cloud**.
- **Aceptación:** en Testnet un lote feliz liquida E2E; el deploy de `release` exige **co-firma
  nativa real** (no una sola clave) → cierra el criterio de S2 "el threshold real fuerza co-firma".
- **Dependencias:** W1-2. **Modelo:** infra/backend → **Gemini 3.5 Flash (med)** o **MiniMax M3** ⚡;
  código de contrato → DeepSeek V4 Pro. **Audita:** Claude (scripts + verificación on-chain) + Gemini 3.1 Pro high.
- **Auditoría:** secrets fuera de git; el E2E prueba co-firma **real**; se ve on-chain en
  CSPR.cloud; fallback documentado si el keys-manager WASM no existe.

## Hito Semana 1 (Definition of Done)
Un **lote feliz liquida end-to-end EN TESTNET**: compradores depositan → productor pone bono →
admin co-firma (multisig nativo) → `release_to_producer` paga al productor + devuelve bono, todo
**visible on-chain**. + `chain_id==0` cerrado + **INV-7** probado.

## Entra en Semana 2 (NO Semana 1) — gates heredados de S3
- **S3 #1:** autorización `signer ∈ compradores(lote)` **ponderada por share** (el registro
  `lote→compradores` ya existe desde W1-1).
- **S3 #2:** `valid_before` (expiry) en la atestación.
- **Disparador paramétrico:** reemplazar el gate admin-M-de-N de `release` por el **tally de
  atestaciones ≥ umbral** (INV-2) + camino `SETTLED_FAIL` (refund + slash + indemnización) +
  `MutualPool`.
