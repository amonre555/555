// Mode aperçu, pour travailler l'interface dans un simple navigateur (`npm run dev`).
// Simule le cœur Rust : morceaux, lecture et paroles d'exemple.
// Jamais inclus dans l'app : chargé seulement en développement, hors de Tauri.
import type { AlbumPage, ArtistPage, Collection, Lyrics, Snapshot, Track } from "./api";
import { openAlbum, openArtist } from "./nav";
// Real metadata from Deezer's public API (VHS by Mattyeux, the artist, an album, the charts),
// used by `?demo=…` to take the website's screenshots. Lyrics stay the made-up sample below.
import DEMO from "./demo-data.json";

/** `?demo` / `?demo=lyrics|artist|album|history`: a ready-made screen for screenshots. */
const demo = new URLSearchParams(location.search).get("demo");

type Listener = (payload: unknown) => void;
const listeners = new Map<string, Listener[]>();
let callbackId = 0;
const callbacks = new Map<number, (e: unknown) => void>();

let tracks: Track[] = [];
let queue: Track[] = [];
let state: Snapshot = {
  track: null, status: "idle", position: 0, duration: null, volume: 0.8, index: null, queueLen: 0, error: null, shuffle: false, repeat: "off",
};
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
  // Historique : noté tout de suite dans l'aperçu (le cœur attend 20 s d'écoute).
  coll.history = [{ track: queue[i], at: Date.now() }, ...coll.history].slice(0, 2000);
  setTimeout(() => emit("collection", structuredClone(coll)), 0);
}

// Morceaux d'exemple (pochettes aléatoires de picsum.photos : l'aperçu n'a pas accès à Deezer).
async function loadTrending(): Promise<Track[]> {
  if (demo !== null) return DEMO.chart as Track[];
  const names: [string, string][] = [
    ["Néons sur la ville", "Les Cassettes"], ["Minuit à Lisbonne", "Clara Vent"], ["Rembobine", "VHS Club"],
    ["Encore une fois", "Mattéo Rive"], ["Lumières basses", "Nuit Blanche"], ["Sweat (Snoop Dogg vs David Guetta Remix)", "Snoop Dogg, David Guetta"],
    ["Autoroute", "Les Phares"], ["Été 98", "Sable Rose"], ["Pluie d'août", "Inès Marée"], ["Satellite", "Orbite"],
    ["Ralenti", "Duo Polaroïd"], ["Fin de soirée", "Les Cassettes"], ["Néon bleu", "Clara Vent"],
  ];
  return names.map(([title, artist], i) => ({
    id: `deezer:demo${i}`, source: "deezer", title, artist, duration: 150 + i * 13,
    trackNumber: null, discNumber: null, artwork: `https://picsum.photos/seed/yato${i}/480`, hasCover: false,
    location: { kind: "remote", remoteId: `demo${i}` },
    artists: artist.split(", ").map((name) => ({ name, id: `demo-${name}` })),
    album: i % 3 === 0 ? null : "Cassettes et néons",
    albumId: i % 3 === 0 ? null : "demo-album",
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
  ].map((text, i) => {
    // Mots répartis sur 2,4 s, au prorata de leur longueur (comme l'estimation du cœur).
    const time = i * 3.2;
    const parts = text.split(/(?<=\s)/).filter((p) => p.trim());
    const total = parts.reduce((a, p) => a + p.length, 0);
    let at = time;
    const words = parts.map((p) => {
      const w = { time: at, end: at + (2.4 * p.length) / total, text: p };
      at = w.end;
      return w;
    });
    return { time, end: at, text, words };
  }),
  wordSynced: false,
};

// Collection en mémoire (perdue au rechargement de la page).
const coll: Collection = { liked: [], playlists: [], history: [] };
let settings = { crossfade: 0, resume: true, discord: true };
let plCounter = 0;
const copy = (): Collection => structuredClone(coll);
const pl = (id: string) => {
  const p = coll.playlists.find((x) => x.id === id);
  if (!p) throw "cette playlist n'existe plus";
  return p;
};
const pushUnique = (list: Track[], add: Track[]) => add.forEach((t) => list.some((x) => x.id === t.id) || list.push(t));

const handlers: Record<string, (args: any) => unknown> = {
  collection: copy,
  clear_history: () => ((coll.history = []), copy()),
  get_settings: () => ({ ...settings }),
  set_settings: ({ value }) => ((settings = { ...value }), { ...settings }),
  set_shuffle: ({ on }) => ((state.shuffle = on), snap()),
  set_repeat: ({ repeat }) => ((state.repeat = repeat), snap()),
  import_playlist: async ({ url }) => {
    if (!/spotify|deezer|youtube/.test(String(url))) throw "Lien non reconnu : colle le lien d'une playlist Spotify, Deezer ou YouTube Music.";
    const all = tracks.length ? tracks : await loadTrending();
    for (let k = 0; k <= 6; k += 2) {
      emit("import-progress", [k, 6]);
      await new Promise((r) => setTimeout(r, 300));
    }
    const p = { id: `pl-${++plCounter}`, name: "Playlist importée", tracks: all.slice(2, 8), created: Date.now() };
    coll.playlists.push(p);
    return { collection: copy(), id: p.id, added: p.tracks.length, missing: 1 };
  },
  "plugin:app|version": () => "0.1.1",
  toggle_like: ({ track }) => {
    const i = coll.liked.findIndex((t) => t.id === track.id);
    if (i >= 0) coll.liked.splice(i, 1);
    else coll.liked.unshift(track);
    return copy();
  },
  create_playlist: ({ name, tracks: t }) => {
    const p = { id: `pl-${++plCounter}`, name: String(name).trim() || `Ma playlist n° ${coll.playlists.length + 1}`, tracks: [] as Track[], created: Date.now() };
    pushUnique(p.tracks, t ?? []);
    coll.playlists.push(p);
    return copy();
  },
  rename_playlist: ({ id, name }) => ((pl(id).name = String(name).trim() || pl(id).name), copy()),
  delete_playlist: ({ id }) => ((coll.playlists = coll.playlists.filter((p) => p.id !== id)), copy()),
  add_to_playlist: ({ id, tracks: t }) => (pushUnique(pl(id).tracks, t), copy()),
  remove_from_playlist: ({ id, trackId }) => ((pl(id).tracks = pl(id).tracks.filter((t) => t.id !== trackId)), copy()),
  move_in_playlist: ({ id, from, to }) => {
    const list = pl(id).tracks;
    const [t] = list.splice(from, 1);
    if (t) list.splice(Math.min(to, list.length), 0, t);
    return copy();
  },
  list_sources: () => [{ id: "local", name: "Ma bibliothèque" }, { id: "deezer", name: "Titres" }, { id: "youtube", name: "YouTube Music" }],
  library_tracks: () => [],
  library_folders: () => [],
  featured: async () => {
    tracks = tracks.length ? tracks : await loadTrending();
    // Example history over a few days (so the History page has something to show).
    if (!coll.history.length) {
      const hour = 3_600_000;
      coll.history = [0, 1, 2, 0, 3, 4, 0, 5, 1, 6, 2, 7].map((n, k) => ({ track: tracks[n], at: Date.now() - (k * 7 + 1) * hour }));
      setTimeout(() => emit("collection", structuredClone(coll)), 0);
    }
    // `?demo`: start playing right away, then open the requested screen (website screenshots).
    if (demo !== null && !state.track) {
      const vhs = DEMO.vhs as Track;
      // The lyrics screen plays VHS too, but its lines (made-up sample) are blurred out: the
      // karaoke layout shows, never readable lyrics attributed to a real song.
      queue = [vhs, ...tracks.filter((t) => t.id !== vhs.id)];
      playAt(0);
      // Lyrics screen: captured during an instrumental break (the active line is ♪, sharp);
      // the made-up sample lines around it stay unreadable, like distant lines in the app.
      startedAt -= demo === "lyrics" ? -(20_000 - 16_700) : 62_000;
      if (demo === "lyrics") {
        const style = document.createElement("style");
        style.textContent = ".lyric:not(.on) { filter: blur(calc(5px + var(--d, 1) * 1.5px)) !important; }";
        document.head.append(style);
      }
      {
        const pick = [vhs, ...DEMO.artist.top.slice(1, 4), ...tracks.slice(0, 8)] as Track[];
        const hour = 3_600_000;
        coll.history = [0, 1, 4, 0, 5, 2, 6, 0, 7, 3, 8, 1, 9, 10].map((n, k) => ({ track: pick[n], at: Date.now() - (k * 5 + 1) * hour }));
        coll.liked = [vhs, ...(DEMO.artist.top.slice(1, 3) as Track[])];
        setTimeout(() => emit("collection", structuredClone(coll)), 0);
      }
      const click = (sel: string, text?: string) =>
        [...document.querySelectorAll<HTMLElement>(sel)].find((el) => !text || el.textContent?.includes(text))?.click();
      setTimeout(() => {
        if (demo === "lyrics") click(".now-btn");
        if (demo === "artist") openArtist({ name: DEMO.artist.name, id: DEMO.artist.id });
        if (demo === "album") openAlbum({ id: DEMO.album.id, title: DEMO.album.title });
        if (demo === "history") click(".sidenav .nav", "History");
      }, 900);
    }
    return tracks;
  },
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
  warm: () => null,
  lyrics: () => LYRICS,
  artist: ({ name }) => (demo !== null && name === DEMO.artist.name ? (DEMO.artist as ArtistPage) : demoArtist(name)),
  album: async ({ title }): Promise<AlbumPage> => {
    if (demo !== null && title === DEMO.album.title) return DEMO.album as AlbumPage;
    const all = tracks.length ? tracks : await loadTrending();
    const list = all.slice(0, 9).map((t) => ({ ...t, album: title, albumId: "demo-album", artwork: "https://picsum.photos/seed/album/600" }));
    return {
      id: "demo-album", title, cover: "https://picsum.photos/seed/album/600", year: "2024", kind: "album",
      label: "Disques Exemple", artists: [{ name: "Les Cassettes", id: "demo-Les Cassettes" }, { name: "Clara Vent", id: "demo-Clara Vent" }],
      duration: list.reduce((a, t) => a + (t.duration ?? 0), 0), tracks: list,
    };
  },
  track_artists: ({ track }) => track.artists ?? [{ name: track.artist }],
  ytdlp_status: () => ({ state: "ready" }),
  // Mises à jour : aucune, sauf avec `?update` dans l'adresse (pour voir la carte).
  "plugin:updater|check": () =>
    location.search.includes("update") ? { rid: 1, currentVersion: "0.1.0", version: "0.2.0", date: null, body: "", rawJson: {} } : null,
  "plugin:updater|download_and_install": () => new Promise((done) => setTimeout(done, 1500)),
  "plugin:process|restart": () => location.reload(),
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
