# Cómo usar `agy` (Antigravity CLI) — worker y auditoría de seguridad

> Guía operativa **verificada empíricamente** (agy `1.0.14`, 2026-06-30) para invocar agy desde la
> línea de comandos: cada modelo, y sus dos funciones (implementar = *worker*, auditar = *read-only*).
> Si algo aquí no coincide con `agy --help` / `agy models`, **gana la salida real del binario** — re-verifica.

---

## 0. TL;DR (copia-pega)

```bash
# AUDITORÍA (read-only): NO lleva --dangerously-skip-permissions. Timeout amplio.
MSG="$(cat /ruta/al/brief-audit.txt)"
agy --model "Gemini 3.1 Pro (High)" \
    --add-dir /ruta/al/worktree \
    --print-timeout 900s \
    -p "$MSG"

# WORKER (escribe/edita): SÍ lleva --dangerously-skip-permissions (si no, en print mode auto-rechaza writes).
MSG="$(cat /ruta/al/brief-impl.txt)"
agy --model "Gemini 3.5 Flash (High)" \
    --add-dir /ruta/al/worktree \
    --dangerously-skip-permissions \
    --print-timeout 900s \
    -p "$MSG"
```

**Las 3 cosas que rompen a la gente** (probablemente tu otro agente):
1. **Nombre de modelo inexacto.** El `--model` exige el string EXACTO de `agy models`, **con el
   paréntesis y mayúsculas**: `"Gemini 3.1 Pro (High)"`, no `gemini-3.1-pro` ni `Gemini 3.1 Pro`.
2. **Timeout corto.** `--print-timeout` por defecto es **5m**. Una auditoría de un contrato grande
   tarda más → se corta a media respuesta. Súbelo a `600s`–`900s` (o más para audits triples).
3. **Permisos en print mode.** En `-p` (no interactivo) no hay forma de aprobar prompts de permiso:
   - **Worker** (necesita escribir): pasa `--dangerously-skip-permissions` o auto-rechaza los writes.
   - **Auditor** (solo lee): **NO** lo pases. Las **lecturas** del `--add-dir` se permiten solas; así
     el modelo no puede modificar nada aunque el prompt se lo pidiera por error.

---

## 1. Qué es y dónde está

- Binario: `~/.local/bin/agy` (`command -v agy`). Versión: `agy --version`. Update: `agy update`.
- Es el CLI de **Antigravity**: una sola sesión-modelo por invocación, con acceso a un *workspace*
  (directorios que tú añades) y herramientas (leer/editar archivos, correr comandos según permisos).
- Plan relativamente generoso → úsalo como **auditor primario** (Gemini 3.1 Pro High) y para **tareas
  simples** (Gemini 3.5 Flash). Detalle de routing del proyecto en `docs/plan/model-routing.md`.

---

## 2. Anatomía del comando (flags que importan)

| Flag | Para qué |
|---|---|
| `--model "<EXACTO>"` | Modelo de la sesión. String exacto de `agy models` (ver §3). |
| `-p` / `--print` / `--prompt` | **Modo no-interactivo**: corre UN prompt y imprime la respuesta a stdout. Es el modo para automatizar. |
| `--add-dir <ruta>` | Añade un directorio al workspace (lo puede leer/editar). **Repetible.** Usa rutas **absolutas**. |
| `--print-timeout <dur>` | Cuánto espera en print mode. Default `5m0s`. **Súbelo** para audits (`900s`). Acepta `s`/`m` (`600s`, `15m`). |
| `--dangerously-skip-permissions` | Auto-aprueba TODA herramienta (incl. escribir/editar/ejecutar). **Solo para worker.** |
| `--sandbox` | Corre con restricciones de terminal (sandbox). Útil para limitar a un worker. |
| `-i` / `--prompt-interactive` | Arranca interactivo (NO para automatizar). |
| `-c` / `--continue` | Continúa la conversación más reciente. |
| `--conversation <ID>` | Reanuda una conversación por ID. |
| `--log-file <ruta>` | Redirige el log del CLI. |

Subcomandos útiles: `agy models` (lista modelos), `agy update`, `agy changelog`, `agy plugin list`.

### Reglas de oro de invocación
- **Pasa el prompt inline** como string citado: `-p "$MSG"` con `MSG="$(cat brief.txt)"`. No hay un
  `-f archivo` fiable; arma el mensaje con `cat` por command-substitution.
- **Rutas absolutas** en `--add-dir` (el CWD del proceso puede no ser el que crees).
- La **respuesta del modelo va a stdout**; captúrala con `| tee run.log` o `2>&1 | tail`.
- Para audits largos, **lánzalo en background** (ver §6) y recoge el resultado al terminar.

---

## 3. Modelos disponibles (string exacto) y cuándo usar cada uno

`agy models` (2026-06-30) devuelve exactamente:

| String exacto para `--model` | Rol recomendado en Ohu |
|---|---|
| `"Gemini 3.1 Pro (High)"` | **Auditor primario** (seguridad de contratos, lógica de fondos). El más capaz para razonar invariantes. |
| `"Gemini 3.1 Pro (Low)"` | Auditor/worker cuando quieres Pro pero más barato/rápido; revisiones de alcance medio. |
| `"Gemini 3.5 Flash (High)"` | **Worker** de tareas claras (scripts, refactors acotados, docs). Audita cosas simples. |
| `"Gemini 3.5 Flash (Medium)"` | Worker/lecturas rápidas, resúmenes, chequeos puntuales. |
| `"Gemini 3.5 Flash (Low)"` | Tareas triviales / smoke-tests del propio agy. **No** para nada sofisticado. |
| `"Claude Sonnet 4.6 (Thinking)"` | Alternativa de auditor/worker con razonamiento; segunda opinión a Gemini. |
| `"Claude Opus 4.6 (Thinking)"` | Razonamiento máximo vía agy (si lo quieres como tercer ojo). |
| `"GPT-OSS 120B (Medium)"` | Modelo abierto; útil para diversidad de familia en un panel. |

> Política del proyecto: **Gemini 3.1 Pro (High) = auditor primario**; **Gemini 3.5 Flash (Med/High) =
> tareas simples** (las hace bien, pero no algo muy sofisticado). Para lo que toca fondos, el audit es
> **triple** (Claude + Gemini-vía-agy + GPT-5.5-vía-codex) — ver `model-routing.md`.

---

## 4. Función AUDITORÍA DE SEGURIDAD (read-only)

**Objetivo:** que el modelo lea el código y **reporte** hallazgos, sin tocar nada.

**Regla clave:** NO pases `--dangerously-skip-permissions`. Las lecturas del `--add-dir` se permiten
solas; el modelo no podrá escribir aunque el prompt fallara. Timeout amplio.

```bash
AUDIT_BRIEF="$(cat <<'EOF'
Eres auditor de contratos Odra/Casper. NO modifiques ningún archivo: solo LEE y REPORTA.
Audita contracts/src/ohu_vault.rs centrándote en: <foco concreto>.
Verifica punto por punto: 1) ... 2) ... 3) ...
Reporta por cada punto OK o PROBLEMA (con archivo:línea). Termina con:
VEREDICTO: PASA / NO-PASA + razón en una línea.
EOF
)"

agy --model "Gemini 3.1 Pro (High)" \
    --add-dir /Volumes/MacMiniExt/dev/web3/ohu/ohu/contracts \
    --print-timeout 900s \
    -p "$AUDIT_BRIEF" 2>&1 | tee /tmp/agy-audit.log
```

**Plantilla de prompt de auditoría** (lo que hace que el reporte sea útil):
- Frase explícita: *“NO modifiques nada; solo lee y reporta”.*
- **Foco acotado** (qué invariante/función), no “audita todo”.
- **Checklist numerado** de lo que debe verificar.
- **Formato de salida fijo**: `OK/PROBLEMA` con `archivo:línea` + `VEREDICTO: PASA/NO-PASA`.
- Si auditas un **fix**, dale el contexto del bug original y pídele que confirme cierre + regresiones.

---

## 5. Función WORKER (implementa / edita)

**Objetivo:** que el modelo escriba o modifique código de forma autónoma.

**Regla clave:** SÍ pasa `--dangerously-skip-permissions` (en print mode, sin esto, los writes se
auto-rechazan y el agente “no hace nada”). Trabaja en un **worktree aislado**, no en `main`.

```bash
git worktree add ../ohu-wt -b spike/mi-tarea main      # aísla el trabajo

IMPL_BRIEF="$(cat <<'EOF'
Trabajas en el worktree spike/mi-tarea. Implementa <X>. NO toques <Y>.
Antes de codear lee <archivos>. Al terminar: corre `cargo test`, deja verde, y HAZ COMMIT.
Resume qué cambiaste y cualquier supuesto. Marca // TODO(verify) lo que no puedas confirmar.
EOF
)"

agy --model "Gemini 3.5 Flash (High)" \
    --add-dir /Volumes/MacMiniExt/dev/web3/ohu/ohu-wt \
    --dangerously-skip-permissions \
    --print-timeout 900s \
    -p "$IMPL_BRIEF" 2>&1 | tee /tmp/agy-impl.log

# auditar SIEMPRE lo que produjo (con otro modelo/familia) antes de mergear.
```

> Para Ohu, el **trabajo pesado de contratos** suele ir por opencode (DeepSeek V4 Pro / MiniMax M3 /
> Kimi), y agy entra sobre todo como **auditor** (Gemini 3.1 Pro High) o **worker de tareas simples**
> (Flash). Pero agy puede ser worker perfectamente si el `--dangerously-skip-permissions` está puesto.

---

## 6. Ejecución en background (audits largos)

Un audit serio puede tardar minutos. Lánzalo en background y recoge al terminar (no bloquees):

```bash
MSG="$(cat /tmp/brief.txt)"
agy --model "Gemini 3.1 Pro (High)" --add-dir <wt> --print-timeout 900s -p "$MSG" \
    > /tmp/agy-out.log 2>&1 &
# ... seguir con otra cosa; luego: tail -40 /tmp/agy-out.log
```

(En esta sesión de Claude Code: usa `run_in_background: true` en la tool Bash y lee el archivo de salida.)

---

## 7. Troubleshooting (síntoma → causa → fix)

| Síntoma | Causa probable | Fix |
|---|---|---|
| “unknown/invalid model” o ignora el modelo | string de `--model` inexacto | usa el string EXACTO de `agy models` con `(High)`/`(Low)` |
| La respuesta se **corta** / vacía a los ~5 min | `--print-timeout` por defecto (5m) | `--print-timeout 900s` (o más) |
| **Se CUELGA** (30+ min, 0 salida, ~0% CPU, estado `S`) | `--add-dir` apuntando a un dir enorme (arrastra `target/`, `node_modules/`) → agy se atasca indexando, o el backend no responde | **Acota `--add-dir` a la carpeta de fuentes** (p.ej. `contracts/src`, no el worktree entero). Verificado: un audit que colgó 31 min con el worktree completo corrió bien con `--add-dir contracts/src`. Si igual cuelga, mátalo (`kill <pid>`) y reintenta — suele ser red. |
| El worker “no hizo nada”, solo describió | faltó `--dangerously-skip-permissions` en print | añádelo (solo worker) |
| “no encuentra el archivo” / no ve el código | falta `--add-dir` o ruta relativa | `--add-dir <ruta ABSOLUTA>` (repetible) |
| Cuelga esperando input | arrancó interactivo | usa `-p`/`--print`, no `-i` |
| El prompt se interpretó raro | se pasó por archivo/flag equivocado | inline: `-p "$MSG"` con `MSG="$(cat brief.txt)"` |
| Mezcla cambios con otra rama | corrió sobre `main` | trabaja en un `git worktree` aislado |

---

## 8. Encaje con el routing de Ohu

- **Auditor primario:** `agy --model "Gemini 3.1 Pro (High)"` (read-only, §4).
- **Tareas simples / worker ligero:** `agy --model "Gemini 3.5 Flash (Medium|High)"` (§5).
- **Triple audit de fondos:** Claude (yo) + esta receta agy + `codex exec -s read-only -m gpt-5.5`.
- Implementación pesada de contratos: **opencode-go** (`opencode run --dir <wt> -m opencode-go/<modelo>`).
- Ver `docs/plan/model-routing.md` para la economía de los pools y la asignación por tarea.
