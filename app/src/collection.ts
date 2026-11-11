// Collection partagée (titres likés, playlists) et petites notifications. N'importe quel
// composant peut liker un morceau ou l'ajouter à une playlist ; le cœur Rust enregistre et
// renvoie la collection à jour, qui remplace celle-ci d'un bloc.
import { createMemo, createSignal } from "solid-js";
import { api, type Collection, type Playlist, type Track } from "./api";
import { t } from "./i18n";

const [collection, setRawCollection] = createSignal<Collection>({ liked: [], playlists: [], history: [] });
export { collection };

/**
 * Le cœur renvoie toute la collection à chaque changement, avec de nouveaux objets. Les listes
 * (Solid) recréeraient alors chaque ligne et chaque carte, animations comprises. On réutilise
 * donc l'objet déjà connu de chaque morceau (même identifiant) : seul ce qui change bouge.
 */
let known = new Map<string, Track>();
function setCollection(c: Collection) {
  const next = new Map<string, Track>();
  const keep = (t: Track) => {
    const prev = known.get(t.id);
    const same = prev && JSON.stringify(prev) === JSON.stringify(t);
    const kept = same ? prev : t;
    next.set(t.id, kept);
    return kept;
  };
  const stable: Collection = {
    liked: c.liked.map(keep),
    playlists: c.playlists.map((p) => ({ ...p, tracks: p.tracks.map(keep) })),
    history: c.history.map((h) => ({ ...h, track: keep(h.track) })),
  };
  known = next;
  setRawCollection(stable);
}

/** Identifiants des titres likés (test rapide pour chaque ligne). */
const likedIds = createMemo(() => new Set(collection().liked.map((t) => t.id)));
export const isLiked = (t: Track | null | undefined) => !!t && likedIds().has(t.id);

export const playlistById = (id: string | null | undefined): Playlist | undefined =>
  collection().playlists.find((p) => p.id === id);

// ---------- Notifications ----------
export type Toast = { text: string; kind: "info" | "error"; at: number };
const [toast, setToast] = createSignal<Toast | null>(null);
export { toast };
let toastTimer: number | undefined;
export function notify(text: string, kind: Toast["kind"] = "info") {
  setToast({ text, kind, at: performance.now() });
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => setToast(null), kind === "error" ? 5000 : 2600);
}
export const dismissToast = () => setToast(null);
const fail = (e: unknown) => notify(String(e), "error");

/** Applique une modification ; renvoie la collection à jour, ou `null` en cas d'échec. */
async function apply(action: Promise<Collection>): Promise<Collection | null> {
  try {
    const c = await action;
    setCollection(c);
    return c;
  } catch (e) {
    fail(e);
    return null;
  }
}

export async function loadCollection() {
  await apply(api.collection());
  // Le cœur prévient quand il note une écoute (historique) : on suit sans redemander.
  api.onCollection(setCollection).catch(() => {});
}

export async function clearHistory() {
  if (await apply(api.clearHistory())) notify(t.historyCleared);
}

/** Morceaux écoutés récemment, sans doublons, du plus récent au plus ancien. */
export const recentTracks = createMemo(() => {
  const seen = new Set<string>();
  return collection().history.map((p) => p.track).filter((t) => !seen.has(t.id) && !!seen.add(t.id));
});

/**
 * Importe une playlist depuis un lien, ou des morceaux Spotify (liens collés) dans les titres
 * likés. `onProgress` suit la recherche des morceaux (Spotify).
 * Renvoie l'identifiant de la nouvelle playlist, `"liked"`, ou `null` en cas d'échec (déjà signalé).
 */
export async function importPlaylist(url: string, onProgress: (done: number, total: number) => void): Promise<string | null> {
  const stop = await api.onImportProgress(onProgress).catch(() => null);
  try {
    const r = await api.importPlaylist(url);
    setCollection(r.collection);
    if (r.liked) {
      notify(t.importedLiked(r.added, r.missing));
      return "liked";
    }
    const name = r.collection.playlists.find((p) => p.id === r.id)?.name ?? t.thePlaylist;
    notify(t.imported(name, r.added, r.missing));
    return r.id;
  } catch (e) {
    fail(e);
    return null;
  } finally {
    stop?.();
  }
}

export async function toggleLike(track: Track) {
  const was = isLiked(track);
  if (await apply(api.toggleLike(track))) notify(was ? t.removedFromLiked : t.addedToLiked);
}

/** Crée une playlist (avec ces morceaux, s'il y en a) ; renvoie son identifiant. */
export async function createPlaylist(name = "", tracks: Track[] = []): Promise<string | null> {
  const c = await apply(api.createPlaylist(name.trim() || t.defaultPlaylist(collection().playlists.length + 1), tracks));
  const p = c?.playlists[c.playlists.length - 1];
  if (p && tracks.length) notify(t.addedTo(p.name));
  return p?.id ?? null;
}

export const renamePlaylist = (id: string, name: string) => apply(api.renamePlaylist(id, name));
export const deletePlaylist = (id: string) => apply(api.deletePlaylist(id));
export const setPlaylistCover = (id: string, cover: string | null) => apply(api.setPlaylistCover(id, cover));

/** Lit une image choisie par l'utilisateur, recadrée au carré et réduite (600 px, JPEG) :
 *  elle est enregistrée telle quelle dans la collection. */
export function imageToCover(file: File, size = 600): Promise<string> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const img = new Image();
    img.onload = () => {
      const side = Math.min(img.naturalWidth, img.naturalHeight);
      const out = Math.min(size, side);
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = out;
      const ctx = canvas.getContext("2d");
      URL.revokeObjectURL(url);
      if (!ctx || !side) return reject(new Error("image"));
      ctx.drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, out, out);
      resolve(canvas.toDataURL("image/jpeg", 0.85));
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error("image"));
    };
    img.src = url;
  });
}
export const removeFromPlaylist = (id: string, trackId: string) => apply(api.removeFromPlaylist(id, trackId));
export const moveInPlaylist = (id: string, from: number, to: number) => apply(api.moveInPlaylist(id, from, to));

export async function addToPlaylist(id: string, tracks: Track[]) {
  const before = playlistById(id)?.tracks.length ?? 0;
  const c = await apply(api.addToPlaylist(id, tracks));
  const p = c?.playlists.find((x) => x.id === id);
  if (!p) return;
  const added = p.tracks.length - before;
  notify(added === 0 ? t.alreadyIn(p.name) : added === 1 ? t.addedTo(p.name) : t.addedManyTo(added, p.name));
}

// ---------- Menu d'un morceau (clic droit, bouton « ⋯ ») ----------
/** `playlist` : la playlist affichée, pour proposer « Retirer de cette playlist ». */
export type TrackMenu = { x: number; y: number; align: "left" | "right"; tracks: Track[]; playlist?: string };
const [trackMenu, setTrackMenu] = createSignal<TrackMenu | null>(null);
export { trackMenu };
export const closeTrackMenu = () => setTrackMenu(null);

/** Ouvre le menu sous le bouton cliqué, ou à la position de la souris (clic droit). */
export function openTrackMenu(e: MouseEvent, tracks: Track[], playlist?: string) {
  e.preventDefault();
  e.stopPropagation();
  const el = e.currentTarget as HTMLElement | null;
  const fromButton = e.type === "click" && el;
  const r = fromButton ? el.getBoundingClientRect() : null;
  setTrackMenu(
    r
      ? { x: r.right, y: r.bottom + 6, align: "right", tracks, playlist }
      : { x: e.clientX, y: e.clientY, align: "left", tracks, playlist },
  );
}

/** Pochettes d'une playlist : jusqu'à 4 différentes, pour une mosaïque. */
export function playlistCovers(p: Playlist, coverOf: (t: Track) => string | null): string[] {
  const out: string[] = [];
  for (const t of p.tracks) {
    const u = coverOf(t);
    if (u && !out.includes(u)) out.push(u);
    if (out.length === 4) break;
  }
  return out;
}
