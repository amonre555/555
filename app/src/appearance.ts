// Apparence choisie dans les Réglages : couleurs de la pochette ou couleur fixe, intensité des
// couleurs, et CSS perso. Gardée sur l'appareil (comme la langue), appliquée dès le démarrage.

import { createSignal } from "solid-js";

export type ColorMode = "cover" | "fixed";

export type Appearance = {
  /** « cover » : l'interface prend les couleurs de la pochette ; « fixed » : `color`, toujours. */
  mode: ColorMode;
  /** Couleur fixe (#rrggbb), aussi celle des boutons dans ce mode. */
  color: string;
  /** Intensité des couleurs : 0 = noir et blanc, 1 = normal, 1.5 = très vif. */
  intensity: number;
  /** CSS ajouté après celui de l'appli. */
  css: string;
};

export const BRAND_BLUE = "#2433ff";

/** Couleurs proposées en un clic (la première : le bleu de 555). */
export const SWATCHES = ["#2433ff", "#7c3aed", "#e11d74", "#f97316", "#16a34a", "#0ea5e9", "#a3a3a3"];

const DEFAULT: Appearance = { mode: "cover", color: BRAND_BLUE, intensity: 1, css: "" };
const KEY = "yato.appearance";

function read(): Appearance {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (!saved || typeof saved !== "object") return DEFAULT;
    const a = { ...DEFAULT, ...saved } as Appearance;
    if (a.mode !== "cover" && a.mode !== "fixed") a.mode = "cover";
    if (!/^#[0-9a-f]{6}$/i.test(a.color)) a.color = BRAND_BLUE;
    a.intensity = Math.min(1.5, Math.max(0, Number(a.intensity) || 0));
    if (typeof a.css !== "string") a.css = "";
    return a;
  } catch {
    return DEFAULT;
  }
}

const [appearance, setSignal] = createSignal<Appearance>(read());
export { appearance };

export function setAppearance(patch: Partial<Appearance>) {
  const next = { ...appearance(), ...patch };
  setSignal(next);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {}
  applyCss(next.css);
}

export function resetAppearance() {
  setAppearance({ ...DEFAULT, css: appearance().css });
}

/** CSS perso : une balise `<style>` à la fin du `<head>`, donc prioritaire sur celui de l'appli. */
function applyCss(css: string) {
  let el = document.getElementById("user-css") as HTMLStyleElement | null;
  if (!css.trim()) return el?.remove();
  if (!el) {
    el = document.createElement("style");
    el.id = "user-css";
  }
  // Toujours en dernier (le CSS de l'appli peut être ajouté après, en développement).
  document.head.appendChild(el);
  el.textContent = css;
}

applyCss(appearance().css);
