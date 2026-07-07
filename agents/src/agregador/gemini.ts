/**
 * Cliente mínimo de Gemini (REST, structured output) para el Agregador.
 * El LLM SOLO normaliza lenguaje natural a un spec JSON; jamás decide el ganador
 * del RFQ (eso es clearing determinista — INV-2). Interfaz `Normalizer` para
 * poder inyectar un mock en los tests (incluido el adversarial).
 */

const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

/** Abstracción del normalizador: texto + schema → objeto JSON. */
export interface Normalizer {
  normalize(text: string, schema: object, systemInstruction: string): Promise<unknown>;
}

export interface GeminiConfig {
  readonly apiKey: string;
  readonly model: string; // p.ej. "gemini-2.5-flash"
  readonly baseUrl?: string;
}

interface GeminiResponse {
  readonly candidates?: ReadonlyArray<{
    readonly content?: { readonly parts?: ReadonlyArray<{ readonly text?: string }> };
  }>;
  readonly error?: { readonly message?: string };
}

/** Normalizador respaldado por Gemini (generateContent con responseSchema). */
export class GeminiNormalizer implements Normalizer {
  private readonly cfg: GeminiConfig;
  constructor(cfg: GeminiConfig) {
    this.cfg = cfg;
  }

  async normalize(text: string, schema: object, systemInstruction: string): Promise<unknown> {
    const base = this.cfg.baseUrl ?? "https://generativelanguage.googleapis.com/v1beta";
    const url = `${base}/models/${this.cfg.model}:generateContent?key=${this.cfg.apiKey}`;
    const body = {
      system_instruction: { parts: [{ text: systemInstruction }] },
      contents: [{ parts: [{ text }] }],
      generationConfig: {
        responseMimeType: "application/json",
        responseSchema: schema,
        temperature: 0, // determinismo en la extracción
      },
    };

    // Gemini devuelve 503 (UNAVAILABLE) / 429 (rate limit) de forma transitoria.
    // Reintentar con backoff exponencial; los demás errores son terminales.
    const maxAttempts = 4;
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      let res: Response;
      try {
        res = await fetch(url, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        });
      } catch (netErr) {
        if (attempt === maxAttempts) throw netErr;
        await sleep(1000 * 2 ** (attempt - 1));
        continue;
      }
      if ((res.status === 503 || res.status === 429) && attempt < maxAttempts) {
        await sleep(1000 * 2 ** (attempt - 1));
        continue;
      }
      if (!res.ok) throw new Error(`Gemini HTTP ${res.status}: ${await res.text()}`);
      const data = (await res.json()) as GeminiResponse;
      if (data.error) throw new Error(`Gemini: ${data.error.message}`);
      const txt = data.candidates?.[0]?.content?.parts?.[0]?.text;
      if (!txt) throw new Error("Gemini: respuesta vacía");
      return JSON.parse(txt);
    }
    throw new Error("Gemini: agotados los reintentos (503/429)");
  }
}

/** Construye el normalizador desde el entorno; undefined si falta GEMINI_API_KEY. */
export function loadGeminiNormalizer(
  env: Record<string, string | undefined> = process.env,
): GeminiNormalizer | undefined {
  const apiKey = env["GEMINI_API_KEY"]?.trim();
  if (!apiKey) return undefined;
  const model = env["GEMINI_MODEL"]?.trim() || "gemini-2.5-flash";
  return new GeminiNormalizer({ apiKey, model });
}
