import { open } from "@tauri-apps/plugin-dialog";
import { createMemo, createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { api, bigCoverUrl, formatTime, type Snapshot, type SourceInfo, type Track, type YtDlpStatus } from "./api";
import { IconClose, IconCompass, IconFolderPlus, IconLibrary, IconNote, IconPlay, IconRefresh, IconSearch } from "./icons";
import { Backdrop, NowPlaying, PlayerBar } from "./player";
import { cancelHover, Cover, Empty, hoverPrefetch, Loader, SourceTag, TrackCard, TrackList, YtDlpBadge } from "./ui";

type View = "discover" | "library" | "search";

const EMPTY: Snapshot = {
  track: null, status: "idle", position: 0, duration: null, volume: 0.8, index: null, queueLen: 0, error: null,
};

export default function App() {
  const [view, setView] = createSignal<View>("discover");
  const [sources, setSources] = createSignal<SourceInfo[]>([]);
  const [library, setLibrary] = createSignal<Track[]>([]);
  const [folders, setFolders] = createSignal<string[]>([]);
  const [scanning, setScanning] = createSignal(false);
  const [featured, setFeatured] = createSignal<Track[] | null>(null);
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<Track[] | null>(null);
  const [snap, setSnap] = createSignal<Snapshot>(EMPTY);
  const [toast, setToast] = createSignal<string | null>(null);
  const [ytdlp, setYtdlp] = createSignal<YtDlpStatus>({ state: "unknown" });
  const [expanded, setExpanded] = createSignal(false);
  let searchInput!: HTMLInputElement;

  // ---------- Erreurs ----------
  let toastTimer: number | undefined;
  const fail = (e: unknown) => {
    setToast(String(e));
    clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => setToast(null), 5000);
  };

  // ---------- Position fluide ----------
  // Le cœur envoie l'état 4 fois par seconde ; entre deux, on extrapole à chaque image.
  const [now, setNow] = createSignal(performance.now());
  let snapAt = performance.now();
  const applySnap = (s: Snapshot) => {
    snapAt = performance.now();
    setSnap(s);
  };
  const position = createMemo(() => {
    const s = snap();
    const p = s.status === "playing" ? s.position + (now() - snapAt) / 1000 : s.position;
    return s.duration ? Math.min(p, s.duration) : p;
  });
  let raf = 0;
  const frame = () => {
    if (snap().status === "playing") setNow(performance.now());
    raf = requestAnimationFrame(frame);
  };

  // ---------- Chargement ----------
  const reloadLibrary = async () => {
    const [tracks, dirs] = await Promise.all([api.libraryTracks(), api.libraryFolders()]);
    setLibrary(tracks);
    setFolders(dirs);
  };

  onMount(async () => {
    // Nettoyages enregistrés avant tout `await`, sinon Solid les perd.
    raf = requestAnimationFrame(frame);
    const unlisten = api.onPlayer(applySnap);
    const unlistenYt = api.onYtdlp(setYtdlp);
    api.ytdlpStatus().then(setYtdlp).catch(() => {});
    window.addEventListener("keydown", onKey);
    onCleanup(() => {
      unlisten.then((u) => u());
      unlistenYt.then((u) => u());
      window.removeEventListener("keydown", onKey);
      cancelAnimationFrame(raf);
    });
    try {
      setSources(await api.sources());
      applySnap(await api.state());
      await reloadLibrary();
    } catch (e) {
      fail(e);
    }
    api.featured("audius").then(setFeatured).catch((e) => {
      setFeatured([]);
      fail(e);
    });
  });

  // ---------- Recherche (anti-rebond, sans course entre requêtes) ----------
  let searchTimer: number | undefined;
  let searchId = 0;
  const onSearch = (q: string) => {
    setQuery(q);
    clearTimeout(searchTimer);
    if (!q.trim()) {
      setResults(null);
      if (view() === "search") setView("discover");
      return;
    }
    setView("search");
    setResults(null);
    searchTimer = window.setTimeout(async () => {
      const id = ++searchId;
      try {
        const r = await api.search(q.trim());
        if (id !== searchId) return;
        setResults(r);
        // Le premier résultat YouTube est le plus souvent cliqué : on le prépare tout de suite.
        const first = r.find((t) => t.source === "youtube");
        if (first) api.prefetch(first);
      } catch (e) {
        fail(e);
      }
    }, 220);
  };

  const grouped = createMemo(() => {
    const r = results() ?? [];
    return sources()
      .map((s) => ({ source: s, tracks: r.filter((t) => t.source === s.id) }))
      .filter((g) => g.tracks.length > 0);
  });
  const best = createMemo(() => grouped().find((g) => g.source.id === "youtube") ?? grouped()[0]);

  // ---------- Lecture ----------
  const play = async (tracks: Track[], index: number) => {
    try {
      applySnap(await api.playQueue(tracks, index));
    } catch (e) {
      fail(e);
      applySnap(await api.state());
    }
  };
  const run = async (action: () => Promise<Snapshot>) => {
    try {
      applySnap(await action());
    } catch (e) {
      fail(e);
    }
  };

  function onKey(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement)?.tagName === "INPUT";
    if ((e.ctrlKey || e.metaKey) && (e.key === "f" || e.key === "k")) {
      e.preventDefault();
      setExpanded(false);
      searchInput.focus();
      searchInput.select();
    } else if (e.key === "Escape") {
      if (expanded()) setExpanded(false);
      else if (typing) searchInput.blur();
    } else if (!typing && e.code === "Space") {
      e.preventDefault();
      run(api.toggle);
    } else if (!typing && e.altKey && e.key === "ArrowRight") {
      run(api.next);
    } else if (!typing && e.altKey && e.key === "ArrowLeft") {
      run(api.previous);
    }
  }

  // ---------- Bibliothèque ----------
  const withScan = async (f: () => Promise<unknown>) => {
    setScanning(true);
    try {
      await f();
      await reloadLibrary();
    } catch (e) {
      fail(e);
    } finally {
      setScanning(false);
    }
  };
  const addFolder = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Choisir un dossier de musique" });
    if (typeof dir !== "string") return;
    await withScan(() => api.addFolder(dir));
    setView("library");
  };

  const libraryDuration = createMemo(() => library().reduce((acc, t) => acc + (t.duration ?? 0), 0));
  const isCurrent = (t: Track) => snap().track?.id === t.id;

  return (
    <div classList={{ app: true, "has-track": !!snap().track }}>
      <Backdrop snap={snap()} />

      <aside class="sidebar glass">
        <div class="brand">
          <img src="/logo.svg" alt="" width="30" height="30" />
          <span>Ondes</span>
        </div>
        <nav>
          <button classList={{ nav: true, active: view() === "discover" }} onClick={() => setView("discover")}>
            <IconCompass /> <span>Découvrir</span>
          </button>
          <button classList={{ nav: true, active: view() === "library" }} onClick={() => setView("library")}>
            <IconLibrary /> <span>Bibliothèque</span>
            <Show when={library().length > 0}>
              <span class="count">{library().length}</span>
            </Show>
          </button>
        </nav>
        <div class="sidebar-section">
          <div class="sidebar-title">Dossiers</div>
          <For each={folders()} fallback={<p class="muted small pad">Aucun dossier pour l'instant.</p>}>
            {(dir) => (
              <div class="folder" title={dir}>
                <span>{dir.split(/[\\/]/).filter(Boolean).pop() ?? dir}</span>
                <button class="icon-btn tiny" title="Retirer ce dossier" onClick={() => withScan(() => api.removeFolder(dir))}>
                  <IconClose />
                </button>
              </div>
            )}
          </For>
          <button class="ghost" onClick={addFolder} disabled={scanning()}>
            <IconFolderPlus /> <span>{scanning() ? "Analyse en cours…" : "Ajouter un dossier"}</span>
          </button>
        </div>
        <YtDlpBadge status={ytdlp()} />
      </aside>

      <main class="main">
        <header class="topbar">
          <label class="search glass">
            <IconSearch />
            <input
              ref={searchInput}
              type="search"
              placeholder="Titres, artistes, albums…"
              value={query()}
              onInput={(e) => onSearch(e.currentTarget.value)}
              spellcheck={false}
            />
            <kbd>Ctrl K</kbd>
          </label>
        </header>

        <section class="content">
          <Switch>
            <Match when={view() === "search"}>
              <Show when={results()} fallback={<Loader />}>
                <Show when={grouped().length > 0} fallback={<Empty text={`Aucun résultat pour « ${query()} ».`} />}>
                  <Show when={best()}>
                    {(g) => (
                      <div class="search-top">
                        <div>
                          <h2 class="section-title">Meilleur résultat</h2>
                          <Hero track={g().tracks[0]} playing={isCurrent(g().tracks[0])} onPlay={() => play(g().tracks, 0)} compact />
                        </div>
                        <div class="search-top-list">
                          <h2 class="section-title">Titres</h2>
                          <TrackList
                            tracks={g().tracks.slice(0, 5)}
                            current={snap().track?.id}
                            status={snap().status}
                            onPlay={(i) => play(g().tracks, i)}
                          />
                        </div>
                      </div>
                    )}
                  </Show>
                  <For each={grouped()}>
                    {(g) => {
                      // Les 5 premiers de la source principale sont déjà affichés en haut.
                      const skip = () => (g.source.id === best()?.source.id ? 5 : 0);
                      const rest = () => g.tracks.slice(skip());
                      return (
                        <Show when={rest().length > 0}>
                          <section class="section">
                            <h2 class="section-title">
                              {skip() ? `Plus de résultats ${g.source.name}` : g.source.name}
                              <span class="muted count-inline">{rest().length}</span>
                            </h2>
                            <TrackList
                              tracks={rest()}
                              current={snap().track?.id}
                              status={snap().status}
                              onPlay={(i) => play(g.tracks, i + skip())}
                              showAlbum
                            />
                          </section>
                        </Show>
                      );
                    }}
                  </For>
                </Show>
              </Show>
            </Match>

            <Match when={view() === "library"}>
              <div class="page-head">
                <div>
                  <div class="eyebrow">Ta musique</div>
                  <h1>Bibliothèque</h1>
                  <p class="muted">
                    {library().length} morceaux · {formatTime(libraryDuration())}
                  </p>
                </div>
                <div class="actions">
                  <Show when={library().length > 0}>
                    <button class="primary" onClick={() => play(library(), 0)}>
                      <IconPlay size={16} /> Tout lire
                    </button>
                  </Show>
                  <button class="ghost" onClick={() => withScan(api.rescan)} disabled={scanning() || folders().length === 0}>
                    <IconRefresh /> Rescanner
                  </button>
                </div>
              </div>
              <Show
                when={library().length > 0}
                fallback={
                  <div class="empty-state glass">
                    <IconNote />
                    <h2>Ta musique, sans attendre</h2>
                    <p class="muted">Ajoute un dossier : Ondes lit les tags sur tous les cœurs de ton processeur.</p>
                    <button class="primary" onClick={addFolder} disabled={scanning()}>
                      <IconFolderPlus /> {scanning() ? "Analyse en cours…" : "Ajouter un dossier"}
                    </button>
                  </div>
                }
              >
                <TrackList tracks={library()} current={snap().track?.id} status={snap().status} onPlay={(i) => play(library(), i)} showAlbum />
              </Show>
            </Match>

            <Match when={view() === "discover"}>
              <Show when={featured()} fallback={<Loader />}>
                {(list) => (
                  <Show when={list().length > 0} fallback={<Empty text="Impossible de charger les tendances." />}>
                    <Hero track={list()[0]} playing={isCurrent(list()[0])} onPlay={() => play(list(), 0)} label="N°1 des tendances Audius" />
                    <section class="section">
                      <h2 class="section-title">En ce moment</h2>
                      <div class="grid">
                        <For each={list().slice(1, 13)}>
                          {(t, i) => <TrackCard track={t} playing={isCurrent(t)} onPlay={() => play(list(), i() + 1)} />}
                        </For>
                      </div>
                    </section>
                    <section class="section">
                      <h2 class="section-title">Toutes les tendances</h2>
                      <TrackList tracks={list()} current={snap().track?.id} status={snap().status} onPlay={(i) => play(list(), i)} />
                    </section>
                  </Show>
                )}
              </Show>
            </Match>
          </Switch>
        </section>
      </main>

      <PlayerBar snap={snap()} position={position()} run={run} onExpand={() => setExpanded((v) => !v)} expanded={expanded()} />

      <Show when={expanded() && snap().track}>
        <NowPlaying snap={snap()} position={position()} run={run} onClose={() => setExpanded(false)} />
      </Show>

      <Show when={toast()}>
        <div class="toast" role="alert" onClick={() => setToast(null)}>
          {toast()}
        </div>
      </Show>
    </div>
  );
}

/** Grande carte mise en avant, avec sa pochette en fond flouté. */
function Hero(props: { track: Track; playing: boolean; onPlay: () => void; label?: string; compact?: boolean }) {
  return (
    <div
      classList={{ hero: true, compact: !!props.compact, playing: props.playing }}
      onMouseEnter={() => hoverPrefetch(props.track)}
      onMouseLeave={cancelHover}
    >
      <div class="hero-bg" style={{ "background-image": `url("${bigCoverUrl(props.track) ?? ""}")` }} />
      <Cover track={props.track} size="xl" />
      <div class="hero-body">
        <Show when={props.label}>
          <div class="eyebrow">{props.label}</div>
        </Show>
        <div class="hero-title">{props.track.title}</div>
        <div class="hero-sub">
          <SourceTag source={props.track.source} />
          {props.track.artist}
          <Show when={props.track.album}>{(a) => <span class="muted"> · {a()}</span>}</Show>
        </div>
        <button class="primary hero-play" onClick={props.onPlay}>
          <IconPlay size={18} /> {props.playing ? "En lecture" : "Écouter"}
        </button>
      </div>
    </div>
  );
}
