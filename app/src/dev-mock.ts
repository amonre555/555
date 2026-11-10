// Mode aperçu, pour travailler l'interface dans un simple navigateur (`npm run dev`).
// Simule le cœur Rust : morceaux, lecture et paroles d'exemple.
// Jamais inclus dans l'app : chargé seulement en développement, hors de Tauri.
import type { Lyrics, Snapshot, Track } from "./api";

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
  }));
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
