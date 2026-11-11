// Mode aperçu, pour travailler l'interface dans un simple navigateur (`npm run dev`).
// Simule le cœur Rust : morceaux, lecture et paroles d'exemple.
// Jamais inclus dans l'app : chargé seulement en développement, hors de Tauri.
import type { AlbumPage, ArtistPage, Collection, DiscoverSection, Lyrics, Period, Snapshot, Stats, Track } from "./api";
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

// ---------- Découvrir ----------
// `?newcomer` : page d'un nouvel utilisateur (tendances, artistes et albums du moment).

const WORDS = ["Néon", "Velours", "Rivage", "Comète", "Mirage", "Ruban", "Écho", "Vertige", "Lagune", "Orage", "Satin", "Prisme"];
const PLACES = ["de minuit", "sur la ville", "d'août", "bleu", "en sourdine", "au ralenti", "des phares", "de verre"];
const NEW_ARTISTS = ["Lune Sauvage", "Les Vitrines", "Marée Haute", "Polaire", "Odéon Club", "Ciel Rose", "Brume", "Kilomètre", "Les Échos", "Néréide", "Halo", "Station 9"];

/** Morceaux inventés (pochettes aléatoires), variés d'une section à l'autre. */
function fakeTracks(prefix: string, n: number, artists: string[]): Track[] {
  return Array.from({ length: n }, (_, i) => {
    const artist = artists[i % artists.length];
    const title = `${WORDS[(i * 7 + prefix.length) % WORDS.length]} ${PLACES[(i * 3 + prefix.length) % PLACES.length]}`;
    return {
      id: `deezer:${prefix}${i}`, source: "deezer", title, artist, album: "Cassettes et néons", albumId: "demo-album",
      duration: 160 + ((i * 17) % 90), trackNumber: null, discNumber: null, artwork: `https://picsum.photos/seed/${prefix}${i}/480`,
      hasCover: false, location: { kind: "remote", remoteId: `${prefix}${i}` }, artists: [{ name: artist, id: `demo-${artist}` }],
    };
  });
}

let discoverCache: DiscoverSection[] | null = null;

async function demoDiscover(): Promise<DiscoverSection[]> {
  const chart = (await handlers.featured({})) as Track[];
  if (discoverCache) return discoverCache;
  const day = Math.floor(Date.now() / 86_400_000);
  const section = (id: string, kind: DiscoverSection["kind"], more: Partial<DiscoverSection> = {}): DiscoverSection => ({
    id, kind, tracks: [], albums: [], artists: [], ready: false, at: 0, ...more,
  });
  const iso = (daysAgo: number) => new Date(Date.now() - daysAgo * 86_400_000).toISOString().slice(0, 10);
  const releases = ["Clara Vent", "Les Cassettes", "VHS Club", "Nuit Blanche", "Sable Rose", "Orbite"].map((artist, i) => ({
    id: `demo-rel${i}`, title: `${WORDS[i + 2]} ${PLACES[i]}`, cover: `https://picsum.photos/seed/rel${i}/480`,
    year: iso(i * 9).slice(0, 4), kind: ["album", "single", "ep"][i % 3], artist, date: iso(i * 9),
  }));
  const suggestions = NEW_ARTISTS.map((name, i) => ({
    id: `demo-${name}`, name, picture: `https://picsum.photos/seed/${encodeURIComponent(name)}/400`, near: ["Clara Vent", "Les Cassettes", "VHS Club"][i % 3],
  }));
  let full: DiscoverSection[];
  if (new URLSearchParams(location.search).has("newcomer")) {
    full = [
      section("charts", "charts", { tracks: chart }),
      section("chart-artists", "chartArtists", { artists: suggestions.map((s) => ({ ...s, near: null })) }),
      section("chart-albums", "chartAlbums", { albums: releases.map((r) => ({ ...r, date: "" })) }),
    ];
  } else {
    full = [
      section(`mix:${day}`, "mix", { tracks: [...fakeTracks("mix", 28, NEW_ARTISTS), ...chart.slice(0, 12)].sort((a, b) => a.id.localeCompare(b.id)) }),
      section("because:clara vent", "because", { artist: { name: "Clara Vent", id: "demo-Clara Vent" }, tracks: fakeTracks("bcv", 15, NEW_ARTISTS.slice(0, 6)) }),
      section("because:les cassettes", "because", { artist: { name: "Les Cassettes", id: "demo-Les Cassettes" }, tracks: fakeTracks("blc", 15, NEW_ARTISTS.slice(4)) }),
      section("releases", "releases", { albums: releases }),
      section("artists", "artists", { artists: suggestions }),
      section("rediscover", "rediscover", { tracks: chart.slice(4, 13), ready: true }),
      section("charts", "charts", { tracks: chart }),
    ];
  }
  // Chargement progressif : les sections arrivent une à une, comme depuis Deezer.
  full.forEach((s, i) => {
    if (s.ready) return;
    setTimeout(() => {
      const done = { ...s, ready: true, at: Date.now() };
      discoverCache = discoverCache?.map((x) => (x.id === done.id ? done : x)) ?? null;
      emit("discover-section", done);
    }, 400 + i * 250);
  });
  discoverCache = full.map((s) => (s.ready ? s : { ...s, tracks: [], albums: [], artists: [] }));
  return discoverCache;
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

// ---------- Stats : une année d'écoutes inventée (toujours la même), pour la page et le récap ----------
type DemoListen = { at: number; track: Track; secs: number };
let demoListenList: DemoListen[] | null = null;
async function demoListens(): Promise<DemoListen[]> {
  if (demoListenList) return demoListenList;
  const all = tracks.length ? tracks : await loadTrending();
  let seed = 555;
  const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const list: DemoListen[] = [];
  const now = new Date();
  // Du 1er octobre de l'an dernier à aujourd'hui : il y a aussi des écoutes avant l'année.
  for (let d = new Date(now.getFullYear() - 1, 9, 1); d < now; d.setDate(d.getDate() + 1)) {
    // Plus d'écoute en été et en fin d'année ; certains jours, rien.
    const m = d.getMonth();
    const sessions = Math.floor(rnd() * (m === 6 || m === 7 ? 4 : m >= 10 ? 3.4 : 2.6));
    for (let k = 0; k < sessions; k++) {
      const hour = [8, 12, 18, 21, 22, 22, 23, 0][Math.floor(rnd() * 8)];
      let at = new Date(d.getFullYear(), d.getMonth(), d.getDate(), hour, Math.floor(rnd() * 60)).getTime();
      for (let n = 3 + Math.floor(rnd() * 10); n > 0 && at < now.getTime(); n--) {
        // Les premiers morceaux du top reviennent bien plus souvent.
        const t = all[Math.floor(Math.pow(rnd(), 2.4) * all.length)];
        const secs = Math.round((t.duration ?? 180) * (rnd() < 0.2 ? 0.4 : 1));
        list.push({ at, track: t, secs });
        at += secs * 1000;
      }
    }
  }
  return (demoListenList = list);
}

/** Même calcul que `yato-core/src/stats.rs`, en plus simple (heure locale du navigateur). */
async function demoStats(period: Period): Promise<Stats> {
  const all = await demoListens();
  const inPeriod = (d: Date) =>
    period.kind === "all" || (d.getFullYear() === period.year && (period.kind === "year" || d.getMonth() + 1 === period.month));
  const dayKey = (d: Date) => `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;
  const from = period.kind === "all" ? -Infinity : new Date(period.year, period.kind === "month" ? period.month - 1 : 0, 1).getTime();
  const artistsOf = (t: Track) => (t.artists?.length ? t.artists : [{ name: t.artist, id: null }]);
  const firstSeen = new Map<string, number>();
  const years = new Set<number>();
  for (const l of all) {
    years.add(new Date(l.at).getFullYear());
    for (const a of artistsOf(l.track)) if (!firstSeen.has(a.name)) firstSeen.set(a.name, l.at);
  }
  type Tally = { plays: number; seconds: number; byTrack: Map<string, number> };
  const tally = (): Tally => ({ plays: 0, seconds: 0, byTrack: new Map() });
  const add = (x: Tally, l: DemoListen) => {
    x.plays++;
    x.seconds += l.secs;
    x.byTrack.set(l.track.id, (x.byTrack.get(l.track.id) ?? 0) + 1);
  };
  const best = (x: Tally) => [...x.byTrack].sort((a, b) => b[1] - a[1])[0][0];
  const tr = new Map<string, Tally & { track: Track }>();
  const ar = new Map<string, Tally & { id: string | null }>();
  const al = new Map<string, Tally & { title: string; artist: string; id: string | null }>();
  const days = new Map<string, { d: Date; seconds: number; plays: number }>();
  const byTrackId = new Map<string, Track>();
  const byMonth = Array(12).fill(0), byHour = Array(24).fill(0), byWeekday = Array(7).fill(0);
  const sources = new Map<string, number>();
  let plays = 0, seconds = 0;
  let first: Stats["first"] = null;
  for (const l of all) {
    const d = new Date(l.at);
    if (!inPeriod(d)) continue;
    plays++;
    seconds += l.secs;
    first ??= { track: l.track, at: l.at };
    byTrackId.set(l.track.id, l.track);
    if (!tr.has(l.track.id)) tr.set(l.track.id, { ...tally(), track: l.track });
    add(tr.get(l.track.id)!, l);
    for (const a of artistsOf(l.track)) {
      if (!ar.has(a.name)) ar.set(a.name, { ...tally(), id: a.id ?? null });
      add(ar.get(a.name)!, l);
    }
    if (l.track.album) {
      const artist = artistsOf(l.track)[0].name;
      const key = `${l.track.album}|${artist}`;
      if (!al.has(key)) al.set(key, { ...tally(), title: l.track.album, artist, id: l.track.albumId ?? null });
      add(al.get(key)!, l);
    }
    const k = dayKey(d);
    const day = days.get(k) ?? { d, seconds: 0, plays: 0 };
    day.seconds += l.secs;
    day.plays++;
    days.set(k, day);
    byMonth[d.getMonth()] += l.secs;
    byHour[d.getHours()] += l.secs;
    byWeekday[(d.getDay() + 6) % 7] += l.secs;
    sources.set(l.track.source, (sources.get(l.track.source) ?? 0) + l.secs);
  }
  const argmax = (v: number[]) => (v.some((x) => x > 0) ? v.indexOf(Math.max(...v)) : null);
  const busiest = [...days.values()].sort((a, b) => b.seconds - a.seconds)[0];
  const sortedDays = [...days.values()].map((x) => new Date(x.d.getFullYear(), x.d.getMonth(), x.d.getDate()).getTime()).sort((a, b) => a - b);
  let streak = 0, run = 0;
  sortedDays.forEach((t, i) => {
    run = i > 0 && Math.round((t - sortedDays[i - 1]) / 86_400_000) === 1 ? run + 1 : 1;
    streak = Math.max(streak, run);
  });
  const busiestMonth = argmax(byMonth);
  return {
    period, plays, seconds,
    tracks: tr.size, artists: ar.size, albums: al.size,
    newArtists: [...ar.keys()].filter((n) => firstSeen.get(n)! >= from).length,
    hasEarlier: all.some((l) => l.at < from),
    topTracks: [...tr.values()].sort((a, b) => b.plays - a.plays || b.seconds - a.seconds).slice(0, 10)
      .map((x) => ({ track: x.track, plays: x.plays, seconds: x.seconds })),
    topArtists: [...ar].sort((a, b) => b[1].seconds - a[1].seconds).slice(0, 10)
      .map(([name, x]) => ({ name, id: x.id, plays: x.plays, seconds: x.seconds, track: byTrackId.get(best(x))! })),
    topAlbums: [...al.values()].sort((a, b) => b.seconds - a.seconds).slice(0, 10)
      .map((x) => ({ title: x.title, artist: x.artist, id: x.id, plays: x.plays, seconds: x.seconds, track: byTrackId.get(best(x))! })),
    byMonth, byHour, byWeekday,
    bySource: [...sources].map(([source, seconds]) => ({ source, seconds })),
    busiestDay: busiest ? { year: busiest.d.getFullYear(), month: busiest.d.getMonth() + 1, day: busiest.d.getDate(), seconds: busiest.seconds, plays: busiest.plays } : null,
    busiestMonth: busiestMonth == null ? null : busiestMonth + 1,
    favoriteHour: argmax(byHour),
    activeDays: days.size,
    longestStreak: streak,
    first,
    years: [...years].sort((a, b) => b - a),
  };
}

// Recherches récentes : gardées dans le navigateur, pour survivre au rechargement de l'aperçu.
const RECENT_KEY = "yato.demo.recentSearches";
const readRecent = (): string[] => {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
  } catch {
    return [];
  }
};
const writeRecent = (list: string[]) => {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    // Stockage indisponible : la liste vaut pour cette page seulement.
  }
  return list;
};
const cleanQuery = (q: unknown) => String(q).trim().split(/\s+/).join(" ");

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
  listening_stats: ({ period }) => demoStats(period),
  clear_stats: () => ((demoListenList = []), null),
  recent_searches: () => readRecent(),
  add_recent_search: ({ query }) => {
    const q = cleanQuery(query);
    if (!q) return readRecent();
    return writeRecent([q, ...readRecent().filter((x) => x.toLowerCase() !== q.toLowerCase())].slice(0, 10));
  },
  remove_recent_search: ({ query }) => writeRecent(readRecent().filter((x) => x.toLowerCase() !== cleanQuery(query).toLowerCase())),
  clear_recent_searches: () => writeRecent([]),
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
  set_playlist_cover: ({ id, cover }) => ((pl(id).cover = (cover as string | null) || null), copy()),
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
  discover: () => demoDiscover(),
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
