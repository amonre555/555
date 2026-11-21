// Couleurs dynamiques (Material You) : toute l'interface prend les couleurs de la pochette en
// cours. On extrait la couleur source de l'image, puis la bibliothèque officielle de Google
// en déduit un schéma Material 3 complet (fonds, conteneurs, accents, textes), en mode sombre.
//
// Les couleurs sont des variables CSS déclarées avec `@property` (voir styles.css) : le passage
// d'une palette à l'autre se fait donc en fondu.
import {
  argbFromHex,
  DynamicColor,
  Hct,
  hexFromArgb,
  MaterialDynamicColors as C,
  SchemeContent,
  sourceColorFromImageBytes,
} from "@material/material-color-utilities";
import { createEffect, createRoot } from "solid-js";
import { appearance } from "./appearance";

/** 555's color when nothing is playing (the brand's electric blue). */
const DEFAULT_SOURCE = argbFromHex("#2433ff");

/** Rôles Material 3 utilisés par l'interface → variable CSS `--md-<nom>`. */
const ROLES: Record<string, DynamicColor> = {
  primary: C.primary,
  "on-primary": C.onPrimary,
  "primary-container": C.primaryContainer,
  "on-primary-container": C.onPrimaryContainer,
  "secondary-container": C.secondaryContainer,
  "on-secondary-container": C.onSecondaryContainer,
  tertiary: C.tertiary,
  "tertiary-container": C.tertiaryContainer,
  surface: C.surface,
  "surface-container-lowest": C.surfaceContainerLowest,
  "surface-container-low": C.surfaceContainerLow,
  "surface-container": C.surfaceContainer,
  "surface-container-high": C.surfaceContainerHigh,
  "surface-container-highest": C.surfaceContainerHighest,
  "on-surface": C.onSurface,
  "on-surface-variant": C.onSurfaceVariant,
  "outline-variant": C.outlineVariant,
  error: C.error,
  "error-container": C.errorContainer,
  "inverse-surface": C.inverseSurface,
  "inverse-on-surface": C.inverseOnSurface,
  "on-error-container": C.onErrorContainer,
};

/** Couleur source déjà calculée pour chaque pochette. */
const cache = new Map<string, number>();
let current = "";
/** Couleur de la pochette en cours (gardée pour réappliquer quand l'apparence change). */
let coverSource = DEFAULT_SOURCE;

/**
 * Applique le schéma Material 3 sombre : couleur de la pochette (ou couleur fixe choisie dans les
 * Réglages), avec l'intensité choisie (0 : noir et blanc).
 */
function apply(source: number) {
  coverSource = source;
  const a = appearance();
  const base = a.mode === "fixed" ? argbFromHex(a.color) : source;
  const key = `${base}/${a.intensity}`;
  if (key === current) return;
  current = key;
  const hct = Hct.fromInt(base);
  hct.chroma = hct.chroma * a.intensity;
  const scheme = new SchemeContent(hct, true, 0);
  const root = document.documentElement.style;
  for (const [name, color] of Object.entries(ROLES)) {
    root.setProperty(`--md-${name}`, hexFromArgb(color.getArgb(scheme)));
  }
}

/** Boutons et accents (`--blue`) : le bleu de 555, ou la couleur fixe choisie. */
function applyAccent() {
  const a = appearance();
  const root = document.documentElement.style;
  if (a.mode !== "fixed") {
    root.removeProperty("--blue");
    root.removeProperty("--blue-deep");
    return;
  }
  const accent = Hct.fromInt(argbFromHex(a.color));
  const deep = Hct.from(accent.hue, accent.chroma, Math.max(0, accent.tone - 25));
  root.setProperty("--blue", a.color);
  root.setProperty("--blue-deep", hexFromArgb(deep.toInt()));
}

/** Couleur source d'une image : analysée sur une miniature 64 × 64 (rapide). */
function sourceOf(url: string): Promise<number> {
  const known = cache.get(url);
  if (known !== undefined) return Promise.resolve(known);
  return new Promise((resolve, reject) => {
    const img = new Image();
    // Les serveurs de pochettes autorisent la lecture des pixels (CORS).
    img.crossOrigin = "anonymous";
    img.decoding = "async";
    img.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 64;
        const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
        ctx.drawImage(img, 0, 0, 64, 64);
        const source = sourceColorFromImageBytes(ctx.getImageData(0, 0, 64, 64).data);
        cache.set(url, source);
        resolve(source);
      } catch (e) {
        reject(e); // image protégée (pas de CORS) : on garde la palette actuelle
      }
    };
    img.onerror = reject;
    img.src = url;
  });
}

let request = 0;

/** Thème de la pochette `url` (ou le thème par défaut sans pochette). */
export function themeFromCover(url: string | null) {
  const id = ++request;
  if (!url) return apply(DEFAULT_SOURCE);
  sourceOf(url)
    .then((source) => id === request && apply(source))
    .catch(() => {});
}

// Palette par défaut dès le démarrage, puis à chaque changement d'apparence.
apply(DEFAULT_SOURCE);
applyAccent();
createRoot(() =>
  createEffect(() => {
    appearance();
    apply(coverSource);
    applyAccent();
  }),
);
