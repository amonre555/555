// Fiche d'un album : pochette, artistes, année, morceaux.
import { Show } from "solid-js";
import { type AlbumPage, type Snapshot, type Track } from "./api";
import { IconBack, IconMore, IconNote, IconPlay } from "./icons";
import { openTrackMenu } from "./collection";
import { ArtistLinks, Empty, Loader, TrackList } from "./ui";
import { t } from "./i18n";

/** Durée longue lisible : « 1 h 02 min », « 46 min ». */
function longDuration(seconds: number): string {
  const min = Math.round(seconds / 60);
  if (min < 60) return t.minutes(min);
  return t.hours(Math.floor(min / 60), String(min % 60).padStart(2, "0"));
}

/** `page` : `undefined` pendant le chargement, `null` en cas d'échec. */
export function AlbumView(props: {
  page: AlbumPage | null | undefined;
  title: string;
  snap: Snapshot;
  canGoBack: boolean;
  onBack: () => void;
  onPlay: (tracks: Track[], index: number) => void;
}) {
  return (
    <div class="album-page">
      <Show when={props.canGoBack}>
        <button class="ghost back" onClick={props.onBack}>
          <IconBack /> {t.back}
        </button>
      </Show>

      <Show
        when={props.page}
        fallback={
          <Show when={props.page === null} fallback={<Loader />}>
            <Empty text={t.albumError(props.title)} />
          </Show>
        }
      >
        {(p) => {
          const count = () => p().tracks.length;
          const meta = () =>
            [p().year, t.songsCount(count()), p().duration ? longDuration(p().duration!) : null]
              .filter(Boolean)
              .join(" · ");
          return (
            <>
              <header class="artist-head album-head">
                <div class="hero-bg" style={{ "background-image": p().cover ? `url("${p().cover}")` : "none" }} />
                <div class="album-cover hero-art">
                  <Show when={p().cover} fallback={<IconNote />}>
                    {(src) => <img src={src()} alt="" />}
                  </Show>
                </div>
                <div class="artist-body">
                  <div class="eyebrow">{t.kind[p().kind] ?? t.kind.album}</div>
                  <h1 class="album-title" title={p().title}>{p().title}</h1>
                  <div class="album-artists">
                    <ArtistLinks artists={p().artists} />
                  </div>
                  <p class="muted">{meta()}</p>
                  <Show when={count() > 0}>
                    <div class="actions hero-play">
                      <button class="primary" onClick={() => props.onPlay(p().tracks, 0)}>
                        <IconPlay size={18} /> {t.play}
                      </button>
                      <button class="icon-btn" title={t.addAlbumToPlaylist} onClick={(e) => openTrackMenu(e, p().tracks)}>
                        <IconMore />
                      </button>
                    </div>
                  </Show>
                </div>
              </header>

              <section class="section">
                <TrackList
                  tracks={p().tracks}
                  current={props.snap.track?.id}
                  status={props.snap.status}
                  onPlay={(i) => props.onPlay(p().tracks, i)}
                />
                <Show when={p().label}>{(l) => <p class="muted small album-label">℗ {l()}</p>}</Show>
              </section>
            </>
          );
        }}
      </Show>
    </div>
  );
}
