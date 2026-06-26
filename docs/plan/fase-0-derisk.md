# Plan de implementación — Ohu · Fase 0 (sprint de de-risk, 48–72h)

> Fuente: `ohu.md §11` (plan) + `techs-specs.md` (factibilidad). Objetivo de la Fase 0: **matar los 4 riesgos técnicos** con spikes mínimos en Testnet antes de construir el producto. Nada aquí es "demo de mentira": cada spike valida un mecanismo real.
>
> **Rol:** Claude **planifica y audita**; los **agentes opencode implementan**. Una rama por tarea, PR, y **mi auditoría es el gate** — ninguna tarea se da por cerrada sin pasar su checklist.

---

## 1. Invariantes globales (esto es lo que audito en TODAS las tareas)

| ID | Invariante | Por qué |
|:--|:--|:--|
| **INV-1** | El **agente nunca mueve capital relevante**: su cuenta solo llama entrypoints **capados**; todo release grande exige `caller == admin` **+ aprobación M-de-N** en el contrato. | "Un LLM comprometido toca, como mucho, micropagos acotados." |
| **INV-2** | **Ningún release depende del input del agente/LLM**: siempre lo autoriza una **condición on-chain** (tally de atestaciones ≥ umbral). | Liquidación paramétrica, no peritaje. |
| **INV-3** | **No usar Addressable Entity** (no activado en mainnet ni testnet). Custodia = contrato con `purse` + **multisig nativo de cuenta** + **M-de-N en contrato**. | `techs-specs.md §2`. |
| **INV-4** | **x402 solo** para oráculo/servicios HTTP (pay-per-request). El settlement de escrow es una **transferencia del contrato**, NO x402. | `techs-specs.md §4`. |
| **INV-5** | Atestaciones = **EIP-712 firmadas off-chain, verificadas on-chain** (gasless). **Silencio = recibido**. | `ohu.md §3, §4.5`. |
| **INV-6** | **Datos de circuito cerrado**; liquidación por **aritmética sobre evidencia multiparte**, nunca juicio humano. | Regla de calidad R1/R2. |

> Regla para los agentes: **no inventar API**. Si la firma exacta de Odra / `casper-eip-712` / `casper-x402` no se conoce, consultar los docs oficiales enlazados y, si hay duda, dejar un `// TODO(audit): verificar contra <doc>` — yo lo reviso. Preferible un hueco marcado a una API alucinada.

**Docs oficiales de referencia:** Odra `https://odra.dev/docs/` · `casper-ecosystem/casper-eip-712` · `make-software/casper-x402` · CSPR.cloud `https://docs.cspr.cloud/` · Casper AI Toolkit `https://www.casper.network/ai`.

---

## 2. Asignación de modelos (por fortaleza de familia)

| Tier | Modelos | Para qué |
|:--|:--|:--|
| 🧠 **Razonamiento / seguridad / cripto** | **DeepSeek V4 Pro**, **Qwen3.7 Max** | custodia, multisig M-de-N, EIP-712, lógica que toca dinero |
| 🛠️ **Agentic coding / implementación** | **Kimi K2.7 Code**, **GLM-5.2** | escribir contratos/integraciones, tool-use, scaffolding |
| ⚡ **Rápido / mecánico** | **Qwen3.6 Plus**, **MiMo-V2.5**, **Kimi K2.6** | tests repetitivos, tipos, glue, fixtures |
| 📚 **Contexto largo / multi-archivo** | **MiniMax M3** | leer todo el spec, refactors amplios, orquestación |
| 🔁 **Reserva / segunda opinión** | GLM-5.1, MiMo-V2.5-Pro, MiniMax M2.7, Qwen3.7 Plus | rotación para auditoría cruzada |

**Regla de auditoría:** el modelo auditor ≠ el modelo implementador. Si DeepSeek V4 Pro implementó S2, audita Qwen3.7 Max (y yo encima).

---

## 3. Stack y layout (monorepo)

```
ohu/
├── contracts/        # Odra (Rust) — OhuVault, (luego) MutualPool, Reputation, CoopRegistry
├── agents/           # TypeScript — Agregador, Tesoreria, Mutual (casper-js-sdk)
├── web/              # TypeScript — dashboard (Fase 3)
├── infra/            # scripts deploy testnet, .env.example, Makefile/justfile
└── docs/plan/        # este plan
```
Toolchain (pinear a último estable y dejarlo en README): Rust stable + `cargo-odra`, `casper-client`; Node 20+ + pnpm + `casper-js-sdk`. Red objetivo: **Casper Testnet**.

---

## 4. Tareas de la Fase 0

> Formato: **Objetivo · Entregable · Criterios de aceptación · Dependencias · Modelo · Mi checklist de auditoría · Brief para opencode.**

### S0 — Scaffold del monorepo + toolchain + CI
- **Objetivo:** esqueleto reproducible; `cargo odra build/test` y el toolchain TS corriendo en limpio + en CI.
- **Entregable:** layout §3; `contracts/` con proyecto Odra vacío que compila; `infra/.env.example` (NODE_URL testnet, CHAIN_NAME, keys placeholder); `justfile`/`Makefile` con `build/test/deploy`; GitHub Actions que corre `cargo odra test` y lint.
- **Aceptación:** clonar limpio → `just build && just test` verde sin pasos manuales; CI verde en el PR.
- **Dependencias:** ninguna. **Modelo:** Kimi K2.7 Code. **Audita:** GLM-5.2 + yo.
- **Auditoría:** reproducibilidad (toolchain pineado), nada hardcodeado, `.env` fuera de git, CI realmente ejecuta los tests.
- **Brief opencode:** *"Crea el monorepo Ohu (layout en docs/plan/fase-0-derisk.md §3). contracts/ = proyecto Odra que compila y testea con cargo-odra; infra/ con justfile (build/test/deploy a Casper testnet) y .env.example; CI en GitHub Actions que corre cargo odra test + clippy. No incluyas lógica de negocio aún. Pinea versiones y documenta el setup en README."*

### S1 — `OhuVault`: custodia en `purse` con depósito + transferencia E2E
- **Objetivo:** contrato Odra que recibe fondos en un **`purse`** y transfiere de salida — el ladrillo base de la custodia (INV-3).
- **Entregable:** `contracts/.../ohu_vault.rs` con: init que crea el purse; `deposit()` (entra al purse); `withdraw_to(recipient, amount)` **temporalmente abierto solo para el test E2E** (en S2 se cierra tras admin+M-de-N); tests Odra de ida y vuelta.
- **Aceptación:** test E2E en testnet/livenet-sim: depósito de N CSPR al purse → balance del purse = N → transferencia a una cuenta → balance receptor sube N. Sin usar Addressable Entity.
- **Dependencias:** S0. **Modelo:** Kimi K2.7 Code o GLM-5.2. **Audita:** DeepSeek V4 Pro.
- **Auditoría:** que use `purse` (no AE); manejo de under/overflow; que `withdraw_to` quede marcado `// TODO(S2): gate admin+M-de-N` para no olvidarlo; eventos emitidos para CSPR.cloud.
- **Brief opencode:** *"Implementa OhuVault en Odra con un purse: deposit() y withdraw_to(recipient, amount). Tests de depósito+transferencia E2E. Usa purse + multisig nativo a futuro; NO uses Addressable Entity. Marca withdraw_to como provisional (se gateará en S2). Emite eventos de deposit/withdraw."*

### S2 — Multisig nativo + entrypoint capado (el invariante "el agente no drena") ⭐ crítico
- **Objetivo:** demostrar **INV-1**: una **cuenta admin** con claves asociadas ponderadas + threshold alto controla el capital; una **cuenta agente** puede llamar un entrypoint **capado** (micropago con tope) pero **NO** puede ejecutar un retiro grande.
- **Entregable:** script/infra que configura la cuenta admin (associated keys + weights + deploy threshold); en `OhuVault`: `route_micropayment(...)` (invocable por `operator`/agente, con **tope por llamada**) y gating de `withdraw_to`/`release` a `caller == admin` **+ patrón `approve(id)` de M firmantes antes de `execute(id)`**; tests que prueban **ambos lados**.
- **Aceptación:** (a) el agente ejecuta un micropago dentro del tope ✔; (b) el agente intentando retirar capital **revierte** ✔; (c) un release grande **solo** procede con M aprobaciones distintas + caller admin ✔.
- **Dependencias:** S1. **Modelo:** **DeepSeek V4 Pro** o **Qwen3.7 Max** (lo más fuerte). **Audita:** el otro de esos dos + yo (a fondo).
- **Auditoría:** intentar **romperlo** — ¿hay algún path donde el operator mueva más que el tope? ¿reentrancy? ¿el threshold real fuerza co-firma? ¿`execute` valida que las M aprobaciones son de firmantes **distintos** y vigentes? Esta tarea no pasa con dudas.
- **Brief opencode:** *"Implementa el modelo de seguridad de OhuVault SIN Addressable Entity: cuenta admin con claves asociadas ponderadas + threshold alto; cuenta agente (operator) que solo puede llamar route_micropayment con tope por llamada; withdraw/release grande gateado por caller==admin + aprobación M-de-N (approve(id) de M firmantes distintos → execute(id)). Tests que prueben que el agente NO puede drenar y que el release grande exige M-de-N. Adjunta los tests negativos."*

### S3 — Verificación EIP-712 on-chain (atestación gasless)
- **Objetivo:** verificar **dentro del contrato** un mensaje **EIP-712** firmado off-chain (INV-5) — base de la atestación "recibí/no recibí" sin que el comprador tenga gas.
- **Entregable:** port del ejemplo `permit` de `casper-eip-712` a Odra: `verify_attestation(payload, signature, signer)` que valida la firma on-chain; test con vector firmado off-chain. **Fallback** documentado: validación ed25519 simple si EIP-712 se atasca.
- **Aceptación:** firma válida → `true` y registra atestación; firma manipulada/replay → revierte. Gasless desde la perspectiva del firmante (lo retransmite el agente).
- **Dependencias:** S1. **Modelo:** **DeepSeek V4 Pro** o **Qwen3.7 Max**. **Audita:** el otro.
- **Auditoría:** dominio/typehash correctos; **anti-replay** (nonce/lote); que no se pueda reusar una atestación en otro lote; correcta recuperación del signer. Validar contra el repo `casper-eip-712`.
- **Brief opencode:** *"Porta el ejemplo permit de casper-ecosystem/casper-eip-712 a Odra: verify_attestation(payload, signature, signer) verificada on-chain, con anti-replay por (lote, nonce). Test con un mensaje firmado off-chain. Si te bloqueas con EIP-712, implementa el fallback ed25519 y déjalo marcado. No inventes el typehash: cópialo del repo oficial."*

### S4 — Un cobro x402 real en Testnet (riel B genuino)
- **Objetivo:** un endpoint **x402** que cobre por request (el futuro "oráculo de reputación") usando `make-software/casper-x402` — validando el **riel x402 genuino** (INV-4), separado del settlement de escrow.
- **Entregable:** servicio HTTP mínimo en `agents/` que responde `402` con monto → cliente firma autorización ed25519 → facilitator liquida en Testnet → se entrega el recurso. **Fallback:** facilitator de referencia **local** apuntando a Testnet.
- **Aceptación:** un request de prueba paga y recibe el recurso; la liquidación se ve on-chain (CSPR.cloud). Queda explícito en el código/README que **esto NO es el rail de settlement de escrow**.
- **Dependencias:** S0 (independiente de S1–S3). **Modelo:** **Kimi K2.7 Code** o **GLM-5.2** (integración/agentic). **Audita:** Qwen3.7 Max.
- **Auditoría:** que x402 NO se use para mover fondos de escrow; manejo del flujo 402→firma→settle; fallback local realmente funciona si el facilitator hosteado falla.
- **Brief opencode:** *"Monta un servicio x402 mínimo con make-software/casper-x402: GET protegido que responde 402 con monto, acepta la autorización firmada (ed25519) y entrega el recurso tras liquidar en Casper Testnet. Incluye un facilitator local como fallback apuntando a Testnet. Documenta claramente que x402 es solo para servicios HTTP, no para settlement de escrow."*

---

## 5. Gate de auditoría (lo corro al cerrar cada PR)
1. ¿Cumple **todos** los INV aplicables? (S2/S3 son los que más miro.)
2. ¿Los **tests negativos** existen y pasan? (el agente no drena; firma manipulada revierte; x402 no toca escrow.)
3. ¿Algún API alucinado? → contrastar contra docs oficiales; los `TODO(audit)` se resuelven.
4. ¿Reproducible desde clon limpio? ¿secrets fuera de git? ¿eventos para CSPR.cloud?
5. Reporte corto: ✅/❌ por criterio + riesgos abiertos.

## 6. Definition of Done — Fase 0
Los 4 spikes en verde en **Testnet** con sus tests (incl. negativos), CI corriendo, y un `infra/` que despliega de cero. Con eso, los 4 unknowns están muertos y arrancamos **Semana 1 (núcleo de liquidación)** sin walk-backs.
```
