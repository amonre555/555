// Pont typé vers le cœur Rust. Les types reflètent ceux de `ondes-core`.
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

export type Status = "idle" | "loading" | "playing" | "paused" | "error";

export interface Snapshot {
  track: Track | null;
  status: Status;
  position: number;
  duration: number | null;
  volume: number;
  index: number | null;
  queueLen: number;
  error: string | null;
}

export type YtDlpStatus =
  | { state: "unknown" }
  | { state: "installing"; progress: number }
  | { state: "ready" }
  | { state: "system"; path: string }
  | { state: "error"; message: string };

export interface LyricLine {
  time: number;
  text: string;
}

export interface Lyrics {
  lines: LyricLine[];
  plain: string | null;
  instrumental: boolean;
}

export interface SourceInfo {
  id: string;
  name: string;
}

export const api = {
  sources: () => invoke<SourceInfo[]>("list_sources"),
  libraryTracks: () => invoke<Track[]>("library_tracks"),
  libraryFolders: () => invoke<string[]>("library_folders"),
  addFolder: (path: string) => invoke<number>("add_folder", { path }),
  removeFolder: (path: string) => invoke<number>("remove_folder", { path }),
  rescan: () => invoke<number>("rescan"),
  search: (query: string, source?: string) => invoke<Track[]>("search", { query, source: source ?? null }),
  featured: (source: string) => invoke<Track[]>("featured", { source }),
  playQueue: (tracks: Track[], index: number) => invoke<Snapshot>("play_queue", { tracks, index }),
  playIndex: (index: number) => invoke<Snapshot>("play_index", { index }),
  toggle: () => invoke<Snapshot>("toggle"),
  next: () => invoke<Snapshot>("next"),
  previous: () => invoke<Snapshot>("previous"),
  seek: (position: number) => invoke<Snapshot>("seek", { position }),
  prefetch: (track: Track) => invoke<void>("prefetch", { track }),
  lyrics: (track: Track) => invoke<Lyrics | null>("lyrics", { track }),
  artist: (ref: ArtistRef) => invoke<ArtistPage>("artist", { id: ref.id ?? null, name: ref.name }),
  album: (ref: AlbumRef) => invoke<AlbumPage>("album", { id: ref.id ?? null, title: ref.title, artist: ref.artist ?? null }),
  trackArtists: (track: Track) => invoke<ArtistRef[]>("track_artists", { track }),
  setVolume: (volume: number) => invoke<Snapshot>("set_volume", { volume }),
  state: () => invoke<Snapshot>("player_state"),
  queue: () => invoke<Track[]>("queue"),
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

/** Nom affiché d'un type d'album. */
export const ALBUM_KIND: Record<string, string> = { album: "Album", single: "Single", ep: "EP", compile: "Compilation" };

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
  if (seconds == null || !isFinite(seconds)) return "–:––";
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}
