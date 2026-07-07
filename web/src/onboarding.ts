// "El Almanaque" — onboarding (capítulo ii del demo): un restaurante se une a
// la cuadrilla en 4 pasos de siembra. Cada paso explica qué pasaría on-chain.
// El resultado se guarda en localStorage y lo saluda el Control Room.
import { navRibbon, seal, sprite, saveCrew } from "./ui.js";

interface Product {
  readonly id: string;
  readonly name: string;
  readonly unit: string;
}

const PRODUCTS: readonly Product[] = [
  { id: "tomato", name: "Tomatoes", unit: "kg" },
  { id: "lettuce", name: "Lettuce", unit: "heads" },
  { id: "carrot", name: "Carrots", unit: "bunches" },
  { id: "pumpkin", name: "Pumpkins", unit: "units" },
  { id: "orange", name: "Oranges", unit: "kg" },
  { id: "olives", name: "Olives", unit: "kg" },
  { id: "eggs", name: "Eggs", unit: "dozens" },
  { id: "cheese", name: "Cheese", unit: "wheels" },
];

/** Estado del wizard (vive en memoria; se siembra a localStorage al firmar). */
const state = {
  step: 0,
  name: "",
  demand: new Map<string, number>(),
  stamped: false,
};

// Deep-links de demo: ?step=3&name=Café%20Aroha&demand=tomato:20,eggs:4&stamped=1
{
  const q = new URLSearchParams(location.search);
  const name = q.get("name");
  if (name) state.name = name;
  for (const part of (q.get("demand") ?? "").split(",")) {
    const [id, qty] = part.split(":");
    if (id && PRODUCTS.some((p) => p.id === id)) state.demand.set(id, Math.max(0, Number(qty) || 0));
  }
  state.stamped = q.get("stamped") === "1" && state.name.length > 0;
  const step = Number(q.get("step") ?? "1");
  if (step >= 1 && step <= 4) {
    // no dejes saltar a un paso cuyo requisito no está sembrado
    const max = !state.name.trim() ? 1 : state.demand.size === 0 ? 2 : !state.stamped ? 3 : 4;
    state.step = Math.min(step, max) - 1;
  }
}

/** account-hash determinista de demo a partir del nombre (FNV-1a extendido). */
function demoAccountHash(name: string): string {
  let h = 0x811c9dc5;
  const out: string[] = [];
  for (let i = 0; out.length < 8; i++) {
    const c = name.charCodeAt(i % Math.max(name.length, 1)) || 111;
    h ^= c + i;
    h = Math.imul(h, 0x01000193) >>> 0;
    out.push(h.toString(16).padStart(8, "0"));
  }
  return `account-hash-${out.join("")}`;
}

const STEP_TITLES = ["Prepare the soil", "Sow your demand", "Sign the ledger", "Welcome to the ohu"];

/** El tui dice una cosa distinta en cada paso. */
const TUI_SAYS: readonly string[] = [
  "Kia ora! I'm the ohu's tui. First, tell the ledger who you are — the soil remembers.",
  "Now sow what your kitchen needs every week. Don't be shy — the crew buys better together.",
  "Read the terms like a farmer reads the sky. Then stamp it — ink is binding around here.",
  "Welcome to the crew! Your demand is sown into the next batch. Come see the season run.",
];

/** Surco de progreso: un hueco por paso; los completados brotan. */
function furrow(): string {
  const holes = STEP_TITLES.map((t, i) => {
    const cls = i < state.step ? "furrow__hole furrow__hole--grown" : i === state.step ? "furrow__hole furrow__hole--here" : "furrow__hole";
    const art = i < state.step ? sprite("sprout", 26, "done") : i === state.step ? sprite("seed", 26, "current") : "";
    return `<div class="${cls}" title="${t}">${art}<span class="furrow__label">${i + 1}. ${t}</span></div>`;
  });
  return `<div class="furrow" role="progressbar" aria-valuemin="1" aria-valuemax="4" aria-valuenow="${state.step + 1}" aria-valuetext="Step ${state.step + 1} of 4: ${STEP_TITLES[state.step]}">${holes.join("")}</div>`;
}

/** La mascota + bocadillo. */
function tui(): string {
  return `
  <div class="tui">
    <img class="tui__img" src="/img/mascot-tui.png" alt="" width="72" height="72" />
    <div class="tui__bubble">${TUI_SAYS[state.step]}</div>
  </div>`;
}

/** Cajita plegable "under the hood" — la explicación técnica honesta del paso. */
function hood(text: string): string {
  return `
  <details class="hood">
    <summary>Under the hood — what this really does</summary>
    <p>${text}</p>
  </details>`;
}

function step1(): string {
  const account = state.name ? demoAccountHash(state.name) : "";
  return `
  <div class="wiz__fields">
    <label class="field">
      <span class="field__label">Name of your kitchen / crew</span>
      <input id="crew-name" class="field__input" type="text" placeholder="e.g. Café Aroha" value="${state.name.replace(/"/g, "&quot;")}" autocomplete="off" />
    </label>
    <div class="field">
      <span class="field__label">Your Casper identity (demo)</span>
      <code class="field__hash${account ? "" : " field__hash--empty"}">${account || "— type a name and the soil will assign you a plot —"}</code>
    </div>
  </div>
  ${hood(
    "Your crew identity is an ordinary Casper <b>account</b> — not a custodial login. Keys stay on your device; the vault is a contract purse guarded by native multisig (INV-3). Ohu never holds your keys, so there is nothing for Ohu to lose.",
  )}
  <div class="wiz__nav">
    <a class="btn btn--ghost" href="/">← Back to the almanac</a>
    <button class="btn btn--primary" id="next" ${state.name.trim() ? "" : "disabled"}>Sow the name <span class="btn__arrow">→</span></button>
  </div>`;
}

function step2(): string {
  const rows = PRODUCTS.map((p) => {
    const qty = state.demand.get(p.id) ?? 0;
    return `
    <div class="prod${qty > 0 ? " prod--on" : ""}" data-id="${p.id}">
      <img class="prod__img" src="/img/product-${p.id}.png" alt="${p.name}" width="56" height="62" />
      <div class="prod__name">${p.name}</div>
      <div class="prod__unit">${p.unit} / week</div>
      <div class="prod__ctrl">
        <button class="prod__btn" data-act="dec" aria-label="less ${p.name}">−</button>
        <span class="prod__qty">${qty}</span>
        <button class="prod__btn" data-act="inc" aria-label="more ${p.name}">+</button>
      </div>
    </div>`;
  }).join("");
  const picked = [...state.demand.entries()].filter(([, q]) => q > 0);
  const crate = picked.length
    ? picked
        .map(([id, q]) => {
          const p = PRODUCTS.find((x) => x.id === id)!;
          return `<span class="crate-label">${p.name} · ${q} ${p.unit}</span>`;
        })
        .join("")
    : `<span class="crate-label crate-label--empty">the crate is still empty…</span>`;
  return `
  <div class="prod__grid">${rows}</div>
  <div class="crate-strip"><span class="crate-strip__title">Your weekly crate</span>${crate}</div>
  ${hood(
    "In the real product you'd just tell the Aggregator “20 kg of tomatoes for Friday” in plain language. Gemini only <b>normalizes</b> that into a spec — the batching itself is deterministic bin-packing, and the clearing price comes from a sealed RFQ. No capital ever moves on a model's judgment (INV-2).",
  )}
  <div class="wiz__nav">
    <button class="btn btn--ghost" id="back">← Back</button>
    <button class="btn btn--primary" id="next" ${picked.length ? "" : "disabled"}>Load the crate <span class="btn__arrow">→</span></button>
  </div>`;
}

function step3(): string {
  const account = demoAccountHash(state.name);
  const lines = [...state.demand.entries()]
    .filter(([, q]) => q > 0)
    .map(([id, q]) => {
      const p = PRODUCTS.find((x) => x.id === id)!;
      return `<div class="ledger__row"><span class="ledger__label">${p.name}</span><span class="ledger__dots"></span><span class="ledger__value">${q} ${p.unit} / week</span></div>`;
    })
    .join("");
  return `
  <div class="sign">
    <div class="sign__page">
      <div class="sign__head">Membership entry — Ohu Cooperative Ledger</div>
      <div class="ledger__row"><span class="ledger__label">Crew</span><span class="ledger__dots"></span><span class="ledger__value">${state.name}</span></div>
      <div class="ledger__row"><span class="ledger__label">Account</span><span class="ledger__dots"></span><span class="ledger__value ledger__value--hash">${account.slice(0, 34)}…</span></div>
      ${lines}
      <div class="sign__terms">
        <div class="ledger__row"><span class="ledger__label">Delivery window</span><span class="ledger__dots"></span><span class="ledger__value">Fridays 06:00–10:00</span></div>
        <div class="ledger__row"><span class="ledger__label">Mutual premium</span><span class="ledger__dots"></span><span class="ledger__value">0.5% per settled batch</span></div>
        <div class="ledger__row"><span class="ledger__label">Producer bond</span><span class="ledger__dots"></span><span class="ledger__value">≥ batch target</span></div>
        <div class="ledger__row"><span class="ledger__label">Fail quorum</span><span class="ledger__dots"></span><span class="ledger__value">60% weighted attestations</span></div>
      </div>
      <div class="sign__spot${state.stamped ? " sign__spot--done" : ""}" id="stamp-spot">
        ${
          state.stamped
            ? seal("ob-signed", "SIGNED INTO THE OHU", `${state.name.toUpperCase().slice(0, 12)}|2026-07-07`, "ED25519 · GASLESS", { tone: "clay", rotate: -5, size: 124 })
            : `<span class="sign__hint">the ink waits for you ↓</span>`
        }
      </div>
    </div>
    <button class="btn btn--stamp${state.stamped ? " btn--stamp-done" : ""}" id="stamp" ${state.stamped ? "disabled" : ""}>
      ${state.stamped ? "Stamped. Ink is binding." : "Press the stamp"}
    </button>
  </div>
  ${hood(
    "In production this button signs an <b>Ed25519 attestation</b> over the membership payload, right here in the browser — gasless for you, verified on-chain by the contract (INV-5). The same mechanism later confirms deliveries: stamps, not claims adjusters.",
  )}
  <div class="wiz__nav">
    <button class="btn btn--ghost" id="back">← Back</button>
    <button class="btn btn--primary" id="next" ${state.stamped ? "" : "disabled"}>Enter the ohu <span class="btn__arrow">→</span></button>
  </div>`;
}

function step4(): string {
  const picked = [...state.demand.entries()].filter(([, q]) => q > 0);
  return `
  <div class="done">
    <div class="done__badge">
      ${sprite("crate", 64, "")}
      <span class="done__pixel">CREW MEMBER · No. 27</span>
    </div>
    <h2 class="done__title">${state.name} is in the crew.</h2>
    <p class="done__text">Your ${picked.length} product${picked.length === 1 ? "" : "s"} are sown into <b>next week's batch #5</b>. The Aggregator will fold them into the RFQ tonight; escrow opens when the batch is full. From here, the season runs <b>hands-free</b> — watch the swarm do its work.</p>
    <div class="done__cta">
      <a class="btn btn--primary" href="/dashboard.html">Enter the control room <span class="btn__arrow">→</span></a>
      <a class="btn btn--ghost" href="/">Back to the almanac</a>
    </div>
  </div>
  ${hood(
    "What the demo just did: wrote your crew to <code>localStorage</code> so the Control Room greets you. What the product does at this point: <code>open_lote</code> on the OhuVault with your demand folded in — and every step after that is the exact on-chain trace you're about to see.",
  )}`;
}

const STEP_RENDER = [step1, step2, step3, step4];

function render(): string {
  return `
  ${navRibbon("onboarding")}
  <header class="hero hero--onboarding">
    <div class="hero__brand">Chapter ii — joining the crew</div>
    <h1 class="hero__lema">${STEP_TITLES[state.step]}</h1>
  </header>
  ${furrow()}
  ${tui()}
  <section class="card wiz">${STEP_RENDER[state.step]()}</section>`;
}

/** Re-render + listeners. Wizard pequeño: el estado manda, el DOM obedece. */
function mount(): void {
  const app = document.getElementById("app");
  if (!app) return;
  app.innerHTML = render();
  app.setAttribute("aria-busy", "false");

  const nameInput = document.getElementById("crew-name") as HTMLInputElement | null;
  nameInput?.addEventListener("input", () => {
    state.name = nameInput.value;
    // solo refrescar hash y botón, sin perder el foco
    const hash = document.querySelector(".field__hash");
    const next = document.getElementById("next") as HTMLButtonElement | null;
    if (hash) {
      const has = state.name.trim().length > 0;
      hash.textContent = has ? demoAccountHash(state.name) : "— type a name and the soil will assign you a plot —";
      hash.classList.toggle("field__hash--empty", !has);
    }
    if (next) next.disabled = state.name.trim().length === 0;
  });

  document.querySelectorAll<HTMLButtonElement>(".prod__btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      const card = btn.closest<HTMLElement>(".prod");
      if (!card?.dataset.id) return;
      const id = card.dataset.id;
      const cur = state.demand.get(id) ?? 0;
      state.demand.set(id, Math.max(0, cur + (btn.dataset.act === "inc" ? 1 : -1)));
      mount();
    });
  });

  document.getElementById("stamp")?.addEventListener("click", () => {
    state.stamped = true;
    saveCrew({
      name: state.name.trim(),
      account: demoAccountHash(state.name),
      demand: [...state.demand.entries()]
        .filter(([, q]) => q > 0)
        .map(([id, qty]) => {
          const p = PRODUCTS.find((x) => x.id === id)!;
          return { product: p.name, qty, unit: p.unit };
        }),
    });
    mount();
    // reproducir la animación de estampado tras el re-render
    document.querySelector(".sign__spot--done .seal")?.classList.add("seal--stamping");
  });

  document.getElementById("next")?.addEventListener("click", () => {
    if (state.step < 3) {
      state.step += 1;
      mount();
      window.scrollTo({ top: 0 });
    }
  });
  document.getElementById("back")?.addEventListener("click", () => {
    if (state.step > 0) {
      state.step -= 1;
      mount();
      window.scrollTo({ top: 0 });
    }
  });
}

mount();
