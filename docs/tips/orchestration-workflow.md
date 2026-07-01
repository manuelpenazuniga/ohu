# Workflow del orquestador — cómo Claude coordina agentes en Ohu

> Para el Claude (u otro orquestador) que retome este repo. Describe **cómo trabajo**: qué hago yo
> vs qué delego, qué modelo/CLI uso para cada cosa, la disciplina de auditoría, y los gotchas que
> aprendí en carne propia. Complementa `model-routing.md` (economía/asignación) y `agy-cli.md`
> (sintaxis de agy). Aquí va el *método*, no solo las herramientas.

---

## 0. Regla de oro: yo planifico, brifeo, audito e integro; los agentes implementan

- **Yo (Claude, main loop):** entiendo el problema, diseño la solución/máquina de estados, escribo el
  brief, **audito el resultado leyendo el código**, integro (merge), y llevo el estado (`ESTADO.md`).
- **Agentes CLI (opencode/agy):** escriben el código de la tarea en un **worktree aislado**.
- **NO delego:** el diseño, la decisión de merge, y la auditoría de lo que toca fondos. Eso lo hago yo
  + un panel de modelos.
- **Excepción:** si un agente se atasca en internals sutiles (p.ej. cómo firmar como cuenta fondeada
  en odra-test), lo resuelvo yo directamente. La regla "los agentes implementan" cede ante "no
  quedarse bloqueado".

---

## 1. Los tres CLIs y para qué uso cada uno

| CLI | Rol | Invocación (validada) |
|---|---|---|
| **opencode** (opencode-go) | **Implementar** (trabajo pesado) | `opencode run --dir <worktree> -m opencode-go/<modelo> "$MSG"` |
| **agy** (Antigravity) | **Auditar** (Gemini) o tareas simples | `agy --model "Gemini 3.1 Pro (High)" --add-dir <src> --print-timeout 900s -p "$MSG"` |
| **codex** (OpenAI) | **Auditoría adversarial** (GPT-5.5) | `codex exec -s read-only -m gpt-5.5 -c mcp_servers="{}" < brief.txt` |

Reglas duras aprendidas:
- **opencode:** SIEMPRE `opencode-go/<modelo>` (nunca el provider `zen`). Pasa el prompt **inline**
  (`"$MSG"` con `MSG="$(cat brief.txt)"`), NO con `-f` (es array-greedy y traga el mensaje como
  filename). No necesita `--dangerously-skip-permissions` (ejecuta tools solo en modo no-interactivo).
- **agy:** el prompt inline con `-p`; `--add-dir` **acotado a `contracts/src`** (NO el worktree entero
  con `target/` → cuelga); `--print-timeout 900s` (el default de 5m corta audits largos). Para
  **auditar**, NO pases `--dangerously-skip-permissions` (read-only). Ver `agy-cli.md`.
- **codex:** el prompt va por **stdin** (`< brief.txt`), NO como arg (si lo pasas como arg imprime
  "Reading additional input from stdin..." y no arranca). `-s read-only` para auditar. `-m gpt-5.5`
  (NO `gpt-5.5-codex` en cuenta ChatGPT). **Cuota fluctúa** → preguntar al usuario antes de usar
  (ver memoria `gpt5-quota-ask-first`).

---

## 2. Routing de modelos — qué uso para cada cosa

| Tarea | Modelo / CLI | Por qué |
|---|---|---|
| **Implementar contratos** (Odra/Rust, toca fondos) | **DeepSeek V4 Pro** vía opencode | Mejor en Rust/lógica compleja del pool de barato |
| Infra / scripts / deploy | **MiniMax M3** o **Gemini 3.5 Flash (High)** vía opencode/agy | Terminal-heavy, tareas acotadas |
| Tareas simples / docs / smoke-tests | **Gemini 3.5 Flash (Med/High)** vía agy | Rápidas, las hace bien; no para lógica sofisticada |
| **Auditor primario** (seguridad/lógica) | **Gemini 3.1 Pro (High)** vía agy | El más capaz para razonar invariantes |
| **Auditor adversarial de cierre** (fondos) | **GPT-5.5** vía codex | Framing "búscalo con saña" — caza lo económico/juego-teórico |
| Diseño, brief, auditoría de fondos, merge | **Claude (yo)** | No se delega |

Retirados por caros (quemaron una cuenta en 1 día): Qwen 3.7 Max, GLM 5.2. Ver `model-routing.md`.

---

## 3. La disciplina de auditoría (lo más importante)

**Nivel de auditoría por tipo de cambio:**
- **No toca fondos** (docs, tests, atestación sin settlement) → **dual: Claude + Gemini**.
- **Toca fondos** (release, settle, pool, refund) → **triple: Claude + Gemini + GPT-5.5**.
- **Antes de CUALQUIER deploy a Testnet** → **pase holístico de cierre** con familia distinta (GPT-5.5).

**Por qué el panel multi-modelo NO es redundante — la asimetría de lentes:**
Cada modelo caza una clase distinta de bug. Verificado dos veces en este proyecto:
- **Claude (yo) = lente de conservación/estructura:** "¿los motes cuadran? ¿CEI? ¿checked_*?".
- **GPT-5.5 = lente adversarial/juego:** "¿quién puede *ganar dinero* rompiendo la regla del juego?".
- **Gemini = lente de correctness + prueba algebraica.**

Casos reales donde esto salvó el proyecto:
1. **W1 escrow-isolation:** 120 tests verdes + mi análisis por-tarea limpio, PERO el pase holístico
   GPT-5.5 halló que outflows viejos drenaban escrow earmarked. (Memoria `w1-escrow-isolation-critical`.)
2. **W2-3 MutualPool:** 176 tests verdes + mi conservación cerraba perfecto, PERO GPT (framing
   adversarial) halló que un anillo con bono de 1 mote drenaba el pool. La conservación es *ciega a la
   economía del juego repetido*. Y en la re-auditoría GPT cazó que MI PROPIO fix introdujo un lock de
   fondos que yo y Gemini dimos por PASA.

**Lección:** un contrato que *compila y conserva fondos* NO es un contrato *que no se puede saquear*.
El auditor que menos deferencia le tiene a mis suposiciones es el que más valor da. NUNCA saltarse el
pase adversarial antes de tocar Testnet.

---

## 4. El ciclo por tarea (paso a paso)

1. **Entender + diseñar.** Leo el spec (`ohu.md`/`techs-specs.md`), el código actual, y **diseño yo la
   máquina de estados / la solución** para que el brief sea inequívoco. No delego el diseño.
2. **Worktree aislado.** `git worktree add ../ohu-wNN -b spike/wNN-tarea main`. Uno por tarea. Evita
   que los agentes se pisen y mantiene `main` limpio.
3. **Brief.** Uso el wrapper (`docs/plan/_wrapper.md`) + el brief específico. Claves del brief:
   - Invariantes aplicables (INV-1..7) inline.
   - **Regla anti-alucinación:** "no inventes APIs; si dudas deja `// TODO(audit): verificar contra
     <doc>`. Un hueco marcado > una API inventada."
   - Criterios de aceptación **al pie de la letra** + **tests NEGATIVOS** obligatorios.
   - "SERÁS AUDITADO contra X" — hace que el agente se autocontrole.
   - Decisiones de diseño ya tomadas (no dejar que el agente improvise lo económico).
4. **Lanzar** en background (`run_in_background: true`) con `tee`/`>` a un log.
5. **Verificar SIEMPRE (nunca confiar en el claim del agente).** Corro yo: `cargo odra test`,
   `cargo clippy --all-targets`, `cargo odra build`, y **leo la lógica crítica** (el gate de fondos,
   la aritmética). Los agentes a veces dicen "hecho" con tests rojos o sin commitear.
6. **Auditar** según el nivel (§3). Los auditores en **paralelo** (background).
7. **Convergencia → merge.** Si el panel converge en PASA, `git merge --ff-only`, `git worktree
   remove`, `git branch -d`, `git push`. Si un auditor da NO-PASA con un hallazgo real, **ronda de
   fix** (vuelve a 3) — no mergear.
8. **Actualizar `ESTADO.md`** (header, roadmap, gates) + memoria si hay lección. Commit + push.

---

## 5. Gotchas operativos (me costaron tiempo real)

- **Detección de cuelgue de agentes:** compara **CPU acumulada vs reloj**. `ps -o pid,etime,time,%cpu
  -p <pid>`. Si lleva 30+ min de reloj pero **segundos de CPU** y `%cpu~0`, está **colgado**
  (esperando al backend que no responde), no trabajando. Pasó con agy (31 min) y DeepSeek (41 min).
  **Watchdog:** lanza en background `sleep 540; ps ...; git log` — te notifica a los ~9 min para
  supervisar sin que se te pasen 40. Al detectar cuelgue: `kill <pid>` + re-lanzar.
- **NUNCA confiar en "exit 0"** de instalaciones: verifica el binario (`which`, `--version`). `cargo
  install` puede dar exit 0 y no instalar (timeout de red).
- **Verificar el commit del agente:** a veces hace el trabajo pero NO commitea (queda en working tree).
  Revisa `git log` + `git status`; commitea tú si hace falta (kill del entorno puede perderlo).
- **zsh, no bash:** el shell es zsh → `${!var}`, `${arr[@]}` bash-isms fallan ("bad substitution").
  Usa formas portables o `bash -c`.
- **Paths con espacios** (p.ej. claves de wallet) → citar SIEMPRE en `.env` y comandos.
- **Reinicios del entorno** scramblean el transcript: si algo no cuadra, re-oriéntate con el estado
  REAL (`git log`, worktrees, `ps`), no con tu narrativa. El repo/GitHub es la verdad, no tu memoria.
- **Auditores externos fallan de formas propias:** agy (Gemini) **rechaza** el framing "pentest/
  security audit" → re-frasea como "revisa MI código pre-merge por bugs de correctness/conservación".
  agy **se cuelga** con `--add-dir` amplio → acota a `contracts/src`.

---

## 6. Verificar en el nodo real, no solo en odra-test

`odra-test` corre en un VM nativo que **no preprocesa el WASM ni serializa igual que el nodo**. Bugs
invisibles a los tests que solo el Testnet expone:
- **bulk-memory/sign-ext** en el WASM (toolchain moderno) → el nodo Casper los rechaza. Fix: `wasm-opt`
  lowering a MVP (ver `deploy_testnet.sh`).
- **`transfer_tokens` a un `Address::Contract` revierte** en Casper WASM (odra-test NO lo caza). Fondos
  cross-contract solo a **cuentas**. (Memoria `odra-transfer-to-contract-reverts`.)
- Formato de clave (secp256k1 vs ed25519), multi-key signing, args de init — solo el nodo real valida.

Por eso el **deploy + E2E real en Testnet** es un gate distinto de "tests verdes", no un trámite.

---

## 7. Resumen en una frase

*Diseño yo, implementan los agentes baratos, y **audita un panel de modelos con lentes distintos**
antes de tocar dinero real — porque ningún modelo (yo incluido) ve todas las clases de bug, y "compila
y conserva" no es "no se puede saquear".*
