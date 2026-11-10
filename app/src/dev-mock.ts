// Mode aperçu, pour travailler l'interface dans un simple navigateur (`npm run dev`).
// Simule le cœur Rust : morceaux, lecture et paroles d'exemple.
// Jamais inclus dans l'app : chargé seulement en développement, hors de Tauri.
import type { ArtistPage, Lyrics, Snapshot, Track } from "./api";

type Listener = (payload: unknown) => void;
const listeners = new Map<string, Listener[]>();
let callbackId = 0;
const callbacks = new Map<number, (e: unknown) => void>();

let tracks: Track[] = [];
let queue: Track[] = [];
let state: Snapshot = { track: null, status: "idle", position: 0, duration: null, volume: 0.8, index: null, queueLen: 0, error: null };
let startedAt = 0;

const snap = (): Snapshot => {
  if (state.status === "playing") state.position = (performance.now() - startedAt) / 1000;
  return { ...state };
};
const emit = (event: string, payload: unknown) => (listeners.get(event) ?? []).forEach((l) => l(payload));
setInterval(() => emit("player", snap()), 250);

function playAt(i: number) {
  state = { ...state, track: queue[i], index: i, status: "playing", position: 0, duration: queue[i].duration, queueLen: queue.length };
  startedAt = performance.now();
}

// Morceaux d'exemple (pochettes aléatoires de picsum.photos : l'aperçu n'a pas accès à Deezer).
async function loadTrending(): Promise<Track[]> {
  const names: [string, string][] = [
    ["Néons sur la ville", "Les Cassettes"], ["Minuit à Lisbonne", "Clara Vent"], ["Rembobine", "VHS Club"],
    ["Encore une fois", "Mattéo Rive"], ["Lumières basses", "Nuit Blanche"], ["Sweat (Snoop Dogg vs David Guetta Remix)", "Snoop Dogg, David Guetta"],
    ["Autoroute", "Les Phares"], ["Été 98", "Sable Rose"], ["Pluie d'août", "Inès Marée"], ["Satellite", "Orbite"],
    ["Ralenti", "Duo Polaroïd"], ["Fin de soirée", "Les Cassettes"], ["Néon bleu", "Clara Vent"],
  ];
  return names.map(([title, artist], i) => ({
    id: `deezer:demo${i}`, source: "deezer", title, artist, album: null, duration: 150 + i * 13,
    trackNumber: null, discNumber: null, artwork: `https://picsum.photos/seed/ondes${i}/480`, hasCover: false,
    location: { kind: "remote", remoteId: `demo${i}` },
    artists: artist.split(", ").map((name) => ({ name, id: `demo-${name}` })),
  }));
}

/** Fiche d'exemple : les morceaux de l'artiste dans la liste, des albums et des voisins inventés. */
async function demoArtist(name: string): Promise<ArtistPage> {
  const all = tracks.length ? tracks : await loadTrending();
  const mine = all.filter((t) => t.artists?.some((a) => a.name === name));
  const seed = encodeURIComponent(name);
  return {
    id: `demo-${name}`, name, picture: `https://picsum.photos/seed/${seed}/600`,
    top: [...mine, ...all.filter((t) => !mine.includes(t)).slice(0, 7 - mine.length)],
    albums: [
      { id: `${seed}-a1`, title: "Cassettes et néons", cover: `https://picsum.photos/seed/${seed}a1/480`, year: "2024", kind: "album" },
      { id: `${seed}-a2`, title: "Premières lumières", cover: `https://picsum.photos/seed/${seed}a2/480`, year: "2019", kind: "album" },
      { id: `${seed}-s1`, title: "Fin de soirée", cover: `https://picsum.photos/seed/${seed}s1/480`, year: "2025", kind: "single" },
      { id: `${seed}-e1`, title: "Ralenti EP", cover: `https://picsum.photos/seed/${seed}e1/480`, year: "2022", kind: "ep" },
    ],
    related: ["Clara Vent", "VHS Club", "Nuit Blanche", "Sable Rose", "Orbite"]
      .filter((n) => n !== name)
      .map((n) => ({ id: `demo-${n}`, name: n, picture: `https://picsum.photos/seed/${encodeURIComponent(n)}/400` })),
  };
}

const LYRICS: Lyrics = {
  plain: null,
  instrumental: false,
  lines: [
    "", "Les néons s'allument sur la ville", "On roule sans savoir où l'on va", "Ta main dans la mienne, tout est facile",
    "Et la nuit ne finit pas", "", "On danse au ralenti", "Comme dans une vieille cassette", "Les souvenirs en couleurs",
    "Que le temps ne rembobine pas", "", "Encore une fois", "Encore une fois",
  ].map((text, i) => ({ time: i * 3.2, text })),
};

const handlers: Record<string, (args: any) => unknown> = {
  list_sources: () => [{ id: "local", name: "Ma bibliothèque" }, { id: "deezer", name: "Titres" }, { id: "youtube", name: "YouTube Music" }],
  library_tracks: () => [],
  library_folders: () => [],
  featured: async () => (tracks = tracks.length ? tracks : await loadTrending()),
  search: async ({ query }) => {
    const all = tracks.length ? tracks : await loadTrending();
    const q = String(query).toLowerCase();
    const hits = all.filter((t) => (t.title + t.artist).toLowerCase().includes(q));
    const list = hits.length ? hits : all.slice(0, 8);
    return list;
  },
  play_queue: ({ tracks: t, index }) => ((queue = t), playAt(index), snap()),
  play_index: ({ index }) => (playAt(index), snap()),
  toggle: () => {
    if (state.status === "playing") state = { ...snap(), status: "paused" };
    else if (state.track) {
      startedAt = performance.now() - state.position * 1000;
      state = { ...state, status: "playing" };
    }
    return snap();
  },
  next: () => (state.index != null && state.index + 1 < queue.length && playAt(state.index + 1), snap()),
  previous: () => (state.index != null && state.index > 0 && playAt(state.index - 1), snap()),
  seek: ({ position }) => ((startedAt = performance.now() - position * 1000), (state.position = position), snap()),
  set_volume: ({ volume }) => ((state.volume = volume), snap()),
  player_state: () => snap(),
  queue: () => queue,
  prefetch: () => null,
  lyrics: () => LYRICS,
  artist: ({ name }) => demoArtist(name),
  album_tracks: async () => (tracks.length ? tracks : await loadTrending()).slice(0, 8),
  track_artists: ({ track }) => track.artists ?? [{ name: track.artist }],
  ytdlp_status: () => ({ state: "ready" }),
};

(window as any).__TAURI_INTERNALS__ = {
  transformCallback: (cb: (e: unknown) => void) => {
    const id = ++callbackId;
    callbacks.set(id, cb);
    return id;
  },
  convertFileSrc: (path: string) => path,
  invoke: async (cmd: string, args: any) => {
    if (cmd === "plugin:event|listen") {
      const cb = callbacks.get(args.handler)!;
      const list = listeners.get(args.event) ?? [];
      list.push((payload) => cb({ event: args.event, payload }));
      listeners.set(args.event, list);
      return args.handler;
    }
    if (cmd === "plugin:event|unlisten") return null;
    const h = handlers[cmd];
    if (!h) throw new Error(`aperçu : commande non simulée « ${cmd} »`);
    return h(args ?? {});
  },
};
(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
