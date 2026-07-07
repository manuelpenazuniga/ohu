import {
  LOTE,
  VAULT,
  NETWORK,
  OPERATOR,
  ADMIN,
  AGENTS,
  MUTUAL,
  shortHash,
  explorerUrl,
  type LoteStep,
  type Agent,
} from "./data.js";

/** Enlace monospace a una tx en el explorer (nueva pestaña). */
function txLink(tx: string): string {
  return `<a class="tx" href="${explorerUrl(tx)}" target="_blank" rel="noopener noreferrer" title="${tx}">${shortHash(tx)}</a>`;
}

/** Un paso del stepper. Los pasos de agente se resaltan. */
function stepCard(s: LoteStep): string {
  const agent = s.kind === "agent";
  return `
    <li class="step ${agent ? "step--agent" : "step--setup"}">
      <div class="step__dot" aria-hidden="true"></div>
      <div class="step__body">
        <div class="step__head">
          <span class="step__state">${s.state}</span>
          ${agent ? `<span class="badge">AGENT · ${s.column}</span>` : ""}
        </div>
        <code class="step__ep">${s.entrypoint}()</code>
        <div class="step__meta"><span class="actor actor--${s.actor}">${s.actor}</span> ${txLink(s.tx)}</div>
      </div>
    </li>`;
}

/** Una columna del feed del enjambre. */
function swarmColumn(
  column: "PROPONE" | "AUTORIZA",
  role: string,
  account: string,
): string {
  const step = LOTE.steps.find((s) => s.column === column)!;
  return `
    <div class="swarm__col swarm__col--${column.toLowerCase()}">
      <div class="swarm__label">${column}</div>
      <div class="swarm__role">${role}</div>
      <code class="swarm__acct" title="${account}">${shortHash(account.replace("account-hash-", ""), 8)}</code>
      <div class="swarm__action">
        <code>${step.entrypoint}()</code>
        <span class="arrow">→</span>
        <span class="result">${step.result}</span>
      </div>
      ${txLink(step.tx)}
    </div>`;
}

/** Tarjeta de un agente del enjambre (live o roadmap). */
function agentCard(a: Agent): string {
  const acct = a.account
    ? `<code class="ag__acct" title="${a.account}">${shortHash(a.account.replace("account-hash-", ""), 8)}</code>`
    : `<code class="ag__acct ag__acct--none">— sin cuenta aún —</code>`;
  const last = a.lastAction
    ? `<div class="ag__last">${a.lastAction}${a.lastTx ? ` ${txLink(a.lastTx)}` : ""}</div>`
    : "";
  return `
    <div class="ag ag--${a.status}">
      <div class="ag__top">
        <span class="ag__name">${a.name}</span>
        <span class="ag__status ag__status--${a.status}">${a.status === "live" ? "LIVE" : "ROADMAP"}</span>
      </div>
      <div class="ag__role">${a.role}</div>
      ${acct}
      <p class="ag__does">${a.does}</p>
      <div class="ag__auth">${a.authority}</div>
      ${last}
    </div>`;
}

/** Sección de la mutual: reserva + primas + cola pagada. */
function mutualSection(): string {
  const { reserveCspr, premiumsCspr, tailPaidCspr, premiumEvents, note, pool } = MUTUAL;
  return `
  <section class="card">
    <div class="card__head"><h2>The mutual</h2><span class="hint">parametric backstop</span></div>
    <a class="vault" href="https://testnet.cspr.live/contract-package/${pool}" target="_blank" rel="noopener noreferrer">MutualPool · ${shortHash(pool.replace("hash-", ""), 8)}</a>
    <div class="mutual__stats">
      <div class="stat"><span class="stat__n">${reserveCspr}</span><span class="stat__l">reserve · CSPR</span></div>
      <div class="stat"><span class="stat__n">${premiumsCspr}</span><span class="stat__l">premiums in</span></div>
      <div class="stat"><span class="stat__n stat__n--good">${tailPaidCspr}</span><span class="stat__l">tail paid</span></div>
    </div>
    <div class="mutual__bar" role="img" aria-label="reserva alimentada por primas, cola pagada cero">
      ${premiumEvents.map((e) => `<span class="seg" style="flex:${e.cspr}" title="lote #${e.lote}: +${e.cspr} CSPR de prima"></span>`).join("")}
      <span class="seg seg--empty" style="flex:0.4" title="capacidad de cola sin usar (tail = 0)"></span>
    </div>
    <p class="note">${note}</p>
  </section>`;
}

function render(): string {
  return `
  <header class="hero">
    <div class="hero__brand">Ohu</div>
    <h1 class="hero__lema">The LLM orchestrates; the contract authorizes.</h1>
    <p class="hero__sub">Swarm Control Room · <span class="net">${NETWORK}</span></p>
  </header>

  <section class="card">
    <div class="card__head">
      <h2>Batch #${LOTE.id}</h2>
      <div class="chips">
        <span class="chip">funded ${LOTE.funded}</span>
        <span class="chip">bond ${LOTE.bond}</span>
        <span class="chip">premium ${LOTE.premiumBps / 100}%</span>
        <span class="chip">quorum-fail ${LOTE.quorumFailBps / 100}%</span>
      </div>
    </div>
    <a class="vault" href="https://testnet.cspr.live/contract-package/${VAULT}" target="_blank" rel="noopener noreferrer">OhuVault · ${shortHash(VAULT.replace("hash-", ""), 8)}</a>
    <ol class="stepper">
      ${LOTE.steps.map(stepCard).join("")}
    </ol>
  </section>

  <section class="card swarm">
    <div class="card__head"><h2>The swarm</h2><span class="hint">3 agents · separated authority</span></div>
    <div class="ag__grid">
      ${AGENTS.map(agentCard).join("")}
    </div>
    <div class="swarm__sub">Last round · PROPONE → AUTORIZA</div>
    <div class="swarm__grid">
      ${swarmColumn("PROPONE", "Tesorería · operator", OPERATOR)}
      <div class="swarm__link" aria-hidden="true">→</div>
      ${swarmColumn("AUTORIZA", "Autorizador · admin", ADMIN)}
    </div>
    <p class="note">The <strong>operator can only evaluate</strong> — authorized by the on-chain tally (INV-2), it cannot move capital. Capital is executed by the <strong>admin</strong> (INV-1). Even a jailbroken operator can't pay out: <code>release_to_producer</code> reverts with <code>NotAdmin</code> for anyone but the admin identity.</p>
  </section>

  ${mutualSection()}

  <footer class="foot">
    Every hash is a real transaction on Casper Testnet · <code>error_message=None</code> · lote #${LOTE.id} settled hands-free by the agent swarm.
  </footer>`;
}

const app = document.getElementById("app");
if (app) {
  app.innerHTML = render();
  app.setAttribute("aria-busy", "false");
}
