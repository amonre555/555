// Pont typé vers le cœur Rust. Les types reflètent ceux de `yato-core`.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Location = { kind: "file"; path: string } | { kind: "remote"; remoteId: string };

export interface Track {
  id: string;
  source: string;
  title: string;
  artist: string;
  album: string | null;
  /** Identifiant de l'album, s'il est connu (sinon la fiche est retrouvée par titre). */
  albumId?: string | null;
  duration: number | null;
  trackNumber: number | null;
  discNumber: number | null;
  artwork: string | null;
  hasCover: boolean;
  location: Location;
  /** Chaque artiste séparément. Peut être vide (anciens morceaux en cache) : voir `artistsOf`. */
  artists?: ArtistRef[];
}

/** Un artiste cité par un morceau. `id` : identifiant Deezer, sinon la fiche est cherchée par nom. */
export interface ArtistRef {
  name: string;
  id?: string | null;
}

export interface ArtistPage {
  id: string;
  name: string;
  picture: string | null;
  top: Track[];
  albums: Album[];
  related: ArtistCard[];
}

export interface Album {
  id: string;
  title: string;
  cover: string | null;
  year: string | null;
  kind: "album" | "single" | "ep" | "compile" | string;
}

/** Un album à ouvrir : par identifiant, sinon par titre (et artiste). */
export interface AlbumRef {
  id?: string | null;
  title: string;
  artist?: string | null;
}

export interface AlbumPage {
  id: string;
  title: string;
  cover: string | null;
  year: string | null;
  kind: string;
  label: string | null;
  artists: ArtistRef[];
  duration: number | null;
  tracks: Track[];
}

export interface ArtistCard {
  id: string;
  name: string;
  picture: string | null;
}

/** Page Découvrir. */
export type SectionKind = "mix" | "because" | "releases" | "artists" | "rediscover" | "charts" | "chartArtists" | "chartAlbums";

/** Une sortie (album, EP, single) et son artiste. `date` : « 2026-11-07 », vide si inconnue. */
export interface Release extends Album {
  artist: string;
  date: string;
}

/** Un artiste proposé, proche d'un artiste que tu écoutes (`near`). */
export interface Suggestion {
  id: string;
  name: string;
  picture: string | null;
  near: string | null;
}

export interface DiscoverSection {
  /** Stable : `mix:<jour>`, `because:<artiste>`, `releases`… */
  id: string;
  kind: SectionKind;
  /** « Parce que tu écoutes X » : X. */
  artist?: ArtistRef | null;
  tracks: Track[];
  albums: Release[];
  artists: Suggestion[];
  /** Contenu présent ; sinon en cours de chargement. */
  ready: boolean;
  at: number;
}

export type Status = "idle" | "loading" | "playing" | "paused" | "error";

/** Répétition : rien, toute la file, ou le morceau en cours. */
export type Repeat = "off" | "all" | "one";

export interface Snapshot {
  track: Track | null;
  status: Status;
  position: number;
  duration: number | null;
  volume: number;
  index: number | null;
  queueLen: number;
  error: string | null;
  shuffle: boolean;
  repeat: Repeat;
}

export type YtDlpStatus =
  | { state: "unknown" }
  | { state: "installing"; progress: number }
  | { state: "ready" }
  | { state: "system"; path: string }
  | { state: "error"; message: string };

/** Un mot (ou une syllabe), avec l'espace qui le suit s'il y en a une. */
export interface LyricWord {
  time: number;
  end: number;
  text: string;
}

export interface LyricLine {
  time: number;
  /** Fin du chant de la ligne. */
  end: number;
  text: string;
  /** Vide pour une ligne vide (pause instrumentale). */
  words: LyricWord[];
}

export interface Lyrics {
  lines: LyricLine[];
  plain: string | null;
  instrumental: boolean;
  /** Minutage des mots donné par la source (sinon, estimé). */
  wordSynced: boolean;
}

export interface SourceInfo {
  id: string;
  name: string;
}

export interface Playlist {
  id: string;
  name: string;
  tracks: Track[];
  created: number;
  /** Image choisie (https ou data:) ; sinon, mosaïque des pochettes. */
  cover?: string | null;
}

/** Un morceau écouté, et quand (millisecondes depuis 1970). */
export interface Played {
  track: Track;
  at: number;
}

/** Titres likés et historique (le plus récent en premier), playlists. */
export interface Collection {
  liked: Track[];
  playlists: Playlist[];
  history: Played[];
}

/** Résultat d'un import de playlist. */
export interface ImportResult {
  collection: Collection;
  id: string | null;
  added: number;
  missing: number;
  /** Les morceaux sont allés dans les titres likés (liens de morceaux Spotify). */
  liked: boolean;
}

/** Réglages de l'app (page Réglages). */
export interface Settings {
  /** Fondu enchaîné, en secondes (0 : aucun). */
  crossfade: number;
  /** Reprendre l'écoute au lancement. */
  resume: boolean;
  /** Statut Discord. */
  discord: boolean;
}

/** Période résumée par les statistiques. */
export type Period = { kind: "year"; year: number } | { kind: "month"; year: number; month: number } | { kind: "all" };

export interface TopTrack {
  track: Track;
  plays: number;
  seconds: number;
}

export interface TopArtist {
  name: string;
  id: string | null;
  plays: number;
  seconds: number;
  /** Son morceau le plus écouté (pochette, lecture). */
  track: Track;
}

export interface TopAlbum {
  title: string;
  artist: string;
  id: string | null;
  plays: number;
  seconds: number;
  track: Track;
}

/** Statistiques d'écoute d'une période (voir `yato-core/src/stats.rs`). */
export interface Stats {
  period: Period;
  plays: number;
  /** Temps d'écoute total, en secondes. */
  seconds: number;
  tracks: number;
  artists: number;
  albums: number;
  newArtists: number;
  /** Il y a des écoutes avant la période. */
  hasEarlier: boolean;
  topTracks: TopTrack[];
  topArtists: TopArtist[];
  topAlbums: TopAlbum[];
  /** Secondes par mois (janvier d'abord), par heure (0 h d'abord), par jour (lundi d'abord). */
  byMonth: number[];
  byHour: number[];
  byWeekday: number[];
  bySource: { source: string; seconds: number }[];
  busiestDay: { year: number; month: number; day: number; seconds: number; plays: number } | null;
  /** 1 à 12. */
  busiestMonth: number | null;
  /** 0 à 23. */
  favoriteHour: number | null;
  activeDays: number;
  longestStreak: number;
  first: { track: Track; at: number } | null;
  /** Années qui ont des écoutes, la plus récente d'abord. */
  years: number[];
}

/** Décalage de l'heure locale sur UTC, en minutes (Paris en hiver : 60). */
const utcOffset = () => -new Date().getTimezoneOffset();

export const api = {
  sources: () => invoke<SourceInfo[]>("list_sources"),
  libraryTracks: () => invoke<Track[]>("library_tracks"),
  libraryFolders: () => invoke<string[]>("library_folders"),
  addFolder: (path: string) => invoke<number>("add_folder", { path }),
  removeFolder: (path: string) => invoke<number>("remove_folder", { path }),
  rescan: () => invoke<number>("rescan"),
  search: (query: string, source?: string) => invoke<Track[]>("search", { query, source: source ?? null }),
  featured: (source: string) => invoke<Track[]>("featured", { source }),
  /** Sections de Découvrir, tout de suite (cache) ; les sections recalculées arrivent par `onDiscoverSection`. */
  discover: (force = false) => invoke<DiscoverSection[]>("discover", { force, offset: utcOffset() }),
  onDiscoverSection: (cb: (s: DiscoverSection) => void): Promise<UnlistenFn> =>
    listen<DiscoverSection>("discover-section", (e) => cb(e.payload)),
  playQueue: (tracks: Track[], index: number) => invoke<Snapshot>("play_queue", { tracks, index }),
  playIndex: (index: number) => invoke<Snapshot>("play_index", { index }),
  toggle: () => invoke<Snapshot>("toggle"),
  next: () => invoke<Snapshot>("next"),
  previous: () => invoke<Snapshot>("previous"),
  seek: (position: number) => invoke<Snapshot>("seek", { position }),
  prefetch: (track: Track) => invoke<void>("prefetch", { track }),
  /** Préparation légère (adresse du son, sans téléchargement) des morceaux affichés. */
  warm: (tracks: Track[]) => invoke<void>("warm", { tracks }),
  lyrics: (track: Track) => invoke<Lyrics | null>("lyrics", { track }),
  artist: (ref: ArtistRef) => invoke<ArtistPage>("artist", { id: ref.id ?? null, name: ref.name }),
  album: (ref: AlbumRef) => invoke<AlbumPage>("album", { id: ref.id ?? null, title: ref.title, artist: ref.artist ?? null }),
  trackArtists: (track: Track) => invoke<ArtistRef[]>("track_artists", { track }),
  setVolume: (volume: number) => invoke<Snapshot>("set_volume", { volume }),
  state: () => invoke<Snapshot>("player_state"),
  queue: () => invoke<Track[]>("queue"),
  collection: () => invoke<Collection>("collection"),
  toggleLike: (track: Track) => invoke<Collection>("toggle_like", { track }),
  createPlaylist: (name: string, tracks?: Track[]) => invoke<Collection>("create_playlist", { name, tracks: tracks ?? null }),
  renamePlaylist: (id: string, name: string) => invoke<Collection>("rename_playlist", { id, name }),
  setPlaylistCover: (id: string, cover: string | null) => invoke<Collection>("set_playlist_cover", { id, cover }),
  deletePlaylist: (id: string) => invoke<Collection>("delete_playlist", { id }),
  addToPlaylist: (id: string, tracks: Track[]) => invoke<Collection>("add_to_playlist", { id, tracks }),
  removeFromPlaylist: (id: string, trackId: string) => invoke<Collection>("remove_from_playlist", { id, trackId }),
  moveInPlaylist: (id: string, from: number, to: number) => invoke<Collection>("move_in_playlist", { id, from, to }),
  clearHistory: () => invoke<Collection>("clear_history"),
  stats: (period: Period) => invoke<Stats>("listening_stats", { period, offset: utcOffset() }),
  clearStats: () => invoke<void>("clear_stats"),
  recentSearches: () => invoke<string[]>("recent_searches"),
  addRecentSearch: (query: string) => invoke<string[]>("add_recent_search", { query }),
  removeRecentSearch: (query: string) => invoke<string[]>("remove_recent_search", { query }),
  clearRecentSearches: () => invoke<string[]>("clear_recent_searches"),
  importPlaylist: (url: string) => invoke<ImportResult>("import_playlist", { url }),
  onImportProgress: (cb: (done: number, total: number) => void): Promise<UnlistenFn> =>
    listen<[number, number]>("import-progress", (e) => cb(e.payload[0], e.payload[1])),
  onCollection: (cb: (c: Collection) => void): Promise<UnlistenFn> => listen<Collection>("collection", (e) => cb(e.payload)),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (value: Settings) => invoke<Settings>("set_settings", { value }),
  setShuffle: (on: boolean) => invoke<Snapshot>("set_shuffle", { on }),
  setRepeat: (repeat: Repeat) => invoke<Snapshot>("set_repeat", { repeat }),
  onPlayer: (cb: (s: Snapshot) => void): Promise<UnlistenFn> =>
    listen<Snapshot>("player", (e) => cb(e.payload)),
  ytdlpStatus: () => invoke<YtDlpStatus>("ytdlp_status"),
  onYtdlp: (cb: (s: YtDlpStatus) => void): Promise<UnlistenFn> =>
    listen<YtDlpStatus>("ytdlp", (e) => cb(e.payload)),
};

/** Les artistes d'un morceau, un par un (repli : découpe du texte « A, B & C feat. D »). */
export function artistsOf(t: Track): ArtistRef[] {
  if (t.artists?.length) return t.artists;
  return t.artist
    .split(/, | & | feat\.? | ft\. | featuring /i)
    .map((name) => name.trim())
    .filter(Boolean)
    .map((name) => ({ name }));
}

/** L'album d'un morceau, à ouvrir (rien si le morceau n'a pas d'album). */
export function albumRefOf(t: Track): AlbumRef | null {
  if (!t.album) return null;
  return { id: t.albumId ?? null, title: t.album, artist: artistsOf(t)[0]?.name ?? null };
}

/** URL de la pochette d'un morceau, distante ou servie par le protocole `cover://`. */
export function coverUrl(t: Track | null | undefined): string | null {
  if (!t) return null;
  if (t.artwork) return t.artwork;
  if (t.hasCover && t.location.kind === "file") return convertFileSrc(t.location.path, "cover");
  return null;
}

/** Version haute définition d'une pochette (pour les grands affichages). */
export function bigCoverUrl(t: Track | null | undefined): string | null {
  const url = coverUrl(t);
  if (!url) return null;
  // Pochettes YouTube Music : la taille est dans l'URL (=w226-h226…).
  return url.replace(/=w\d+-h\d+[^/]*$/, "=w1200-h1200-l90-rj");
}

export function formatTime(seconds: number | null | undefined): string {
  if (seconds == null || !isFinite(seconds)) return "--:--";
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${String(m).padStart(2, "0")}:${sec}`;
}
