// Briques d'interface réutilisables : pochettes, listes, états vides.
import { createEffect, createSignal, For, Match, onCleanup, onMount, Show, Switch, type JSX } from "solid-js";
import { albumRefOf, api, artistsOf, coverUrl, formatTime, type ArtistRef, type Snapshot, type Track, type YtDlpStatus } from "./api";
import { openAlbum, openArtist } from "./nav";
import { IconBack, IconForward, IconGrip, IconHeart, IconMore, IconNote, IconPlay, IconPlaylist, IconPlus } from "./icons";
import { t as tr } from "./i18n";
import { isMobile } from "./mobile";
import {
  addToPlaylist, closeTrackMenu, collection, createPlaylist, isLiked, openTrackMenu, playlistById, playlistCovers,
  removeFromPlaylist, toggleLike, trackMenu, type TrackMenu,
} from "./collection";

export function Loader() {
  return (
    <div class="loader" aria-label={tr.loading}>
      <span /><span /><span />
    </div>
  );
}

export function Empty(props: { text: string }) {
  return <p class="muted empty">{props.text}</p>;
}

export function Cover(props: { track: Track | null | undefined; size?: "sm" | "md" | "lg" | "xl"; hd?: boolean }) {
  // On retient l'adresse qui a échoué, pas un simple « cassé » : sinon, quand le même composant
  // affiche un autre morceau (barre de lecture), la nouvelle pochette restait masquée.
  const [broken, setBroken] = createSignal<string | null>(null);
  const url = () => {
    const u = coverUrl(props.track);
    return u && u !== broken() ? u : null;
  };
  return (
    <div class={`cover ${props.size ?? "sm"}`}>
      <Show when={url()} fallback={<IconNote />}>
        {(u) => <img src={u()} alt="" loading="lazy" decoding="async" onError={() => setBroken(u())} />}
      </Show>
    </div>
  );
}

// Survol d'un morceau en ligne pendant 300 ms : on le prépare pour un démarrage immédiat.
let hoverTimer: number | undefined;
const prefetched = new Set<string>();
export function hoverPrefetch(t: Track) {
  clearTimeout(hoverTimer);
  if (t.source === "local" || prefetched.has(t.id)) return;
  hoverTimer = window.setTimeout(() => {
    prefetched.add(t.id);
    api.prefetch(t);
  }, 300);
}
export function cancelHover() {
  clearTimeout(hoverTimer);
}

/**
 * Les artistes d'un morceau, chacun cliquable vers sa fiche. `artists` : liste déjà complète
 * (crédits Deezer du morceau en cours) ; sinon, celle du morceau.
 */
export function ArtistLinks(props: { track?: Track; artists?: ArtistRef[] }) {
  const list = () => props.artists ?? (props.track ? artistsOf(props.track) : []);
  return (
    <span class="artists">
      <For each={list()}>
        {(a, i) => (
          <>
            <Show when={i() > 0}>{", "}</Show>
            <span
              class="artist-link"
              role="link"
              tabIndex={0}
              title={tr.seeArtist(a.name)}
              onClick={(e) => {
                // Dans une ligne ou une carte : ne pas lancer la lecture en même temps.
                e.stopPropagation();
                openArtist(a);
              }}
              onDblClick={(e) => e.stopPropagation()}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.stopPropagation();
                  openArtist(a);
                }
              }}
            >
              {a.name}
            </span>
          </>
        )}
      </For>
    </span>
  );
}

/** Nom de l'album d'un morceau, cliquable vers sa fiche. */
export function AlbumLink(props: { track: Track }) {
  const ref = () => albumRefOf(props.track);
  return (
    <Show when={ref()}>
      {(a) => (
        <span
          class="artist-link"
          role="link"
          tabIndex={0}
          title={tr.seeAlbum(a().title)}
          onClick={(e) => {
            e.stopPropagation();
            openAlbum(a());
          }}
          onDblClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.stopPropagation();
              openAlbum(a());
            }
          }}
        >
          {a().title}
        </span>
      )}
    </Show>
  );
}

/** Petite étiquette de source (YouTube…). Rien pour la bibliothèque locale. */
export function SourceTag(props: { source: string }) {
  const label = () => ({ youtube: "YouTube" } as Record<string, string>)[props.source];
  return (
    <Show when={label()}>
      <span class={`tag tag-${props.source}`}>{label()}</span>
    </Show>
  );
}

export function TrackList(props: {
  tracks: Track[];
  current?: string;
  status: Snapshot["status"];
  onPlay: (index: number) => void;
  showAlbum?: boolean;
  showSource?: boolean;
  /** Texte de la colonne album, remplacée (ex. heure d'écoute dans l'historique). */
  aside?: (index: number) => string;
  /** Liste affichée dans une playlist : menu « Retirer », et réordonnable si `onMove`. */
  playlist?: string;
  onMove?: (from: number, to: number) => void;
}) {
  // Glisser-déposer (playlists) : ligne déplacée, et ligne au-dessus de laquelle elle arrive.
  const [dragFrom, setDragFrom] = createSignal<number | null>(null);
  const [dragOver, setDragOver] = createSignal<number | null>(null);
  const endDrag = () => {
    setDragFrom(null);
    setDragOver(null);
  };
  /**
   * Téléphone : le glisser-déposer du navigateur ne marche pas au doigt. On réordonne avec la
   * poignée de la ligne : la ligne visée est celle sous le doigt, et la page défile toute seule
   * près des bords.
   */
  const touchDrag = (e: PointerEvent, from: number) => {
    e.preventDefault();
    e.stopPropagation();
    const list = (e.currentTarget as HTMLElement).closest(".tracklist");
    const scroller = (e.currentTarget as HTMLElement).closest(".content");
    setDragFrom(from);
    setDragOver(from);
    let y = e.clientY;
    let raf = 0;
    const edgeScroll = () => {
      if (scroller) {
        const r = scroller.getBoundingClientRect();
        const step = y < r.top + 70 ? -8 : y > r.bottom - 70 ? 8 : 0;
        if (step) scroller.scrollTop += step;
      }
      raf = requestAnimationFrame(edgeScroll);
    };
    raf = requestAnimationFrame(edgeScroll);
    const move = (ev: PointerEvent) => {
      y = ev.clientY;
      const row = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>(".row");
      if (row && list?.contains(row) && row.dataset.index) setDragOver(Number(row.dataset.index));
    };
    const stop = (ev: PointerEvent) => {
      cancelAnimationFrame(raf);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      const to = dragOver();
      if (ev.type === "pointerup" && to !== null && to !== from) props.onMove?.(from, to);
      endDrag();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  };
  return (
    <div classList={{ tracklist: true, "with-album": !!props.showAlbum || !!props.aside }} role="list">
      <For each={props.tracks}>
        {(t, i) => {
          const isCurrent = () => props.current === t.id;
          return (
            <div
              role="listitem"
              classList={{
                row: true,
                current: isCurrent(),
                dragging: dragFrom() === i(),
                "drop-before": dragOver() === i() && dragFrom() !== null && dragFrom()! > i(),
                "drop-after": dragOver() === i() && dragFrom() !== null && dragFrom()! < i(),
              }}
              style={{ "--i": Math.min(i(), 16) }}
              data-index={i()}
              draggable={!!props.onMove && !isMobile()}
              onDragStart={(e) => {
                setDragFrom(i());
                e.dataTransfer!.effectAllowed = "move";
              }}
              onDragOver={(e) => {
                if (dragFrom() === null) return;
                e.preventDefault();
                setDragOver(i());
              }}
              onDrop={(e) => {
                e.preventDefault();
                const from = dragFrom();
                if (from !== null && from !== i()) props.onMove?.(from, i());
                endDrag();
              }}
              onDragEnd={endDrag}
              onContextMenu={(e) => openTrackMenu(e, [t], props.playlist)}
              onDblClick={() => props.onPlay(i())}
              // Téléphone : un simple toucher lance le morceau (pas de double-clic au doigt).
              onClick={() => isMobile() && props.onPlay(i())}
              onKeyDown={(e) => e.key === "Enter" && props.onPlay(i())}
              onMouseEnter={() => hoverPrefetch(t)}
              onMouseLeave={cancelHover}
              tabIndex={0}
            >
              <div class="row-num">
                <span class="num">
                  <Show when={isCurrent() && props.status === "playing"} fallback={String(i() + 1).padStart(2, "0")}>
                    <span class="eq"><i /><i /><i /></span>
                  </Show>
                </span>
                <button
                  class="row-play"
                  title={tr.play}
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onPlay(i());
                  }}
                >
                  <IconPlay size={14} />
                </button>
              </div>
              <Cover track={t} />
              <div class="row-main">
                <div class="row-title">{t.title}</div>
                <div class="row-sub">
                  <Show when={props.showSource}>
                    <SourceTag source={t.source} />
                  </Show>
                  <ArtistLinks track={t} />
                </div>
              </div>
              <Show when={props.aside}>
                {(aside) => <div class="row-album row-aside">{aside()(i())}</div>}
              </Show>
              <Show when={props.showAlbum && !props.aside}>
                <div class="row-album">
                  <AlbumLink track={t} />
                </div>
              </Show>
              <LikeButton track={t} class="row-like" />
              <div class="row-time">{formatTime(t.duration)}</div>
              <button class="row-more" title={tr.moreOptions} onClick={(e) => openTrackMenu(e, [t], props.playlist)} onDblClick={(e) => e.stopPropagation()}>
                <IconMore />
              </button>
              <Show when={props.onMove && isMobile()}>
                <span class="row-handle" title={tr.dragToReorder} onPointerDown={(e) => touchDrag(e, i())} onClick={(e) => e.stopPropagation()}>
                  <IconGrip />
                </span>
              </Show>
            </div>
          );
        }}
      </For>
    </div>
  );
}

/**
 * Rangée de cartes sur une seule ligne, qui défile de côté (glisser sur téléphone, flèches au
 * survol sur ordinateur). Jamais de défilement vertical à l'intérieur : il gênerait celui de la page.
 */
export function Carousel(props: { children: JSX.Element; class?: string }) {
  let el!: HTMLDivElement;
  const [atStart, setAtStart] = createSignal(true);
  const [atEnd, setAtEnd] = createSignal(true);
  const update = () => {
    setAtStart(el.scrollLeft < 4);
    setAtEnd(el.scrollLeft + el.clientWidth >= el.scrollWidth - 4);
  };
  onMount(() => {
    update();
    // Cartes ajoutées ou fenêtre redimensionnée : les flèches suivent.
    const ro = new ResizeObserver(update);
    ro.observe(el);
    const mo = new MutationObserver(update);
    mo.observe(el, { childList: true });
    onCleanup(() => {
      ro.disconnect();
      mo.disconnect();
    });
  });
  const by = (dir: number) => el.scrollBy({ left: dir * el.clientWidth * 0.85, behavior: "smooth" });
  return (
    <div class="carousel-wrap" onMouseEnter={update}>
      <div ref={el} class={`grid carousel ${props.class ?? ""}`} onScroll={update}>
        {props.children}
      </div>
      <button class="carousel-nav prev" classList={{ off: atStart() }} onClick={() => by(-1)} title={tr.scrollPrev} aria-label={tr.scrollPrev} tabIndex={-1}>
        <IconBack />
      </button>
      <button class="carousel-nav next" classList={{ off: atEnd() }} onClick={() => by(1)} title={tr.scrollNext} aria-label={tr.scrollNext} tabIndex={-1}>
        <IconForward />
      </button>
    </div>
  );
}

/** Carte de morceau (grilles « Découvrir »), avec bouton lecture au survol. */
export function TrackCard(props: { track: Track; playing: boolean; onPlay: () => void; index?: number; note?: string }) {
  return (
    <button
      classList={{ card: true, playing: props.playing }}
      style={{ "--i": Math.min(props.index ?? 0, 16) }}
      onClick={props.onPlay}
      onMouseEnter={() => hoverPrefetch(props.track)}
      onMouseLeave={cancelHover}
    >
      <div class="card-art">
        <Cover track={props.track} size="lg" />
        <span class="card-play"><IconPlay size={20} /></span>
      </div>
      <div class="card-title">{props.track.title}</div>
      <div class="card-sub">
        <ArtistLinks track={props.track} />
      </div>
      <Show when={props.note}>
        <div class="card-note">{props.note}</div>
      </Show>
    </button>
  );
}

/** État du moteur YouTube : visible seulement quand il se passe quelque chose. */
export function YtDlpBadge(props: { status: YtDlpStatus }) {
  const s = () => props.status;
  return (
    <Switch>
      <Match when={s().state === "installing" && (s() as { progress: number })}>
        {(st) => (
          <div class="engine" title={tr.ytInstallingHint}>
            <div class="engine-label">{tr.ytInstalling(Math.round(st().progress * 100))}</div>
            <div class="engine-bar"><div style={{ width: `${st().progress * 100}%` }} /></div>
          </div>
        )}
      </Match>
      <Match when={s().state === "error" && (s() as { message: string })}>
        {(st) => (
          <div class="engine error" title={st().message}>
            {tr.ytError(st().message)}
          </div>
        )}
      </Match>
    </Switch>
  );
}

/** Cœur : like / retire le like. Plein et coloré quand le morceau est liké. */
export function LikeButton(props: { track: Track | null | undefined; class?: string; size?: number }) {
  const liked = () => isLiked(props.track);
  return (
    <button
      classList={{ "like-btn": true, liked: liked(), [props.class ?? ""]: !!props.class }}
      title={liked() ? tr.removeFromLiked : tr.addToLiked}
      aria-pressed={liked()}
      disabled={!props.track}
      onClick={(e) => {
        e.stopPropagation();
        if (props.track) toggleLike(props.track);
      }}
      onDblClick={(e) => e.stopPropagation()}
    >
      <IconHeart filled={liked()} size={props.size} />
    </button>
  );
}

/** Image d'une playlist : mosaïque de 4 pochettes, une seule, ou une icône. */
export function PlaylistArt(props: { id?: string; liked?: boolean; class?: string }) {
  const covers = () => {
    if (props.liked) return [];
    const p = playlistById(props.id);
    if (p?.cover) return [p.cover];
    return p ? playlistCovers(p, coverUrl) : [];
  };
  return (
    <div classList={{ "pl-art": true, liked: !!props.liked, mosaic: covers().length >= 4, [props.class ?? ""]: !!props.class }}>
      <Show when={!props.liked} fallback={<IconHeart filled size={22} />}>
        <Show when={covers().length > 0} fallback={<IconPlaylist />}>
          <For each={covers().length >= 4 ? covers() : covers().slice(0, 1)}>{(u) => <img src={u} alt="" loading="lazy" />}</For>
        </Show>
      </Show>
    </div>
  );
}

/**
 * Menu d'un ou plusieurs morceaux (clic droit sur une ligne, bouton « ⋯ ») : like, ajout à une
 * playlist (existante ou nouvelle), retrait de la playlist affichée. Placé dans l'écran.
 */
export function TrackMenuHost(props: { onOpenPlaylist: (id: string) => void }) {
  let el: HTMLDivElement | undefined;
  const [pos, setPos] = createSignal<{ left: number; top: number } | null>(null);
  createEffect(() => {
    const m = trackMenu();
    setPos(null);
    if (!m) return;
    requestAnimationFrame(() => {
      if (!el) return;
      const r = el.getBoundingClientRect();
      const x = m.align === "right" ? m.x - r.width : m.x;
      const left = Math.max(8, Math.min(x, window.innerWidth - r.width - 8));
      const top = m.y + r.height > window.innerHeight - 8 ? Math.max(8, m.y - r.height - 12) : m.y;
      setPos({ left, top });
    });
  });
  const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeTrackMenu();
  window.addEventListener("keydown", onKey);
  window.addEventListener("resize", closeTrackMenu);
  onCleanup(() => {
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("resize", closeTrackMenu);
  });

  /** Ferme le menu puis agit, avec son contenu lu avant la fermeture (après, il n'existe plus). */
  const act = (f: (m: TrackMenu) => unknown) => () => {
    const m = trackMenu();
    closeTrackMenu();
    if (m) f(m);
  };
  return (
    <Show when={trackMenu()}>
      {(m) => {
        const single = () => (m().tracks.length === 1 ? m().tracks[0] : null);
        const targets = () => collection().playlists.filter((p) => p.id !== m().playlist);
        return (
          <div class="menu-layer" onMouseDown={closeTrackMenu} onContextMenu={(e) => (e.preventDefault(), closeTrackMenu())}>
            <div
              ref={el}
              class="menu"
              role="menu"
              style={pos() ? { left: `${pos()!.left}px`, top: `${pos()!.top}px` } : { left: "0px", top: "0px", visibility: "hidden" }}
              onMouseDown={(e) => e.stopPropagation()}
            >
              <Show when={single()}>
                {(t) => (
                  <>
                    <div class="menu-head">
                      <Cover track={t()} />
                      <div class="menu-head-text">
                        <div class="row-title">{t().title}</div>
                        <div class="row-sub">{t().artist}</div>
                      </div>
                    </div>
                    <button class="menu-item" role="menuitem" onClick={act((m) => toggleLike(m.tracks[0]))}>
                      <IconHeart filled={isLiked(t())} /> {isLiked(t()) ? tr.removeFromLiked : tr.addToLiked}
                    </button>
                  </>
                )}
              </Show>
              <Show when={m().playlist && single()}>
                <button class="menu-item" role="menuitem" onClick={act((m) => removeFromPlaylist(m.playlist!, m.tracks[0].id))}>
                  <IconPlaylist size={18} /> {tr.removeFromThisPlaylist}
                </button>
              </Show>
              <div class="menu-label">{tr.addToPlaylist}</div>
              <button
                class="menu-item"
                role="menuitem"
                onClick={act(async (m) => {
                  const id = await createPlaylist(m.tracks.length === 1 ? m.tracks[0].title : "", m.tracks);
                  if (id) props.onOpenPlaylist(id);
                })}
              >
                <IconPlus /> {tr.newPlaylist}
              </button>
              <div class="menu-scroll">
                <For each={targets()}>
                  {(p) => (
                    <button class="menu-item" role="menuitem" onClick={act((m) => addToPlaylist(p.id, m.tracks))}>
                      <PlaylistArt id={p.id} class="xs" />
                      <span class="menu-text">{p.name}</span>
                      <span class="menu-count">{p.tracks.length}</span>
                    </button>
                  )}
                </For>
              </div>
            </div>
          </div>
        );
      }}
    </Show>
  );
}
