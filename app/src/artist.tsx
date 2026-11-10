// Fiche d'un artiste : photo, titres populaires, albums, singles, artistes similaires.
import { createMemo, createSignal, For, Show } from "solid-js";
import { ALBUM_KIND, api, type Album, type ArtistPage, type Snapshot, type Track } from "./api";
import { IconBack, IconPlay, IconUser } from "./icons";
import { openAlbum, openArtist } from "./nav";
import { Empty, Loader, TrackList } from "./ui";

/** `page` : `undefined` pendant le chargement, `null` en cas d'échec. */
export function ArtistView(props: {
  page: ArtistPage | null | undefined;
  name: string;
  snap: Snapshot;
  canGoBack: boolean;
  onBack: () => void;
  onPlay: (tracks: Track[], index: number) => void;
  onError: (e: unknown) => void;
}) {
  const albums = createMemo(() => props.page?.albums.filter((a) => a.kind === "album" || a.kind === "compile") ?? []);
  const singles = createMemo(() => props.page?.albums.filter((a) => a.kind !== "album" && a.kind !== "compile") ?? []);
  const [showAllTop, setShowAllTop] = createSignal(false);

  return (
    <div class="artist-page">
      <Show when={props.canGoBack}>
        <button class="ghost back" onClick={props.onBack}>
          <IconBack /> Retour
        </button>
      </Show>

      <Show
        when={props.page}
        fallback={
          <Show when={props.page === null} fallback={<Loader />}>
            <Empty text={`Impossible de charger la fiche de ${props.name}.`} />
          </Show>
        }
      >
        {(p) => (
          <>
            <header class="artist-head">
              <div class="hero-bg" style={{ "background-image": p().picture ? `url("${p().picture}")` : "none" }} />
              <div class="artist-photo hero-art">
                <Show when={p().picture} fallback={<IconUser />}>
                  {(src) => <img src={src()} alt="" />}
                </Show>
              </div>
              <div class="artist-body">
                <div class="eyebrow">Artiste</div>
                <h1 class="artist-name">{p().name}</h1>
                <Show when={p().top.length > 0}>
                  <button class="primary hero-play" onClick={() => props.onPlay(p().top, 0)}>
                    <IconPlay size={18} /> Lecture
                  </button>
                </Show>
              </div>
            </header>

            <Show when={p().top.length > 0}>
              <section class="section">
                <h2 class="section-title">Titres populaires</h2>
                <TrackList
                  tracks={showAllTop() ? p().top : p().top.slice(0, 5)}
                  current={props.snap.track?.id}
                  status={props.snap.status}
                  onPlay={(i) => props.onPlay(p().top, i)}
                  showAlbum
                />
                <Show when={p().top.length > 5}>
                  <button class="link-btn" onClick={() => setShowAllTop((v) => !v)}>
                    {showAllTop() ? "Afficher moins" : "Afficher plus"}
                  </button>
                </Show>
              </section>
            </Show>

            <AlbumGrid title="Albums" albums={albums()} onPlay={props.onPlay} onError={props.onError} />
            <AlbumGrid title="Singles et EP" albums={singles()} onPlay={props.onPlay} onError={props.onError} />

            <Show when={p().related.length > 0}>
              <section class="section">
                <h2 class="section-title">Artistes similaires</h2>
                <div class="grid artists-grid">
                  <For each={p().related}>
                    {(a) => (
                      <button
                        class="card artist-card"
                        onClick={(e) => openArtist({ id: a.id, name: a.name }, e.currentTarget.querySelector(".artist-photo") ?? undefined)}
                      >
                        <div class="artist-photo sm">
                          <Show when={a.picture} fallback={<IconUser />}>
                            {(src) => <img src={src()} alt="" loading="lazy" />}
                          </Show>
                        </div>
                        <div class="card-title">{a.name}</div>
                        <div class="card-sub">Artiste</div>
                      </button>
                    )}
                  </For>
                </div>
              </section>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}

/** Grille d'albums : un clic ouvre la fiche de l'album, le bouton lecture le lance directement. */
function AlbumGrid(props: {
  title: string;
  albums: Album[];
  onPlay: (tracks: Track[], index: number) => void;
  onError: (e: unknown) => void;
}) {
  const [loading, setLoading] = createSignal<string | null>(null);
  const playAlbum = async (a: Album) => {
    setLoading(a.id);
    try {
      const { tracks } = await api.album({ id: a.id, title: a.title });
      if (tracks.length) props.onPlay(tracks, 0);
    } catch (e) {
      props.onError(e);
    } finally {
      setLoading(null);
    }
  };
  return (
    <Show when={props.albums.length > 0}>
      <section class="section">
        <h2 class="section-title">
          {props.title}
          <span class="muted count-inline">{props.albums.length}</span>
        </h2>
        <div class="grid">
          <For each={props.albums}>
            {(a, i) => (
              <button
                classList={{ card: true, loading: loading() === a.id }}
                style={{ "--i": Math.min(i(), 16) }}
                onClick={(e) => openAlbum({ id: a.id, title: a.title }, e.currentTarget.querySelector(".cover") ?? undefined)}
                title={`Voir l'album « ${a.title} »`}
              >
                <div class="card-art">
                  <div class="cover lg">
                    <Show when={a.cover}>{(src) => <img src={src()} alt="" loading="lazy" decoding="async" />}</Show>
                  </div>
                  <span
                    class="card-play"
                    role="button"
                    title={`Lire « ${a.title} »`}
                    onClick={(e) => {
                      e.stopPropagation();
                      playAlbum(a);
                    }}
                  >
                    <IconPlay size={20} />
                  </span>
                </div>
                <div class="card-title">{a.title}</div>
                <div class="card-sub">
                  {[a.year, ALBUM_KIND[a.kind] ?? a.kind].filter(Boolean).join(" · ")}
                </div>
              </button>
            )}
          </For>
        </div>
      </section>
    </Show>
  );
}
