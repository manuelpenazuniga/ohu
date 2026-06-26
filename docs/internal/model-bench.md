# Asignación multiagente de modelos — OpenCode (jun 2026)

> Análisis costo-beneficio para enrutar 13 modelos a tareas de dev según tu perfil
> (Rust / Web3 / web-LMS / Node / planificación). Benchmarks recopilados jun 2026.

---

## 0. La idea central antes de la tabla

En un plan de **cuota fija**, el costo real **no es el precio por token: es la cuota de peticiones**. Tu tabla revela tres tiers naturales según `peticiones/5h`:

| Tier | Modelos | Cuota/5h | Para qué sirve |
|---|---|---|---|
| **T0 — Workers** | DeepSeek V4 Flash, MiMo V2.5 | ~30.000 | Volumen masivo, casi gratis |
| **T1 — Daily drivers** | MiniMax M3·M2.7, DeepSeek V4 Pro, MiMo V2.5 Pro, Qwen3.7·3.6 Plus | ~3.200–4.300 | El 80% del trabajo diario |
| **T2 — Premium / hard** | GLM-5.2·5.1, Kimi K2.6·K2.7 Code, Qwen3.7 Max | ~880–1.350 | Lo difícil; escasos, reservar |

**Principio de orquestación:** el orquestador debe mandar **80–90%** de los subtasks a T0/T1 y **escalar solo el 10–20% más duro** a T2. La cuota premium es ~10× más escasa que la de workers (GLM 880/5h vs Flash 31.650/5h ≈ 1:36), así que cada llamada a T2 "vale" como ~30 llamadas a un worker.

**Dato temporal:** MiniMax M3 tiene cuota extra ahora → es el **mejor valor de T1 mientras dure el bono**. Úsalo agresivo como driver de backend/agéntico.

**GLM-5.1 está dominado por GLM-5.2** (mismo precio $1.40/$4.40, misma cuota 880/5h, scores inferiores). No hay razón para usar 5.1 salvo como overflow cuando agotes 5.2.

---

## 1. Tabla de benchmarks consolidada

Ordenada por capacidad de coding (SWE-bench Pro donde existe). `(v)` = vendor-reported, `(i)` = independiente, `TB` = Terminal-Bench.

| Modelo | SWE-Pro | SWE-Verified | Terminal-Bench | Standout | $ in/out | Cuota 5h / mes |
|---|---|---|---|---|---|---|
| **GLM-5.2** | **62.1** (i) | – | **81.0** (TB2.1) | #1 open coding · #1 web-dev DesignArena · FrontierSWE 74.4 · AIME 99.2 | 1.40/4.40 | 880 / 4.300 |
| **Qwen3.7 Max** | 60.6 (v) | 80.4 | 69.7 (TB2.0) | **Menor alucinación 22.9%** · 35h autónomo | 2.50/7.50 | 950 / 4.770 |
| **MiniMax M3** | 59.0 (v) | – | 66.0 (TB2.1) | **BrowseComp 83.5 (#1)** · MCP 74.2 · multimodal · **cuota extra** | 0.30/1.20 | 3.200⁺ / 16.000 |
| **Kimi K2.6** | 58.6 (i) | 80.2 | – | General + tool-use sólido | 0.95/4.00 | 1.150 / 5.750 |
| **GLM-5.1** | 58.4 (i) | – | 63.5 (TB2.1) | *(superado por 5.2)* | 1.40/4.40 | 880 / 4.300 |
| **MiMo V2.5 Pro** | 57.2 (v) | 78.9 | 68.4 (TB2.0) | **−40-60% tokens** · long-horizon · harness-aware | 1.74/3.48 | 3.250 / 16.300 |
| **MiniMax M2.7** | 56.22 (v) | 78 | 57.0 (TB2) | VIBE-Pro 55.6 · SWE-Multiling 76.5 · rápido | 0.30/1.20 | 3.400 / 17.000 |
| **DeepSeek V4 Pro** | – | **80.6** | sólido | **LiveCodeBench 93.5 (#1)** · Codeforces 3206 · MRCR 83.5 | 1.74/3.48 | 3.450 / 17.150 |
| **Qwen3.7 Plus** | ≈Max (v) | sólido | sólido (TB2.0) | Multimodal · **Deep-Planning** · ~6× < Max | 0.40/1.60 | 4.300 / 21.600 |
| **Qwen3.6 Plus** | – | 78.8 | 61.6 (TB2.0) | **Frontend/web fuerte** · "vibe coding" · 3D/games | 0.50/3.00 | 3.300 / 16.300 |
| **DeepSeek V4 Flash** | – | 79.0 (v) | – | Rápido (108 t/s) · barato · indep. mid-pack | 0.14/0.28 | 31.650 / 158.150 |
| **Kimi K2.7 Code** | s/d (solo vendor) | s/d | s/d | Coding+MCP · −30% tokens vs K2.6 | 0.95/4.00 | 1.350 / 9.250 |
| **MiMo V2.5** | – (< Pro) | – | – | **El más barato** · multimodal · 1M ctx · cuota enorme | 0.14/0.28 | 30.100 / 150.400 |

**Caveats honestos:**
- GLM-5.2 y Kimi K2.7 Code tienen muchos números **solo del vendor**; Kimi K2.7 aún **no tiene SWE-bench independiente** (mídelo tú).
- DeepSeek V4 Pro **no publica SWE-bench Pro**; brilla en algorítmico (LiveCodeBench/Codeforces) y retrieval de contexto largo (MRCR), no necesariamente en repo-level Pro.
- Terminal-Bench mezcla versiones 2.0 vs 2.1 → no son 1:1.
- DeepSeek V4 Flash: el 79 Verified es vendor; en leaderboards independientes (BenchLM) queda mid-pack (~#56/124). Tenlo como **worker fiable, no como cerebro**.

---

## 2. Matriz tarea → modelo (perfil MP)

Para cada tarea: **Primario** (default), **Escalar a** (cuando el primario falla o el task es difícil), y la lógica costo-beneficio.

| Tarea | Primario (valor) | Escalar a (calidad) | Por qué |
|---|---|---|---|
| **Worker / grunt** (codemods, boilerplate, format, mass-edits) | **DeepSeek V4 Flash** | MiMo V2.5 (si multimodal/1M ctx) | Cuota ~30k/5h + $0.14/$0.28. Ahorra cuota premium. |
| **Dev Web3** (Solana/Anchor, Solidity, Substrate) | **GLM-5.2** | Qwen3.7 Max | Stakes altos + Rust/contratos. GLM #1 open coding y más barato que Max; Max si quieres mínima alucinación en lógica de contrato. |
| **Dev Rust** (general — ClawCrate/Quincha/PennyPrompt) | **DeepSeek V4 Pro** | GLM-5.2 | V4 Pro: razonamiento algorítmico (Codeforces 3206) + cuota usable (3.450/5h). GLM para refactors de crate largos/lifetimes duros. |
| **Dev Web / frontend** (LMS UI, Next/React) | **Qwen3.7 Plus** | GLM-5.2 | Plus es multimodal (lee Figma/screenshots), barato, alta cuota. GLM es literalmente **#1 web-dev** para el pulido final. |
| **Dev Node / backend** (API, Supabase, server LMS) | **MiniMax M3** ⚡ | DeepSeek V4 Pro | M3 con cuota extra ahora + agéntico/MCP fuerte + 1M ctx. V4 Pro si la lógica es densa. |
| **Planificación / orquestador** | **GLM-5.2 (Max)** | Qwen3.7 Max | Cerebro del swarm: razonamiento + long-horizon. Pocas llamadas pero críticas → gasta T2 aquí. Routine planning barato → Qwen3.7 Plus (Deep-Planning). |
| **Debug** (root-cause, tests rojos) | **DeepSeek V4 Pro** | GLM-5.2 | Mejor razonamiento analítico; GLM para debug multi-archivo agéntico. |
| **Auditoría / security** (audit de contratos, ChainSentinel) | **Qwen3.7 Max** + **DeepSeek V4 Pro** (dual, en paralelo) | GLM-5.2 (desempate) | Audit = correr **2 modelos independientes y diffear**. Max tiene menor alucinación (menos falsos positivos); V4 Pro razona fino. Justifica T2. |
| **Test** (unit/integration, coverage) | **MiniMax M2.7** | MiniMax M3 | VIBE-Pro/end-to-end fuerte, rápido, barato. Mecánico → no malgastes premium. |
| **Documentación** (READMEs, HOJA_TECNICA, comments) | **MiMo V2.5** | MiniMax M3 (docs de arquitectura) | El más barato + 1M ctx para tragarse el repo. M3 cuando la doc necesita razonar el sistema. |

### Categorías extra que encajan en tu perfil

| Tarea | Primario | Escalar a | Por qué |
|---|---|---|---|
| **Comprensión de codebase** (anti-"reconstruir desde cero" → tu dolor del HOJA_TECNICA) | **DeepSeek V4 Pro** | MiniMax M3 / GLM-5.2 | 1M ctx + **MRCR 83.5** (mejor retrieval long-context). Le metes el repo entero + HOJA_TECNICA y produce el contexto de trabajo. Es **el agente que evita el rebuild**. |
| **Arquitectura / system design** | **GLM-5.2 (Max)** | Qwen3.7 Max | Máximo razonamiento + 1M ctx. |
| **Math / contenido PAES** (tu base IMO, generar ítems/soluciones) | **GLM-5.2** (AIME 99.2) | DeepSeek V4 Pro | GLM lidera math; V4 Pro para algorítmica pura. |
| **Data / SQL** (capa de datos LMS, analytics) | **MiniMax M3** | DeepSeek V4 Pro | Queries complejas → V4 Pro. |
| **DevOps / CI** (GitHub Actions, multi-máquina) | **MiniMax M3** | GLM-5.2 (TB 81.0) | Terminal-heavy: M3 barato para rutina, GLM para lo complejo. |

---

## 3. Router de referencia (lógica sugerida)

```
function route(task):
  # 1. ¿Es masivo/mecánico? → worker
  if task.tipo in [codemod, format, boilerplate, bulk_docs, mass_test]:
      return "deepseek-v4-flash"            # o mimo-v2.5 si necesita 1M ctx / imágenes

  # 2. ¿Security/audit? → dual independiente + diff
  if task.tipo == audit:
      return ["qwen3.7-max", "deepseek-v4-pro"]   # corre ambos, compara hallazgos

  # 3. ¿Necesita el cerebro? (plan/arquitectura/web3 crítico)
  if task.tipo in [orquestar, arquitectura, web3_critico, math_hard]:
      return "glm-5.2"                       # effort=max

  # 4. Daily drivers por dominio
  switch task.dominio:
      rust       -> "deepseek-v4-pro"
      frontend   -> "qwen3.7-plus"           # multimodal
      backend    -> "minimax-m3"             # ⚡ cuota extra ahora
      debug      -> "deepseek-v4-pro"
      tests      -> "minimax-m2.7"
      ci_devops  -> "minimax-m3"
      comprension_repo -> "deepseek-v4-pro"  # 1M ctx + MRCR

  # 5. Escalado: si el primario falla 2 veces → sube un tier
```

**Presupuesto premium (regla de pulgar):** GLM-5.2 ≈ 176 llamadas/h, Qwen3.7 Max ≈ 190/h. Mantén el ratio **workers:premium ≥ 10:1**. Si el orquestador escala más del 20% a T2, algo está mal enrutado.

---

## 4. Cómo validar (no confíes solo en vendor benchmarks)

Tienes algo que ningún benchmark tiene: **tu repo real (LMS PAES)**. Arma un mini-eval en OpenCode:

1. Elige **5–8 tareas reales** representativas (1 fix de bug repo-level, 1 componente React, 1 endpoint Node, 1 módulo Rust, 1 query SQL, 1 doc).
2. Corre cada tarea con 3 candidatos por categoría (p.ej. backend: M3 vs M2.7 vs V4 Pro).
3. Mide: **¿pasa los tests?**, **tokens consumidos**, **peticiones usadas**, **# de reintentos**.
4. El ganador real = mejor `tests_pasados / cuota_consumida`, no el mayor SWE-bench.

Esto resuelve directo tu fricción: en vez de que el modelo proponga reconstruir, el eval premia al que **extiende lo existente** correctamente.

---

## 5. TL;DR — stack por defecto

- **Cerebro / plan / web3-crítico / math:** GLM-5.2
- **Rust / debug / comprensión de repo:** DeepSeek V4 Pro
- **Backend / CI / data:** MiniMax M3 *(úsalo fuerte mientras tenga cuota extra)*
- **Frontend:** Qwen3.7 Plus → pulido con GLM-5.2
- **Tests:** MiniMax M2.7
- **Workers / docs masivas:** DeepSeek V4 Flash + MiMo V2.5
- **Audit:** Qwen3.7 Max + DeepSeek V4 Pro en paralelo, diff de hallazgos
- **No uses:** GLM-5.1 (dominado por 5.2). Kimi K2.7 Code: prométe en coding+MCP pero **valídalo tú** antes de meterlo al router (sin benchmarks independientes aún).

---

## 6. v2 — Afinación: máxima relación costo-beneficio

> Objetivo de esta sección: extraer el **máximo trabajo capaz por ventana de cuota**.
> **Regla de oro (tuya):** si dos modelos quedan con costo-beneficio **similar**, gana **siempre la calidad**.

### 6.1 La métrica de valor

En plan de cuota fija:

```
Valor ≈ (Capacidad efectiva)  ×  (Throughput de cuota)        [criterio primario]
        ──────────────────────────────────────────────
                      $ / token                              [desempate secundario]
```

Dos matices que cambian todo:
1. **Capacidad por encima del nivel que la tarea exige = desperdicio.** Usar GLM-5.2 (880/5h) para escribir tests es quemar el recurso más escaso en algo que M2.7/M3 hacen igual de bien.
2. **La cuota, no el dólar, es el muro.** Por eso "máximo valor" = empujar cada tarea al modelo **más barato-en-cuota que supere el umbral de calidad**, y reservar premium para lo que de verdad lo necesita.

### 6.2 Ranking de valor (solo modelos "capaces", CI alto)

`Throughput capaz` = cuota/5h × índice de capacidad (CI, proxy de SWE-Pro/standing). Ignora a los workers puros (Flash/MiMo V2.5), que juegan en otra liga de volumen.

| # | Modelo | CI aprox | Cuota/5h | Throughput capaz | $ in/out | Lectura |
|---|---|---|---|---|---|---|
| 1 | **Qwen3.7 Plus** | 0.88 | 4.300 | ~3.784 | 0.40/1.60 | Mejor throughput capaz **sin promo** · multimodal |
| 2 | **DeepSeek V4 Pro** | 0.90 | 3.450 | ~3.105 | 1.74/3.48 | Mejor razonamiento del cluster |
| 3 | **MiniMax M2.7** | 0.85 | 3.400 | ~2.890 | 0.30/1.20 | Barato + rápido |
| 4 | **MiniMax M3** *(base)* | 0.90 | 3.200 | ~2.880 | 0.30/1.20 | Mismo precio que M2.7, **más calidad** |
| 5 | **MiMo V2.5 Pro** | 0.87 | 3.250 | ~2.828 | 1.74/3.48 | Token-eficiente |
| 6 | **Qwen3.6 Plus** | 0.84 | 3.300 | ~2.772 | 0.50/3.00 | Frontend |
| 7 | **Kimi K2.7 Code** | 0.85* | 1.350 | ~1.148 | 0.95/4.00 | *CI sin validar* |
| 8 | **Kimi K2.6** | 0.88 | 1.150 | ~1.012 | 0.95/4.00 | Calidad probada |
| 9 | **Qwen3.7 Max** | 0.93 | 950 | ~884 | 2.50/7.50 | Solo para lo crítico |
| 10 | **GLM-5.2** | 0.95 | 880 | ~836 | 1.40/4.40 | El cerebro |

> Los CI son heurísticos (proxy de capacidad para ordenar valor), no medidas exactas. El throughput es la señal, no el decimal.

### 6.3 Dominancias por la regla "empate → calidad"

Aplicar tu regla colapsa varios modelos a "solo overflow":

- **GLM-5.1 ≺ GLM-5.2** — mismo precio y cuota, peor score. *(ya conocido)*
- **MiniMax M2.7 ≺ MiniMax M3** — **precio idéntico** ($0.30/$1.20), cuota casi igual (3.400 vs 3.200), pero M3 gana en SWE-Pro (59.0 vs 56.2), contexto (1M vs 204K), multimodal y BrowseComp #1. Por tu regla, **M3 domina a M2.7**. → M2.7 queda como *overflow de M3* y para **latencia pura** (100 TPS, su único nicho real).
- **Worker tie (V4 Flash ≈ MiMo V2.5)** — mismo $0.14/$0.28 y ~30k/5h. No es empate puro: V4 Flash es algo mejor en **código** (úsalo para grunt de código); MiMo V2.5 gana en **multimodal + 1M ctx** (úsalo para grunt sobre archivos enormes o con imágenes).
- **Kimi K2.6 vs K2.7 Code** — K2.7 tiene más cuota (1.350) y −30% tokens, pero **sin benchmarks independientes**. Bajo incertidumbre, la regla "calidad" favorece lo **probado**: **K2.6** hasta que valides K2.7 en tu repo.

### 6.4 Escenario A — MiniMax M3 SIN promo (estado normal)

M3 = 3.200/5h · 16.000/mes. Aquí **no hay un único rey**: el cluster capaz (Q3.7 Plus, V4 Pro, M3, M2.7) tiene throughput parecido, así que **repartes por dominio y desempatas por calidad**:

| Carga | Modelo max-valor | Razón |
|---|---|---|
| Backend / agéntico / MCP / data | **MiniMax M3** | Mejor calidad al mismo precio que M2.7; pero ojo a su muro de 3.200/5h |
| Frontend | **Qwen3.7 Plus** | Mayor throughput capaz + multimodal |
| Rust / debug / comprensión repo | **DeepSeek V4 Pro** | Razonamiento; cuota holgada |
| Tests / overflow mid | **MiniMax M2.7** | Absorbe lo que no entra en M3 |
| Workers | **V4 Flash** + MiMo V2.5 | Volumen |
| Premium (10-15%) | **GLM-5.2** / Q3.7 Max | Solo lo duro |

**Sin promo debes balancear**: si metes todo a M3 chocas su pared a las ~3.200 llamadas. Reparte mid-load entre M3 → Q3.7 Plus → V4 Pro → M2.7.

### 6.5 Escenario B — MiniMax M3 CON promo x3 (AHORA)

M3 salta a **9.600/5h · 48.000/mes**. Recalculando su throughput capaz: 9.600 × 0.90 ≈ **8.640** → **2,3× el de Qwen3.7 Plus** y líder absoluto entre modelos capaces, **manteniendo** SWE-Pro 59.0 a $0.30/$1.20.

**Consecuencia:** durante la promo, M3 se comporta casi como un "tier capaz semi-ilimitado". El muro de 9.600/5h es difícil de chocar en uso normal → puedes **dejar de repartir mid-load y consolidar casi todo en M3**.

**Reasignación (sin promo → con promo):**

| Carga | Sin promo | **Con promo x3** |
|---|---|---|
| Backend / agéntico / data / CI | M3 *(con cuidado del muro)* | **M3 sin freno** |
| Tests | M2.7 | **M3** (mejor calidad, ya no falta cuota) |
| Comprensión de repo (no-crítica) | V4 Pro | **M3** (1M ctx, gratis en cuota) |
| Overflow de workers | MiMo V2.5 | **M3** absorbe el grunt "semi-capaz" |
| Frontend | Q3.7 Plus | **Q3.7 Plus** *(se queda: calidad web > M3)* |
| Rust-heavy / debug profundo | V4 Pro | **V4 Pro** *(se queda: razonamiento > M3)* |
| Premium duro | GLM-5.2 / Q3.7 Max | **igual** |

Regla práctica de la promo: **"¿M3 lo hace bien? → M3. Si no, escala."** Workers solo para bulk trivial; V4 Pro/GLM-5.2 solo donde calidad > M3 de forma clara (Rust fino, frontend pulido, web3, math, arquitectura, audit).

### 6.6 Stack max-valor por defecto

**CON promo x3 (ahora):**
- **Caballo de batalla (todo lo mid):** MiniMax M3 ⚡ — backend, data, CI, tests, comprensión, docs con razonamiento, parte del grunt
- **Frontend:** Qwen3.7 Plus → pulido GLM-5.2
- **Rust / debug profundo:** DeepSeek V4 Pro
- **Workers (bulk trivial):** V4 Flash · MiMo V2.5 (multimodal)
- **Premium (≤10%):** GLM-5.2 (cerebro/web3/math/arq) · Q3.7 Max+V4 Pro (audit dual)
- **Banca:** M2.7 (solo latencia/overflow) · GLM-5.1, K2.7 Code (no, hasta validar)

**SIN promo (cuando termine):**
- Igual, pero **rebalancea el mid-load** de vuelta: M3 (cuidando su muro) + Q3.7 Plus + V4 Pro + M2.7 repartidos por dominio. Mueve tests a M2.7 y comprensión-de-repo a V4 Pro para no agotar M3.

### 6.7 Dónde NO maximizar valor (calidad manda, no escatimes cuota)

Aunque optimices costo-beneficio en el 85% del flujo, hay tareas donde **bajar de modelo te cuesta más de lo que ahorras**:

- **Auditoría / security** → dual Q3.7 Max + V4 Pro siempre. Un bug perdido en un contrato > toda la cuota premium del mes.
- **Web3 crítico** (lógica de contrato, manejo de fondos) → GLM-5.2 / Q3.7 Max.
- **Orquestador / arquitectura** → GLM-5.2 (Max). Pocas llamadas, máximo impacto: es el modelo que decide a quién delega todo lo demás.
- **Generación de contenido PAES con math** → GLM-5.2 (AIME 99.2); un error matemático propagado a miles de alumnos no es "ahorro".

En estas, la regla "empate → calidad" se vuelve "**ante la duda → calidad**".
