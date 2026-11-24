// Thèmes CSS partageables : un thème est un simple fichier `.css` dont le premier commentaire
// donne le nom, l'auteur et une description :
//
//   /*
//    * @name Midnight Pink
//    * @author amon
//    * @description Pink accents on pure black.
//    */
//
// Les thèmes installés sont gardés sur l'appareil ; un seul est actif à la fois. Le CSS perso
// (Réglages → Apparence) reste appliqué par-dessus. La galerie vient du site (themes.json),
// alimentée par les Discussions GitHub (catégorie « Themes »).

import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import { createSignal } from "solid-js";

export type Theme = {
  id: string;
  name: string;
  author?: string;
  description?: string;
  css: string;
  /** Identifiant dans la galerie, s'il vient de là (pour les mises à jour et « installé »). */
  galleryId?: string;
};

/** Thème de la galerie (https://555.amon.lol/themes.json). */
export type GalleryTheme = {
  id: string;
  name: string;
  author: string;
  /** Avatar et profil GitHub de l'auteur. */
  authorAvatar?: string;
  authorUrl?: string;
  description?: string;
  css: string;
  votes: number;
  /** Discussion GitHub du thème (pour voter, commenter). */
  url: string;
  updatedAt?: string;
};

export const SITE = "https://555.amon.lol";
export const REPO = "https://github.com/amonre555/555";
export const MAX_THEME_BYTES = 256 * 1024;

/**
 * Thèmes de la galerie : ils ne peuvent charger que des polices Google Fonts (ou des `data:`).
 * Une image sur un autre serveur permettrait de savoir qui utilise le thème (adresse IP).
 * Même liste que scripts/collect-themes.mjs ; le site écarte déjà ces thèmes, l'appli revérifie.
 */
const ALLOWED_HOSTS = ["fonts.googleapis.com", "fonts.gstatic.com"];
export function blockedHosts(css: string): string[] {
  // Comments out, CSS escapes decoded (\\2f is "/"), so nothing hides an address.
  const code = css
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\\([0-9a-f]{1,6})\s?/gi, (_, h: string) => String.fromCodePoint(Math.min(parseInt(h, 16) || 0x20, 0x10ffff)))
    .replace(/\\(.)/g, "$1");
  const hosts = [...code.matchAll(/(?:https?:)?\/\/([^/\s"'()\\]+)/gi)].map((m) => m[1].toLowerCase());
  return [...new Set(hosts)].filter((h) => !ALLOWED_HOSTS.includes(h));
}

const KEY = "yato.themes";

type Stored = { themes: Theme[]; active: string | null };

function read(): Stored {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (s && Array.isArray(s.themes)) {
      const themes = (s.themes as Theme[]).filter((t) => t && typeof t.id === "string" && typeof t.css === "string");
      const active = themes.some((t) => t.id === s.active) ? s.active : null;
      return { themes, active };
    }
  } catch {}
  return { themes: [], active: null };
}

const [stored, setStored] = createSignal<Stored>(read());
export const installedThemes = () => stored().themes;
export const activeThemeId = () => stored().active;

/** Aperçu de la galerie : le thème essayé remplace l'actif tant qu'on ne l'a pas gardé. */
const [trying, setTrying] = createSignal<Theme | null>(null);
export { trying };

function save(next: Stored) {
  setStored(next);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {}
  applyTheme();

/**
 * Secours si un thème rend l'appli inutilisable : Ctrl+Maj+0 (⌘⇧0 sur Mac) coupe le thème et le
 * CSS perso jusqu'au prochain lancement (rien n'est supprimé : il suffit ensuite de choisir
 * « Aucun » dans les Réglages).
 */
window.addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey) || !e.shiftKey || e.code !== "Digit0") return;
  e.preventDefault();
  for (const id of ["theme-css", "user-css"]) {
    const sheet = (document.getElementById(id) as HTMLStyleElement | null)?.sheet;
    if (sheet) sheet.disabled = !sheet.disabled;
  }
});
}

export function setActiveTheme(id: string | null) {
  save({ ...stored(), active: id });
}

export function removeTheme(id: string) {
  const s = stored();
  save({ themes: s.themes.filter((t) => t.id !== id), active: s.active === id ? null : s.active });
}

/** Ajoute (ou met à jour, pour un thème de la galerie déjà installé) et active le thème. */
export function installTheme(theme: Omit<Theme, "id"> & { id?: string }) {
  const s = stored();
  const existing = theme.galleryId ? s.themes.find((t) => t.galleryId === theme.galleryId) : undefined;
  const id = existing?.id ?? theme.id ?? `t${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  const t: Theme = { ...theme, id };
  const themes = existing ? s.themes.map((x) => (x.id === id ? t : x)) : [...s.themes, t];
  setTrying(null);
  save({ themes, active: id });
}

export function tryTheme(theme: Theme | null) {
  setTrying(theme);
  applyTheme();
}

/** Lit l'en-tête d'un fichier de thème (`@name`, `@author`, `@description`). */
export function parseTheme(css: string, fallbackName: string): Omit<Theme, "id"> {
  const header = css.match(/^\s*\/\*([\s\S]*?)\*\//)?.[1] ?? "";
  const tag = (name: string) => header.match(new RegExp(`@${name}\\s+(.+)`))?.[1]?.replace(/\*\/?$/, "").trim() || undefined;
  return { name: tag("name") ?? fallbackName, author: tag("author"), description: tag("description"), css };
}

/** Fichier `.css` choisi par l'utilisateur → thème installé et activé. */
export async function importThemeFile(file: File): Promise<Theme> {
  if (file.size > MAX_THEME_BYTES) throw new Error("too-big");
  const css = await file.text();
  installTheme(parseTheme(css, file.name.replace(/\.css$/i, "")));
  return installedThemes()[installedThemes().length - 1];
}

/** Lien externe : navigateur du système (aperçu web : nouvel onglet). */
async function openUrl(url: string) {
  try {
    await tauriOpenUrl(url);
  } catch {
    window.open(url, "_blank", "noopener");
  }
}

/** Thèmes de la galerie, les plus votés d'abord. */
export async function fetchGallery(): Promise<GalleryTheme[]> {
  const r = await fetch(`${SITE}/themes.json`, { cache: "no-cache" });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  const data = (await r.json()) as { themes?: GalleryTheme[] };
  return (data.themes ?? [])
    .filter((t) => t.css && t.css.length <= MAX_THEME_BYTES && blockedHosts(t.css).length === 0)
    .sort((a, b) => b.votes - a.votes);
}

/** Ouvre la page du thème dans la galerie du site (aperçu, votes). */
export const openGalleryPage = (id?: string) => openUrl(id ? `${SITE}/themes/#${encodeURIComponent(id)}` : `${SITE}/themes/`);

/**
 * Partager : ouvre une nouvelle discussion GitHub (catégorie « Themes ») déjà remplie avec le CSS.
 * Trop long pour une adresse : le CSS est copié, l'utilisateur le colle.
 */
export async function shareTheme(css: string): Promise<"opened" | "copied"> {
  const name = parseTheme(css, "My theme").name;
  const block = "```css\n" + css.trim() + "\n```";
  const body = `<!-- A short description, and a screenshot if you like. Keep the CSS in the code block below. -->\n\n${block}\n`;
  const url = `${REPO}/discussions/new?category=themes&title=${encodeURIComponent(name)}&body=${encodeURIComponent(body)}`;
  if (url.length <= 7000) {
    await openUrl(url);
    return "opened";
  }
  await navigator.clipboard.writeText(block);
  await openUrl(`${REPO}/discussions/new?category=themes&title=${encodeURIComponent(name)}`);
  return "copied";
}

/** CSS du thème actif (ou essayé) : `<style id="theme-css">`, juste avant le CSS perso. */
export function applyTheme() {
  const theme = trying() ?? stored().themes.find((t) => t.id === stored().active) ?? null;
  let el = document.getElementById("theme-css") as HTMLStyleElement | null;
  if (!theme) return el?.remove();
  if (!el) {
    el = document.createElement("style");
    el.id = "theme-css";
  }
  const user = document.getElementById("user-css");
  if (user) document.head.insertBefore(el, user);
  else document.head.appendChild(el);
  el.textContent = theme.css;
}

applyTheme();
