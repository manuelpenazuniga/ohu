# Routing de modelos — Ohu

> Derivado de `docs/internal/model-bench.md` (jun 2026). Reemplaza las asignaciones "a ojo"
> de `fase-0-derisk.md §2` para todo lo que falta.

## Principios (de la economía de cuota)
1. **La cuota (peticiones/5h) es el muro, no el $.** Reservar **T2** (GLM-5.2 ~880/5h, Qwen3.7 Max ~950/5h) para el **10-20% más duro**. Ratio workers:premium **≥ 10:1**.
2. **MiniMax M3 tiene promo x3 AHORA** (~9.600/5h) → úsalo como **caballo de batalla** para agents/backend/CI/tests/docs/comprensión mientras dure.
3. **Empate costo-beneficio → gana calidad.** En **web3-crítico y audit, nunca escatimar.**
4. **Validaciones en repo (2026-06-26):** **Kimi K2.7 Code** implementó **S1 (OhuVault), el mejor de los 3 spikes** → validado para **coding de contratos** (seguir observando en más tareas). **GLM-5.2** hizo **S4 + fixes S4a/S4b** limpios (incluso atrapó un bug de interacción S4a↔S4b) → confirmado para **x402/web3-crítico**. **GLM-5.1** dominado por 5.2 → solo overflow.

## Mapa por tipo de trabajo en Ohu
| Trabajo | Primario | Escalar a | Auditar |
|---|---|---|---|
| **Contratos Odra que tocan fondos** (vault, multisig, EIP-712, settlement, mutual) = **web3-crítico** | **GLM-5.2** | Qwen3.7 Max | **dual Qwen3.7 Max + DeepSeek V4 Pro** |
| Contrato Rust no-crítico / refactor de lifetimes | DeepSeek V4 Pro | GLM-5.2 | — |
| **Agentes TS** (orquestación, MCP, CSPR.cloud, x402) | **MiniMax M3** ⚡ | DeepSeek V4 Pro | — |
| RFQ clearing (algoritmo determinista) | **DeepSeek V4 Pro** | GLM-5.2 | revisión 1× |
| Web / dashboard (Next/React) | **Qwen3.7 Plus** | GLM-5.2 (pulido) | — |
| Tests (unit/integration) | **MiniMax M2.7** (M3 durante promo) | — | — |
| Docs (README, comments) | **MiMo V2.5** | MiniMax M3 (arquitectura) | — |
| Debug / tests rojos | **DeepSeek V4 Pro** | GLM-5.2 | — |
| Comprensión de repo (extender, NO reconstruir) | **DeepSeek V4 Pro** (1M ctx, MRCR 83.5) | MiniMax M3 | — |
| Workers (boilerplate, codemods, format masivo) | **DeepSeek V4 Flash** | MiMo V2.5 | — |
| **Audit de seguridad** (cada hito de contrato) | **dual: Qwen3.7 Max + DeepSeek V4 Pro** | GLM-5.2 (desempate) | — |

## Por etapa
| Etapa | Implementa | Audita |
|---|---|---|
| **S2** multisig + entrypoint capado *(web3-crítico)* | **GLM-5.2** *(corrige: antes decía DeepSeek)* | dual Qwen Max + DeepSeek + Claude |
| **S3** EIP-712 on-chain *(web3-crítico/cripto)* | **GLM-5.2** (o Qwen3.7 Max si la cuota GLM va apretada) | dual + Claude |
| **Sem 1** núcleo de liquidación (contratos) | GLM-5.2 · infra/deploy: M3 · tests: M2.7 | dual |
| **Sem 2** atestación + mutual (contratos + aritmética paramétrica) | **GLM-5.2** (AIME 99.2 ayuda en la math de la mutual) | dual |
| **Sem 3** agentes + RFQ + oráculo x402 | agentes TS → **M3** · RFQ algo → **DeepSeek V4 Pro** · contrato Reputation → GLM-5.2 · servicio x402 → M3 · dashboard → Qwen3.7 Plus | dual en el contrato Reputation |
| **Sem 4** web / UX / demo | frontend → Qwen3.7 Plus → pulido GLM-5.2 · docs/pitch → MiMo V2.5 / M3 | — |

## Disciplina de auditoría (lo que toca fondos)
En cada hito de contrato: **Claude audita** + tú corres el **par independiente (Qwen3.7 Max + DeepSeek V4 Pro)** y **diffeas hallazgos**. Un bug perdido en un contrato cuesta más que toda la cuota premium del mes.
