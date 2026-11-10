// Pages de la collection : titres likés et playlists.
import { createEffect, createSignal, on, Show } from "solid-js";
import { formatTime, type Snapshot, type Track } from "./api";
import { collection, deletePlaylist, moveInPlaylist, openTrackMenu, playlistById, renamePlaylist } from "./collection";
import { IconEdit, IconHeart, IconMore, IconPlay, IconPlaylist, IconTrash } from "./icons";
import { PlaylistArt, TrackList } from "./ui";
import { t } from "./i18n";

type Common = { snap: Snapshot; onPlay: (tracks: Track[], index: number) => void };

const total = (tracks: Track[]) => tracks.reduce((a, t) => a + (t.duration ?? 0), 0);
const countLabel = (n: number) => t.songsCount(n);

export function LikedView(props: Common) {
  const tracks = () => collection().liked;
  return (
    <div class="album-page">
      <header class="artist-head album-head">
        <PlaylistArt liked class="xl" />
        <div class="artist-body">
          <div class="eyebrow">{t.playlist}</div>
          <h1 class="album-title">{t.liked}</h1>
          <p class="muted">
            {countLabel(tracks().length)}
            <Show when={tracks().length > 0}> · {formatTime(total(tracks()))}</Show>
          </p>
          <Show when={tracks().length > 0}>
            <div class="actions hero-play">
              <button class="primary" onClick={() => props.onPlay(tracks(), 0)}>
                <IconPlay size={18} /> {t.play}
              </button>
            </div>
          </Show>
        </div>
      </header>
      <section class="section">
        <Show
          when={tracks().length > 0}
          fallback={
            <div class="empty-state">
              <IconHeart size={26} />
              <h2>{t.noLiked}</h2>
              <p class="muted">{t.noLikedText}</p>
            </div>
          }
        >
          <TrackList tracks={tracks()} current={props.snap.track?.id} status={props.snap.status} onPlay={(i) => props.onPlay(tracks(), i)} showAlbum />
        </Show>
      </section>
    </div>
  );
}

/** `editing` : la playlist vient d'être créée, son nom est prêt à être changé. */
export function PlaylistView(props: Common & { id: string; editing: boolean; onDeleted: () => void }) {
  const playlist = () => playlistById(props.id);
  const [editing, setEditing] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal(false);
  createEffect(on(() => [props.id, props.editing] as const, ([, e]) => {
    setEditing(e);
    setConfirmDelete(false);
  }));
  let input: HTMLInputElement | undefined;
  createEffect(() => {
    if (editing()) requestAnimationFrame(() => input?.select());
  });
  const save = (name: string) => {
    setEditing(false);
    const p = playlist();
    if (p && name.trim() && name.trim() !== p.name) renamePlaylist(p.id, name);
  };
  const remove = async () => {
    if (!confirmDelete()) {
      setConfirmDelete(true);
      setTimeout(() => setConfirmDelete(false), 4000);
      return;
    }
    if (await deletePlaylist(props.id)) props.onDeleted();
  };

  return (
    <Show when={playlist()} fallback={<p class="muted empty">{t.playlistGone}</p>}>
      {(p) => (
        <div class="album-page">
          <header class="artist-head album-head">
            <PlaylistArt id={p().id} class="xl" />
            <div class="artist-body">
              <div class="eyebrow">{t.playlist}</div>
              <Show
                when={editing()}
                fallback={
                  <h1 class="album-title pl-title" title={t.rename} onClick={() => setEditing(true)}>
                    {p().name}
                  </h1>
                }
              >
                <input
                  ref={input}
                  class="album-title pl-title-input"
                  value={p().name}
                  maxLength={120}
                  spellcheck={false}
                  aria-label={t.playlistName}
                  onBlur={(e) => save(e.currentTarget.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") e.currentTarget.blur();
                    if (e.key === "Escape") {
                      e.stopPropagation();
                      e.currentTarget.value = p().name;
                      e.currentTarget.blur();
                    }
                  }}
                />
              </Show>
              <p class="muted">
                {countLabel(p().tracks.length)}
                <Show when={p().tracks.length > 0}> · {formatTime(total(p().tracks))}</Show>
              </p>
              <div class="actions hero-play">
                <Show when={p().tracks.length > 0}>
                  <button class="primary" onClick={() => props.onPlay(p().tracks, 0)}>
                    <IconPlay size={18} /> {t.play}
                  </button>
                </Show>
                <button class="icon-btn" title={t.rename} onClick={() => setEditing(true)}>
                  <IconEdit />
                </button>
                <Show when={p().tracks.length > 0}>
                  <button class="icon-btn" title={t.addWholePlaylist} onClick={(e) => openTrackMenu(e, p().tracks, p().id)}>
                    <IconMore />
                  </button>
                </Show>
                <button classList={{ ghost: confirmDelete(), "icon-btn": !confirmDelete(), danger: true }} title={t.deletePlaylist} onClick={remove}>
                  <IconTrash />
                  <Show when={confirmDelete()}>{t.confirmDelete}</Show>
                </button>
              </div>
            </div>
          </header>
          <section class="section">
            <Show
              when={p().tracks.length > 0}
              fallback={
                <div class="empty-state">
                  <IconPlaylist size={26} />
                  <h2>{t.emptyPlaylist}</h2>
                  <p class="muted">{t.emptyPlaylistText}</p>
                </div>
              }
            >
              <TrackList
                tracks={p().tracks}
                current={props.snap.track?.id}
                status={props.snap.status}
                onPlay={(i) => props.onPlay(p().tracks, i)}
                showAlbum
                playlist={p().id}
                onMove={(from, to) => moveInPlaylist(p().id, from, to)}
              />
            </Show>
          </section>
        </div>
      )}
    </Show>
  );
}
