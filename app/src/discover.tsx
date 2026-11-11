// Page « Découvrir » : recommandations calculées par le cœur (voir `discover.rs`).
// Les sections arrivent tout de suite depuis le cache, puis une à une quand elles sont recalculées.
import { createSignal, For, Match, Show, Switch, type JSX } from "solid-js";
import { api, type DiscoverSection, type Release, type Snapshot, type Suggestion, type Track } from "./api";
import { IconPlay, IconUser } from "./icons";
import { openAlbum, openArtist } from "./nav";
import { Carousel, Empty, TrackCard, TrackList } from "./ui";
import { t } from "./i18n";

// ---------- Données (gardées en quittant la page : le retour est instantané) ----------

const [sections, setSections] = createSignal<DiscoverSection[] | null>(null);
let loadedAt = 0;
let loading: Promise<DiscoverSection[]> | null = null;

/** Charge les sections (instantané : cache du cœur). Les sections recalculées arrivent ensuite. */
export function loadDiscover(force = false): Promise<DiscoverSection[]> {
  loading ??= api
    .discover(force)
    .then((list) => {
      loadedAt = Date.now();
      setSections(list);
      return list;
    })
    .finally(() => (loading = null));
  return loading;
}

/** Retour sur la page après un moment : de quoi rafraîchir (le cœur ne recalcule que le périmé). */
export function refreshDiscover() {
  if (Date.now() - loadedAt > 10 * 60_000) loadDiscover().catch(() => {});
}

/** Reçoit les sections recalculées. À appeler une fois au lancement. */
export function listenDiscover() {
  return api.onDiscoverSection((s) => setSections((list) => list?.map((x) => (x.id === s.id ? s : x)) ?? list));
}

export const discoverSections = sections;

// ---------- Affichage ----------

const hasContent = (s: DiscoverSection) => s.tracks.length + s.albums.length + s.artists.length > 0;

export function DiscoverView(props: {
  snap: Snapshot;
  onPlay: (tracks: Track[], index: number) => void;
  onError: (e: unknown) => void;
  /** Ce qui précède les recommandations (récap, écoutés récemment). */
  children?: JSX.Element;
}) {
  const isCurrent = (tr: Track) => props.snap.track?.id === tr.id;
  const list = () => sections() ?? [];
  const failed = () => list().length > 0 && list().every((s) => s.ready && !hasContent(s));
  const newcomer = () => list().length > 0 && !list().some((s) => s.kind === "mix");

  const tracksRow = (s: DiscoverSection, max = 16) => (
    <Carousel>
      <For each={s.tracks.slice(0, max)}>
        {(tr, i) => <TrackCard track={tr} index={i()} playing={isCurrent(tr)} onPlay={() => props.onPlay(s.tracks, i())} />}
      </For>
    </Carousel>
  );

  return (
    <>
      {props.children}
      <Show when={newcomer()}>
        <div class="discover-welcome">
          <h2>{t.welcomeTitle}</h2>
          <p class="muted">{t.welcomeText}</p>
        </div>
      </Show>
      <Show when={!failed()} fallback={<Empty text={t.featuredError} />}>
        <For each={list()}>
          {(s) => (
            <Show when={!s.ready || hasContent(s)}>
              <section class="section" classList={{ pending: !s.ready }}>
                <Switch>
                  <Match when={s.kind === "mix"}>
                    <h2 class="section-title">
                      {t.dailyMix}
                      <Show when={s.ready}>
                        <button class="link-btn section-link mix-play" onClick={() => props.onPlay(s.tracks, 0)}>
                          <IconPlay size={14} /> {t.playMix}
                        </button>
                      </Show>
                    </h2>
                    <Show when={s.ready}>
                      <p class="section-sub">{t.dailyMixSub(s.tracks.length)}</p>
                    </Show>
                    <Show when={s.ready} fallback={<Placeholder />}>{tracksRow(s, 20)}</Show>
                  </Match>

                  <Match when={s.kind === "because"}>
                    <h2 class="section-title because-title">
                      <span>
                        {t.becauseYouListen}{" "}
                        <Show when={s.artist} fallback="…">
                          {(a) => (
                            <button class="because-artist" onClick={() => openArtist(a())}>
                              {a().name}
                            </button>
                          )}
                        </Show>
                      </span>
                    </h2>
                    <Show when={s.ready} fallback={<Placeholder />}>{tracksRow(s)}</Show>
                  </Match>

                  <Match when={s.kind === "releases" || s.kind === "chartAlbums"}>
                    <h2 class="section-title">{s.kind === "releases" ? t.newReleases : t.chartAlbums}</h2>
                    <Show when={s.ready} fallback={<Placeholder />}>
                      <Carousel>
                        <For each={s.albums}>
                          {(a, i) => <ReleaseCard release={a} index={i()} dated={s.kind === "releases"} onPlay={props.onPlay} onError={props.onError} />}
                        </For>
                      </Carousel>
                    </Show>
                  </Match>

                  <Match when={s.kind === "artists" || s.kind === "chartArtists"}>
                    <h2 class="section-title">{s.kind === "artists" ? t.artistsToDiscover : t.chartArtists}</h2>
                    <Show when={s.ready} fallback={<Placeholder round />}>
                      <Carousel class="artists-row">
                        <For each={s.artists}>{(a, i) => <SuggestionCard artist={a} index={i()} />}</For>
                      </Carousel>
                    </Show>
                  </Match>

                  <Match when={s.kind === "rediscover"}>
                    <h2 class="section-title">{t.rediscover}</h2>
                    <p class="section-sub">{t.rediscoverSub}</p>
                    {tracksRow(s)}
                  </Match>

                  <Match when={s.kind === "charts"}>
                    <Show
                      when={newcomer()}
                      fallback={
                        <>
                          <h2 class="section-title">{t.trending}</h2>
                          <Show when={s.ready} fallback={<Placeholder />}>
                            <ChartList tracks={s.tracks} snap={props.snap} onPlay={props.onPlay} />
                          </Show>
                        </>
                      }
                    >
                      <h2 class="section-title">{t.rightNow}</h2>
                      <Show when={s.ready} fallback={<Placeholder />}>
                        {tracksRow(s, 12)}
                        <h2 class="section-title chart-full">{t.fullChart}</h2>
                        <TrackList tracks={s.tracks} current={props.snap.track?.id} status={props.snap.status} onPlay={(i) => props.onPlay(s.tracks, i)} />
                      </Show>
                    </Show>
                  </Match>
                </Switch>
              </section>
            </Show>
          )}
        </For>
      </Show>
    </>
  );
}

/** Tendances : dix titres, et le reste à la demande. */
function ChartList(props: { tracks: Track[]; snap: Snapshot; onPlay: (tracks: Track[], index: number) => void }) {
  const [all, setAll] = createSignal(false);
  return (
    <>
      <TrackList
        tracks={all() ? props.tracks : props.tracks.slice(0, 10)}
        current={props.snap.track?.id}
        status={props.snap.status}
        onPlay={(i) => props.onPlay(props.tracks, i)}
      />
      <Show when={props.tracks.length > 10}>
        <button class="link-btn" onClick={() => setAll((v) => !v)}>
          {all() ? t.showLess : t.showMore}
        </button>
      </Show>
    </>
  );
}

/** En attendant une section : une rangée de cartes vides. */
function Placeholder(props: { round?: boolean }) {
  return (
    <Carousel>
      <For each={[0, 1, 2, 3, 4, 5, 6]}>
        {() => (
          <div class="card skeleton" classList={{ round: props.round }} aria-hidden="true">
            <div class="skeleton-art" />
            <div class="skeleton-line" />
            <div class="skeleton-line short" />
          </div>
        )}
      </For>
    </Carousel>
  );
}

/** Sortie : un clic ouvre l'album, le bouton lecture le lance. */
function ReleaseCard(props: {
  release: Release;
  index: number;
  dated: boolean;
  onPlay: (tracks: Track[], index: number) => void;
  onError: (e: unknown) => void;
}) {
  const [busy, setBusy] = createSignal(false);
  const a = () => props.release;
  const play = async (e: MouseEvent) => {
    e.stopPropagation();
    setBusy(true);
    try {
      const { tracks } = await api.album({ id: a().id, title: a().title, artist: a().artist });
      if (tracks.length) props.onPlay(tracks, 0);
    } catch (err) {
      props.onError(err);
    } finally {
      setBusy(false);
    }
  };
  const when = () => {
    if (!props.dated || !a().date) return t.kind[a().kind] ?? a().kind;
    const kind = t.kind[a().kind] ?? a().kind;
    return `${kind} · ${t.releasedOn(new Date(`${a().date}T12:00:00`))}`;
  };
  return (
    <button
      classList={{ card: true, loading: busy() }}
      style={{ "--i": Math.min(props.index, 16) }}
      onClick={(e) => openAlbum({ id: a().id, title: a().title, artist: a().artist }, e.currentTarget.querySelector(".cover") ?? undefined)}
      title={t.seeAlbum(a().title)}
    >
      <div class="card-art">
        <div class="cover lg">
          <Show when={a().cover}>{(src) => <img src={src()} alt="" loading="lazy" decoding="async" />}</Show>
        </div>
        <span class="card-play" role="button" title={t.playAlbum(a().title)} onClick={play}>
          <IconPlay size={20} />
        </span>
      </div>
      <div class="card-title">{a().title}</div>
      <div class="card-sub">{a().artist}</div>
      <div class="card-note">{when()}</div>
    </button>
  );
}

function SuggestionCard(props: { artist: Suggestion; index: number }) {
  const a = () => props.artist;
  return (
    <button
      class="card artist-card"
      style={{ "--i": Math.min(props.index, 16) }}
      onClick={(e) => openArtist({ id: a().id, name: a().name }, e.currentTarget.querySelector(".artist-photo") ?? undefined)}
    >
      <div class="artist-photo sm">
        <Show when={a().picture} fallback={<IconUser />}>
          {(src) => <img src={src()} alt="" loading="lazy" decoding="async" />}
        </Show>
      </div>
      <div class="card-title">{a().name}</div>
      <div class="card-sub">{a().near ? t.nearArtist(a().near!) : t.artist}</div>
    </button>
  );
}
