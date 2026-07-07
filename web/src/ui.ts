// Piezas compartidas de "El Almanaque": fuentes, cinta de navegación,
// sello de goma SVG y sprites del ciclo de cultivo. Tokens en CLAUDE.md.
import "@fontsource-variable/fraunces";
import "@fontsource/instrument-sans/400.css";
import "@fontsource/instrument-sans/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/silkscreen/400.css";

export type PageId = "landing" | "onboarding" | "dashboard";

// Permite forzar el tema por URL (?theme=dark|light) — útil para el demo.
const forced = new URLSearchParams(location.search).get("theme");
if (forced === "dark" || forced === "light") {
  document.documentElement.dataset.theme = forced;
}

/** Cinta de navegación de almanaque: tres capítulos numerados + red. */
export function navRibbon(active: PageId): string {
  const item = (id: PageId, href: string, num: string, label: string) => `
    <a class="nav__item${id === active ? " nav__item--here" : ""}" href="${href}"
       ${id === active ? 'aria-current="page"' : ""}>
      <span class="nav__num">${num}</span><span>${label}</span>
    </a>`;
  return `
  <nav class="nav" aria-label="Ohu demo chapters">
    <a class="nav__brand" href="/" title="Ohu — the cooperative almanac">
      <img class="nav__tui" src="/img/mascot-tui.png" alt="" width="26" height="26" />
      <span>ohu</span>
    </a>
    <div class="nav__items">
      ${item("landing", "/", "i.", "The Almanac")}
      ${item("onboarding", "/onboarding.html", "ii.", "Join the ohu")}
      ${item("dashboard", "/dashboard.html", "iii.", "Control Room")}
    </div>
    <span class="nav__net" title="Every figure in this demo is a real Casper Testnet transaction">casper-test</span>
  </nav>`;
}

/** Sello de goma dentado (SVG). `id` debe ser único por página (ancla los arcos). */
export function seal(
  id: string,
  top: string,
  middle: string,
  bottom: string,
  opts: { tone?: "clay" | "green"; rotate?: number; size?: number } = {},
): string {
  const { tone = "clay", rotate = -3, size = 118 } = opts;
  return `
  <span class="seal seal--${tone}" style="--seal-rot:${rotate}deg; --seal-size:${size}px" role="img"
        aria-label="${top} — ${middle} — ${bottom}">
    <svg viewBox="0 0 120 120" width="${size}" height="${size}">
      <defs>
        <path id="${id}-t" d="M 16 60 a 44 44 0 0 1 88 0" fill="none" />
        <path id="${id}-b" d="M 12 60 a 48 48 0 0 0 96 0" fill="none" />
      </defs>
      <circle cx="60" cy="60" r="56" fill="none" stroke="currentColor" stroke-width="5" stroke-dasharray="3.2 2.6" />
      <circle cx="60" cy="60" r="50" fill="none" stroke="currentColor" stroke-width="1.8" />
      <circle cx="60" cy="60" r="31" fill="none" stroke="currentColor" stroke-width="1.4" />
      <text class="seal__arc"><textPath href="#${id}-t" startOffset="50%" text-anchor="middle">${top}</textPath></text>
      <text class="seal__arc"><textPath href="#${id}-b" startOffset="50%" text-anchor="middle">${bottom}</textPath></text>
      <text class="seal__mid" x="60" y="57" text-anchor="middle">${middle.split("|")[0] ?? ""}</text>
      <text class="seal__mid seal__mid--sub" x="60" y="70" text-anchor="middle">${middle.split("|")[1] ?? ""}</text>
    </svg>
  </span>`;
}

/** Sprites del ciclo de cultivo (24px de origen, mostrados como lámina impresa). */
export const CYCLE_SPRITES: Record<string, string> = {
  seed: "/img/cycle-seed.png",
  sprout: "/img/cycle-sprout.png",
  flower: "/img/cycle-flower.png",
  crate: "/img/cycle-crate.png",
  barn: "/img/cycle-barn.png",
};

export function sprite(kind: keyof typeof CYCLE_SPRITES, size = 48, alt = ""): string {
  return `<img class="sprite" src="${CYCLE_SPRITES[kind]}" alt="${alt}" width="${size}" height="${size}" loading="lazy" />`;
}

/** Datos de la cuadrilla sembrados por el onboarding (localStorage). */
export interface CrewData {
  readonly name: string;
  readonly account: string;
  readonly demand: ReadonlyArray<{ product: string; qty: number; unit: string }>;
}

const CREW_KEY = "ohu.crew";

export function loadCrew(): CrewData | null {
  try {
    const raw = localStorage.getItem(CREW_KEY);
    return raw ? (JSON.parse(raw) as CrewData) : null;
  } catch {
    return null;
  }
}

export function saveCrew(crew: CrewData): void {
  localStorage.setItem(CREW_KEY, JSON.stringify(crew));
}
