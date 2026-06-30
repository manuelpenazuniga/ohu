# Routing de modelos — Ohu

> Actualizado **2026-06-29**. Reemplaza la versión basada en cuota GLM/Qwen.
> **Cambio mayor:** fuera **Qwen3.7 Max** y **GLM-5.2** (quemaron una cuenta en 1 día — retirados por
> **costo**, no por calidad). Entran **GPT-5.5** (audit premium) y **agy / Antigravity CLI**
> (Gemini 3.1 Pro *high* + Gemini 3.5 Flash *low/med/high*, **plan generoso**).

## Herramientas (lanes) y su economía
| Lane | Para qué | Economía |
|---|---|---|
| **Claude** (este asistente) | planifica, escribe briefs, **audita** (gate). **No** escribe código de producción. | — |
| **opencode** | **implementación pesada**: DeepSeek V4 Pro, Kimi K2.7 Code, MiniMax M3, MiniMax M2.7. | **cuota req/5h = recurso escaso** → reservar para impl pesada |
| **agy (Antigravity CLI)** | worker ligero **y** auditor: Gemini 3.1 Pro (*high*), Gemini 3.5 Flash (*low/med/high*). | **plan generoso** → empujar aquí lo ligero + auditorías |
| **GPT-5.5** | **audit premium** de hitos que tocan fondos. | reservar para lo crítico |

## Principios
1. **Dos pools, úsalos por escasez.** La **cuota de opencode** es el recurso caro/escaso → **solo
   implementación pesada**. El **plan de agy/Gemini es generoso** → manda ahí lo ligero (tests, docs,
   infra, web) **y las auditorías rutinarias**. Esto libera cuota de opencode para los contratos.
2. **Audit ≠ implementador, siempre.** Si implementa opencode (DeepSeek/Kimi/M3), auditan
   **Gemini 3.1 Pro high + (GPT-5.5 si toca fondos) + Claude** — familias distintas, diversidad real.
3. **Lo que toca fondos nunca escatima:** audit **triple** = **GPT-5.5 + Gemini 3.1 Pro high + Claude**;
   se **diffean** los tres sets de hallazgos.
4. **Validaciones en repo (2026-06-26):** **Kimi K2.7 Code** implementó **S1 (OhuVault), el mejor de
   los 3 spikes** → validado para **coding de contratos**. **DeepSeek V4 Pro** hizo el **fix-round de S2**
   limpio (epoch cap on-chain + 13 tests reales, clippy limpio) → **primario de contratos** (1M ctx,
   MRCR 83.5). GLM-5.2/Qwen3.7 Max **retirados por costo** (su rol de audit lo toman GPT-5.5 + Gemini).

## Mapa por tipo de trabajo en Ohu
| Trabajo | Primario | Escalar a | Auditar |
|---|---|---|---|
| **Contratos Odra que tocan fondos** (vault, multisig, EIP-712, settlement, mutual) = **web3-crítico** | **DeepSeek V4 Pro** *(opencode)* · co-primario **Kimi K2.7 Code** | **Gemini 3.1 Pro high** *(agy)* | **GPT-5.5 + Gemini 3.1 Pro high + Claude** (triple, auditor ≠ implementador) |
| Contrato Rust no-crítico / refactor de lifetimes | DeepSeek V4 Pro | Gemini 3.1 Pro high | Gemini 3.1 Pro high (1×) |
| **Agentes TS** (orquestación, MCP, CSPR.cloud, x402) | **MiniMax M3** ⚡ | DeepSeek V4 Pro · Gemini 3.1 Pro high | — |
| RFQ clearing (algoritmo determinista) | **DeepSeek V4 Pro** | Gemini 3.1 Pro high | Gemini 3.1 Pro high (1×) |
| Web / dashboard (Next/React) | **Gemini 3.5 Flash (high)** *(agy)* | MiniMax M3 | — |
| Tests (unit/integration) | **Gemini 3.5 Flash (med)** · MiniMax M2.7 | — | — |
| Docs (README, comments) | **Gemini 3.5 Flash (med)** | MiniMax M3 (arquitectura) | — |
| Debug / tests rojos | **DeepSeek V4 Pro** | Gemini 3.1 Pro high | — |
| Comprensión de repo (extender, NO reconstruir) | **DeepSeek V4 Pro** (1M ctx) · Gemini 3.1 Pro high | MiniMax M3 | — |
| Workers (boilerplate, codemods, format masivo) | **Gemini 3.5 Flash (med)** | MiniMax M2.7 | — |
| **Audit de seguridad** (cada hito de contrato) | **GPT-5.5 + Gemini 3.1 Pro high + Claude** (auditor ≠ implementador) | el 3º como desempate | — |

## Por etapa (lo que falta)
| Etapa | Implementa | Audita |
|---|---|---|
| **Sem 1** núcleo de liquidación (contratos) | contratos → **DeepSeek V4 Pro** / Kimi K2.7 Code · infra/deploy → **Gemini 3.5 Flash (med)** o M3 · tests → **Gemini 3.5 Flash (med)** | **GPT-5.5 + Gemini 3.1 Pro high + Claude** |
| **Sem 2** atestación + mutual (contratos + aritmética paramétrica) | **DeepSeek V4 Pro** (math de la mutual) · escalar → **Gemini 3.1 Pro high** | triple (toca fondos) |
| **Sem 3** agentes + RFQ + oráculo x402 | agentes TS → **M3** · RFQ algo → **DeepSeek V4 Pro** · contrato Reputation → **DeepSeek V4 Pro** · servicio x402 → M3 · dashboard → **Gemini 3.5 Flash high** | triple en el contrato Reputation |
| **Sem 4** web / UX / demo | frontend → **Gemini 3.5 Flash high** · docs/pitch → Gemini 3.5 Flash low / M3 | — |

## Disciplina de auditoría (lo que toca fondos)
En cada hito de contrato: **Claude audita** + corres el **par independiente GPT-5.5 + Gemini 3.1 Pro
(high)** vía sus CLIs y **diffeas los tres sets de hallazgos**. El implementador (DeepSeek/Kimi vía
opencode) **nunca se audita a sí mismo**. Un bug perdido en un contrato cuesta más que toda la cuota
premium del mes.

## Veredicto agy/Antigravity (worker vs auditor)
- **Gemini 3.1 Pro (high) → auditor de familia diversa** (con GPT-5.5 + Claude) y **escalación** de
  contrato/algoritmo difícil cuando la cuota de opencode aprieta. Su mayor valor es auditar **sin
  gastar cuota de opencode**.
- **Gemini 3.5 Flash → workhorse ligero, SOLO tareas simples** (tests, docs, infra/CI, web,
  codemods). Hace bien lo simple pero **no lo sofisticado** → úsalo en **med/high** (no low);
  lo complejo (contratos, algoritmos, audit) **nunca** va a Flash. Sustituye los tiers baratos de
  opencode y **libera su cuota para los contratos**.

## Operativa CLI / providers (2026-06-29 — validado en repo)
- **Implementadores = `opencode run` con provider `opencode-go` SIEMPRE.** **NUNCA `opencode` (Zen)**:
  no tiene método de pago (falla con *"No payment method"*). IDs: `opencode-go/deepseek-v4-pro`,
  `opencode-go/kimi-k2.7-code`, `opencode-go/minimax-m3`.
  - Invocación no-interactiva: `opencode run --dir <worktree> -m opencode-go/<modelo> "<prompt>"`.
    **No** requiere `--dangerously-skip-permissions` (ejecuta tools en `--dir` por defecto). Ojo: el
    flag `-f` es *array-greedy* y se traga el mensaje como archivo → pasar el prompt **inline**.
- **Auditor por-tarea = `agy` (Antigravity) con Gemini 3.1 Pro (High).** Invocación:
  `agy -p "<prompt>" --model "Gemini 3.1 Pro (High)" --add-dir <worktree>` (modo `--print`, solo
  lectura; no necesita bypass de permisos).
- **GPT-5.5 = `codex` CLI (operable, validado 2026-06-29).** Cuenta ChatGPT/Codex con cupo.
  Invocación: `codex exec -s read-only -m gpt-5.5 -c mcp_servers="{}" "<prompt>"` (no-interactivo,
  sandbox **read-only** = no edita; pasar prompt inline). ⚠️ `gpt-5.5-codex` **no** está soportado en
  cuenta ChatGPT → usar **`gpt-5.5`** a secas. Uso: **auditoría holística de cierre** (por-fase/día)
  sobre `main`; opcionalmente como tercer set por-tarea. **El gate por-tarea sigue = Claude + Gemini
  3.1 Pro High**; GPT-5.5 cierra cada fase.
