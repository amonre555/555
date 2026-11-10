import { open } from "@tauri-apps/plugin-dialog";
import { createEffect, createMemo, createSignal, For, Match, on, onCleanup, onMount, Show, Switch, type JSX } from "solid-js";
import { api, bigCoverUrl, formatTime, type AlbumPage, type ArtistPage, type Snapshot, type SourceInfo, type Track, type YtDlpStatus } from "./api";
import { AlbumView } from "./album";
import { ArtistView } from "./artist";
import { nameForTransition, transition } from "./motion";
import { pageRequest, type Page } from "./nav";
import { IconClose, IconCompass, IconFolderPlus, IconHeart, IconLibrary, IconLink, IconNote, IconPlay, IconPlus, IconRefresh, IconSearch, IconSettings } from "./icons";
import { collection, createPlaylist, dismissToast, loadCollection, notify, recentTracks, toast } from "./collection";
import { ImportDialog, SettingsView } from "./settings";
import { LikedView, PlaylistView } from "./playlist";
import { UpdateCard } from "./update";
import { Backdrop, NowPlaying, PlayerBar } from "./player";
import { AlbumLink, ArtistLinks, cancelHover, Cover, Empty, hoverPrefetch, Loader, PlaylistArt, SourceTag, TrackCard, TrackList, TrackMenuHost, YtDlpBadge } from "./ui";

type View = "discover" | "library" | "liked" | "playlist" | "settings" | "search" | "artist" | "album";

const EMPTY: Snapshot = {
  track: null, status: "idle", position: 0, duration: null, volume: 0.8, index: null, queueLen: 0, error: null, shuffle: false, repeat: "off",
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
  const [ytdlp, setYtdlp] = createSignal<YtDlpStatus>({ state: "unknown" });
  /**
   * Vue « En lecture » : fermée, en ouverture, ouverte, en fermeture. Les états intermédiaires
   * laissent l'app visible dessous pendant l'animation ; une fois ouverte, l'app est masquée
   * (rien à redessiner sous la vue).
   */
  const [np, setNp] = createSignal<"closed" | "opening" | "open" | "closing">("closed");
  const expanded = () => np() === "opening" || np() === "open";
  let npTimer: number | undefined;
  let searchInput!: HTMLInputElement;

  // ---------- Fiches (artistes, albums) ----------
  // Historique pour « Retour » : la vue d'où l'on vient, et la fiche qu'elle montrait.
  const [page, setPage] = createSignal<Page | null>(null);
  const [artistPage, setArtistPage] = createSignal<ArtistPage | null | undefined>(undefined);
  const [albumPage, setAlbumPage] = createSignal<AlbumPage | null | undefined>(undefined);
  const [history, setHistory] = createSignal<{ view: View; page: Page | null }[]>([]);
  /** Une fiche est en cours de chargement (fine barre lumineuse en haut). */
  const [navigating, setNavigating] = createSignal(false);
  /** Playlist affichée (vue « playlist »), et si son nom est à saisir (elle vient d'être créée). */
  const [playlistId, setPlaylistId] = createSignal<string | null>(null);
  const [naming, setNaming] = createSignal(false);
  /** Fenêtre « Importer une playlist » ouverte. */
  const [importing, setImporting] = createSignal(false);
  let content!: HTMLElement;
  let pageLoad = 0;

  /** Change de vue avec une transition ; la page repart du haut. */
  const switchTo = (update: () => void, kind: "page" | "back" = "page") =>
    transition(() => {
      update();
      content.scrollTop = 0;
    }, kind);

  /**
   * Ouvre une fiche. On attend ses données (au plus 450 ms) avant de changer de vue : la
   * transition montre alors directement la fiche, et l'image cliquée (`from`) se transforme en
   * sa grande image. Si c'est plus long, la fiche s'affiche en chargement puis se remplit.
   */
  const showPage = async (p: Page, kind: "page" | "back" = "page", from?: Element) => {
    const id = ++pageLoad;
    setNavigating(true);
    const request: Promise<ArtistPage | AlbumPage> = p.kind === "artist" ? api.artist(p.artist) : api.album(p.album);
    const quick = await Promise.race([
      request.then((data) => ({ data }), (error) => ({ error })),
      new Promise<null>((done) => setTimeout(() => done(null), 450)),
    ]);
    if (id !== pageLoad) return;
    const setData = (d: ArtistPage | AlbumPage | null | undefined) => {
      if (p.kind === "artist") setArtistPage(d as ArtistPage | null | undefined);
      else setAlbumPage(d as AlbumPage | null | undefined);
    };
    nameForTransition(from, "hero-art");
    switchTo(() => {
      setPage(p);
      setData(quick && "data" in quick ? quick.data : quick ? null : undefined);
      setView(p.kind);
    }, kind);
    if (quick && "error" in quick) fail(quick.error);
    if (!quick) {
      try {
        const data = await request;
        if (id === pageLoad) setData(data);
      } catch (e) {
        if (id === pageLoad) setData(null);
        fail(e);
      }
    }
    if (id === pageLoad) setNavigating(false);
  };
  const samePage = (a: Page | null, b: Page) =>
    !!a && a.kind === b.kind &&
    (a.kind === "artist"
      ? a.artist.name === (b as typeof a).artist.name
      : a.album.title === (b as typeof a).album.title && (a.album.id ?? null) === ((b as typeof a).album.id ?? null));
  createEffect(
    on(pageRequest, (req) => {
      if (!req) return;
      if (expanded()) setNowPlaying(false);
      const onPage = view() === "artist" || view() === "album";
      if (!(onPage && samePage(page(), req.page))) {
        setHistory((h) => [...h, { view: view(), page: onPage ? page() : null }]);
      }
      showPage(req.page, "page", req.from);
    }, { defer: true }),
  );
  const goBack = () => {
    const h = history();
    const last = h[h.length - 1];
    if (!last) return switchTo(() => setView("discover"), "back");
    setHistory(h.slice(0, -1));
    if (last.page) showPage(last.page, "back");
    else switchTo(() => setView(last.view), "back");
  };
  /** Changement de page depuis la barre du haut : l'historique repart de zéro. */
  const goTo = (v: View) => {
    pageLoad++;
    setNavigating(false);
    setHistory([]);
    if (view() !== v) switchTo(() => setView(v));
  };
  const openPlaylist = (id: string, name = false) => {
    if (expanded()) setNowPlaying(false);
    pageLoad++;
    setNavigating(false);
    setHistory([]);
    if (view() === "playlist" && playlistId() === id) return setNaming(name);
    switchTo(() => {
      setPlaylistId(id);
      setNaming(name);
      setView("playlist");
    });
  };
  const newPlaylist = async () => {
    const id = await createPlaylist();
    if (id) openPlaylist(id, true);
  };
  /** Ouvre ou ferme la vue « En lecture » ; les animations sont dans NowPlaying et styles.css. */
  const setNowPlaying = (open: boolean) => {
    if (open === expanded()) return;
    clearTimeout(npTimer);
    if (open) {
      setNp("opening");
      npTimer = window.setTimeout(() => setNp("open"), 700);
    } else {
      setNp("closing");
      npTimer = window.setTimeout(() => setNp("closed"), 460);
    }
  };

  // ---------- Erreurs ----------
  const fail = (e: unknown) => notify(String(e), "error");

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
      await Promise.all([reloadLibrary(), loadCollection()]);
    } catch (e) {
      fail(e);
    }
    api.featured("deezer").then(setFeatured).catch((e) => {
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
      if (view() === "search") switchTo(() => setView("discover"), "back");
      return;
    }
    setResults(null);
    if (view() !== "search") {
      pageLoad++;
      setNavigating(false);
      switchTo(() => setView("search"));
    }
    searchTimer = window.setTimeout(async () => {
      const id = ++searchId;
      try {
        const r = await api.search(q.trim());
        if (id !== searchId) return;
        setResults(r);
        // Le premier résultat en ligne est le plus souvent cliqué : on le prépare tout de suite.
        const first = r.find((t) => t.source === "deezer" || t.source === "youtube");
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
  const best = createMemo(() => grouped().find((g) => g.source.id === "deezer") ?? grouped()[0]);

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
      setNowPlaying(false);
      searchInput.focus();
      searchInput.select();
    } else if (e.key === "Escape") {
      if (expanded()) setNowPlaying(false);
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
    goTo("library");
  };

  const libraryDuration = createMemo(() => library().reduce((acc, t) => acc + (t.duration ?? 0), 0));
  const isCurrent = (t: Track) => snap().track?.id === t.id;

  return (
    <div classList={{ app: true, "has-track": !!snap().track, "playing-on": snap().status === "playing", expanded: np() === "open", "np-moving": np() === "opening" || np() === "closing" }}>
      <Backdrop snap={snap()} />

      <aside class="sidebar">
        <button class="brand" onClick={() => goTo("discover")} title="Accueil">
          <img src="/logo.svg" alt="" width="30" height="30" />
          <span>yato</span>
        </button>
        <SideNav
          active={view() === "playlist" ? `pl:${playlistId()}` : view()}
          items={[
            { id: "discover", label: "Découvrir", icon: <IconCompass /> },
            { id: "library", label: "Bibliothèque", icon: <IconLibrary />, count: library().length },
            { id: "liked", label: "Titres likés", icon: <IconHeart />, count: collection().liked.length },
            ...collection().playlists.map((p, i) => ({
              id: `pl:${p.id}`,
              label: p.name,
              icon: <PlaylistArt id={p.id} class="nav-art" />,
              count: p.tracks.length,
              before: i === 0 ? <PlaylistsHeading onNew={newPlaylist} onImport={() => setImporting(true)} /> : undefined,
            })),
          ]}
          onSelect={(id) => (id.startsWith("pl:") ? openPlaylist(id.slice(3)) : goTo(id as View))}
        />
        <Show when={collection().playlists.length === 0}>
          <div class="sidenav-extra">
            <PlaylistsHeading onNew={newPlaylist} onImport={() => setImporting(true)} />
            <button class="nav new-pl" onClick={newPlaylist}>
              <IconPlus size={20} />
              <span>Nouvelle playlist</span>
            </button>
          </div>
        </Show>
        <button classList={{ nav: true, "nav-settings": true, active: view() === "settings" }} title="Réglages" onClick={() => goTo("settings")}>
          <IconSettings />
          <span>Réglages</span>
        </button>
        <UpdateCard />
        <YtDlpBadge status={ytdlp()} />
      </aside>
      <div classList={{ "nav-progress": true, on: navigating() }} aria-hidden="true" />

      <main class="main">
        <header class="topbar">
          <label class="search">
            <IconSearch />
            <input
              ref={searchInput}
              type="search"
              placeholder="Titres, artistes, albums…"
              value={query()}
              onInput={(e) => onSearch(e.currentTarget.value)}
              spellcheck={false}
            />
            <kbd>⌘K</kbd>
          </label>
        </header>
        <section class="content" ref={content}>
          <div class="page">
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
                              {skip() ? (g.source.id === "deezer" ? "Plus de titres" : `Plus de résultats ${g.source.name}`) : g.source.name}
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
                  <h1>Ta musique</h1>
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
              <div class="folders">
                <For each={folders()}>
                  {(dir) => (
                    <span class="chip" title={dir}>
                      <span>{dir.split(/[\\/]/).filter(Boolean).pop() ?? dir}</span>
                      <button class="chip-x" title="Retirer ce dossier" onClick={() => withScan(() => api.removeFolder(dir))}>
                        <IconClose />
                      </button>
                    </span>
                  )}
                </For>
                <button class="chip add" onClick={addFolder} disabled={scanning()}>
                  <IconFolderPlus /> {scanning() ? "Analyse en cours…" : "Ajouter un dossier"}
                </button>
              </div>
              <Show
                when={library().length > 0}
                fallback={
                  <div class="empty-state">
                    <IconNote />
                    <h2>Ta musique, sans attendre</h2>
                    <p class="muted">Ajoute un dossier : yato lit les tags sur tous les cœurs de ton processeur.</p>
                    <button class="primary" onClick={addFolder} disabled={scanning()}>
                      <IconFolderPlus /> {scanning() ? "Analyse en cours…" : "Ajouter un dossier"}
                    </button>
                  </div>
                }
              >
                <TrackList tracks={library()} current={snap().track?.id} status={snap().status} onPlay={(i) => play(library(), i)} showAlbum />
              </Show>
            </Match>

            <Match when={view() === "artist"}>
              <ArtistView
                page={artistPage()}
                name={(() => { const p = page(); return p?.kind === "artist" ? p.artist.name : ""; })()}
                snap={snap()}
                canGoBack={history().length > 0}
                onBack={goBack}
                onPlay={play}
                onError={fail}
              />
            </Match>

            <Match when={view() === "album"}>
              <AlbumView
                page={albumPage()}
                title={(() => { const p = page(); return p?.kind === "album" ? p.album.title : ""; })()}
                snap={snap()}
                canGoBack={history().length > 0}
                onBack={goBack}
                onPlay={play}
              />
            </Match>

            <Match when={view() === "liked"}>
              <LikedView snap={snap()} onPlay={play} />
            </Match>

            <Match when={view() === "playlist" && playlistId()}>
              {(id) => <PlaylistView id={id()} editing={naming()} snap={snap()} onPlay={play} onDeleted={() => goTo("library")} />}
            </Match>

            <Match when={view() === "settings"}>
              <SettingsView onImport={() => setImporting(true)} onLibrary={() => goTo("library")} />
            </Match>

            <Match when={view() === "discover"}>
              <Show when={featured()} fallback={<Loader />}>
                {(list) => (
                  <Show when={list().length > 0} fallback={<Empty text="Impossible de charger le top du moment." />}>
                    <Hero track={list()[0]} playing={isCurrent(list()[0])} onPlay={() => play(list(), 0)} label="N°1 du moment" />
                    <Show when={recentTracks().length > 0}>
                      <section class="section">
                        <h2 class="section-title">Écoutés récemment</h2>
                        <div class="grid recent">
                          <For each={recentTracks().slice(0, 6)}>
                            {(t, i) => <TrackCard track={t} index={i()} playing={isCurrent(t)} onPlay={() => play(recentTracks(), i())} />}
                          </For>
                        </div>
                      </section>
                    </Show>
                    <section class="section">
                      <h2 class="section-title">En ce moment</h2>
                      <div class="grid">
                        <For each={list().slice(1, 13)}>
                          {(t, i) => <TrackCard track={t} index={i()} playing={isCurrent(t)} onPlay={() => play(list(), i() + 1)} />}
                        </For>
                      </div>
                    </section>
                    <section class="section">
                      <h2 class="section-title">Tout le top</h2>
                      <TrackList tracks={list()} current={snap().track?.id} status={snap().status} onPlay={(i) => play(list(), i)} />
                    </section>
                  </Show>
                )}
              </Show>
            </Match>
          </Switch>
          </div>
        </section>
      </main>

      <PlayerBar snap={snap()} position={position()} run={run} onExpand={() => setNowPlaying(!expanded())} expanded={expanded()} />

      <Show when={np() !== "closed" && snap().track}>
        <NowPlaying snap={snap()} position={position()} run={run} leaving={np() === "closing"} onClose={() => setNowPlaying(false)} />
      </Show>

      <TrackMenuHost onOpenPlaylist={(id) => openPlaylist(id)} />

      <Show when={importing()}>
        <ImportDialog
          onClose={() => setImporting(false)}
          onDone={(id) => {
            setImporting(false);
            openPlaylist(id);
          }}
        />
      </Show>

      <Show when={toast()} keyed>
        {(t) => (
          <div classList={{ toast: true, info: t.kind === "info" }} role={t.kind === "error" ? "alert" : "status"} onClick={dismissToast}>
            {t.text}
          </div>
        )}
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
          <ArtistLinks track={props.track} />
          <Show when={props.track.album}>
            <span class="muted">
              {" · "}
              <AlbumLink track={props.track} />
            </span>
          </Show>
        </div>
        <button class="primary hero-play" onClick={props.onPlay}>
          <IconPlay size={18} /> {props.playing ? "En lecture" : "Écouter"}
        </button>
      </div>
    </div>
  );
}

/**
 * Navigation de la barre latérale : une pastille arrondie glisse (avec un léger rebond) jusqu'à
 * l'entrée active. Hors de ces vues (recherche, fiches), elle s'efface.
 */
function SideNav<T extends string>(props: {
  active: string;
  items: { id: T; label: string; icon: JSX.Element; count?: number; before?: JSX.Element }[];
  onSelect: (id: T) => void;
}) {
  const buttons = new Map<string, HTMLButtonElement>();
  const [mark, setMark] = createSignal<{ y: number; h: number } | null>(null);
  const measure = () => {
    const b = buttons.get(props.active);
    setMark(b ? { y: b.offsetTop, h: b.offsetHeight } : null);
  };
  // Les entrées peuvent bouger (playlist ajoutée, renommée…) : on remesure aussi dans ce cas.
  createEffect(on(() => [props.active, props.items], () => requestAnimationFrame(measure)));
  onMount(() => window.addEventListener("resize", measure));
  onCleanup(() => window.removeEventListener("resize", measure));

  return (
    <nav class="sidenav">
      <span
        classList={{ "nav-pill": true, on: !!mark() }}
        style={mark() ? { "--y": `${mark()!.y}px`, "--h": `${mark()!.h}px` } : {}}
        aria-hidden="true"
      />
      <For each={props.items}>
        {(item) => (
          <>
            {item.before}
            <button
              ref={(el) => buttons.set(item.id, el)}
              classList={{ nav: true, active: props.active === item.id }}
              title={item.label}
              onClick={() => props.onSelect(item.id)}
            >
              {item.icon}
              <span>{item.label}</span>
              <Show when={item.count}>
                <span class="nav-count">{item.count}</span>
              </Show>
            </button>
          </>
        )}
      </For>
    </nav>
  );
}

/** Titre « Playlists » de la barre latérale, avec l'import et la création. */
function PlaylistsHeading(props: { onNew: () => void; onImport: () => void }) {
  return (
    <div class="nav-heading">
      <span>Playlists</span>
      <div class="nav-heading-actions">
        <button class="icon-btn" title="Importer une playlist (Spotify, Deezer, YouTube Music)" onClick={props.onImport}>
          <IconLink />
        </button>
        <button class="icon-btn" title="Nouvelle playlist" onClick={props.onNew}>
          <IconPlus />
        </button>
      </div>
    </div>
  );
}
