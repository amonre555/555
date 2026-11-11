// Lecteur : fond vivant, barre de lecture flottante, vue « En lecture » avec paroles.
import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { api, bigCoverUrl, coverUrl, formatTime, type ArtistRef, type LyricWord, type Lyrics, type Snapshot, type Track } from "./api";
import { IconChevronDown, IconLyrics, IconMinus, IconNext, IconPause, IconPlay, IconPlus, IconPrev, IconRepeat, IconShuffle, IconVolume } from "./icons";
import { AlbumLink, ArtistLinks, Cover, LikeButton } from "./ui";
import { themeFromCover } from "./theme";
import { t as tr } from "./i18n";
import { roundedSquare } from "./shapes";

type Run = (a: () => Promise<Snapshot>) => void;

// Crédits complets des morceaux déjà demandés (la recherche Deezer ne donne que l'artiste principal).
const creditsCache = new Map<string, ArtistRef[]>();

/** Tous les artistes du morceau en cours, invités compris (chargés une fois par morceau). */
function useFullArtists(track: () => Track | null | undefined) {
  const id = createMemo(() => track()?.id);
  const [artists, setArtists] = createSignal<ArtistRef[] | undefined>(undefined);
  createEffect(
    on(id, (current) => {
      const t = track();
      setArtists(current ? creditsCache.get(current) : undefined);
      if (!t || !current || creditsCache.has(current)) return;
      api
        .trackArtists(t)
        .then((list) => {
          creditsCache.set(current, list);
          if (id() === current) setArtists(list);
        })
        .catch(() => {});
    }),
  );
  return artists;
}

/**
 * Fond de toute l'app : la pochette en cours, très floutée et saturée, qui dérive lentement.
 * Chaque changement de pochette se fait en fondu (l'ancienne reste dessous pendant le fondu).
 */
export function Backdrop(props: { snap: Snapshot }) {
  const url = createMemo(() => bigCoverUrl(props.snap.track));
  // Couleurs de l'interface : analysées sur la petite pochette (plus rapide, jamais refusée).
  const small = createMemo(() => coverUrl(props.snap.track));
  createEffect(on(small, (u) => themeFromCover(u)));
  const [layers, setLayers] = createSignal<{ url: string; key: number }[]>([]);
  let key = 0;
  createEffect(
    on(url, (u) => {
      if (!u) return setLayers([]);
      setLayers((l) => [...l.slice(-1), { url: u, key: ++key }]);
    }),
  );
  return (
    <div class="backdrop" aria-hidden="true">
      <For each={layers()}>{(l) => <div class="backdrop-layer" style={{ "background-image": `url("${l.url}")` }} />}</For>
      <div class="backdrop-shade" />
    </div>
  );
}

export function PlayerBar(props: {
  snap: Snapshot;
  position: number;
  run: Run;
  onExpand: () => void;
  expanded: boolean;
}) {
  const playing = () => props.snap.status === "playing";
  const artists = useFullArtists(() => props.snap.track);
  return (
    <footer class="player">
      <div class="now">
        <Show when={props.snap.track} fallback={<div class="muted small">{tr.nothingPlaying}</div>}>
          {(t) => (
            <button class="now-btn" onClick={props.onExpand} title={tr.openNowPlaying}>
              <Cover track={t()} size="md" />
              <div class="now-text">
                <div class="now-title">{t().title}</div>
                <div class="now-sub">
                  <ArtistLinks track={t()} artists={artists()} />
                </div>
              </div>
            </button>
          )}
        </Show>
        <LikeButton track={props.snap.track} class="now-like" />
      </div>

      {/* Progress: a thin line along the top edge of the bar (it thickens on hover). */}
      <Progress snap={props.snap} position={props.position} run={props.run} line />

      <div class="transport">
        <Transport snap={props.snap} run={props.run} playing={playing()} />
      </div>

      <div class="side-controls">
        <span class="player-time">
          {formatTime(props.position)}
          <span class="player-time-total"> / {formatTime(props.snap.duration)}</span>
        </span>
        <button
          classList={{ "icon-btn": true, active: props.expanded }}
          title={tr.lyricsAndNowPlaying}
          onClick={props.onExpand}
          disabled={!props.snap.track}
        >
          <IconLyrics />
        </button>
        <Volume snap={props.snap} run={props.run} />
      </div>
    </footer>
  );
}

/** Répétition suivante quand on clique : rien → toute la file → ce morceau → rien. */
const NEXT_REPEAT = { off: "all", all: "one", one: "off" } as const;
const REPEAT_TITLE = {
  off: () => tr.repeatAll,
  all: () => tr.repeatOne,
  one: () => tr.repeatOff,
} as const;

function Transport(props: { snap: Snapshot; run: Run; playing: boolean; big?: boolean }) {
  return (
    <div classList={{ buttons: true, big: !!props.big }}>
      <button
        classList={{ "icon-btn": true, mode: true, active: props.snap.shuffle }}
        title={props.snap.shuffle ? tr.shuffleOff : tr.shuffle}
        aria-pressed={props.snap.shuffle}
        onClick={() => props.run(() => api.setShuffle(!props.snap.shuffle))}
      >
        <IconShuffle />
      </button>
      <button class="icon-btn" title={tr.previous} onClick={() => props.run(api.previous)} disabled={!props.snap.track}>
        <IconPrev />
      </button>
      <button
        classList={{ "play-btn": true, loading: props.snap.status === "loading", playing: props.playing }}
        title={tr.playPause}
        onClick={() => props.run(api.toggle)}
        disabled={!props.snap.track && props.snap.queueLen === 0}
      >
        <Show when={props.playing} fallback={<IconPlay size={props.big ? 30 : 22} />}>
          <IconPause size={props.big ? 30 : 22} />
        </Show>
      </button>
      <button class="icon-btn" title={tr.next} onClick={() => props.run(api.next)} disabled={!props.snap.track}>
        <IconNext />
      </button>
      <button
        classList={{ "icon-btn": true, mode: true, active: props.snap.repeat !== "off" }}
        title={REPEAT_TITLE[props.snap.repeat]()}
        aria-label={REPEAT_TITLE[props.snap.repeat]()}
        onClick={() => props.run(() => api.setRepeat(NEXT_REPEAT[props.snap.repeat]))}
      >
        <IconRepeat one={props.snap.repeat === "one"} />
      </button>
    </div>
  );
}

/** `line`: no times, just the bar (the player's top edge). */
function Progress(props: { snap: Snapshot; position: number; run: Run; line?: boolean }) {
  const [dragging, setDragging] = createSignal<number | null>(null);
  const shown = () => dragging() ?? props.position;
  const duration = () => props.snap.duration ?? 0;
  const pct = () => (duration() > 0 ? (shown() / duration()) * 100 : 0);
  // Max and value set together, max first: on resume the position can arrive before the
  // duration, and the browser would clamp the value to the old max (thumb stuck at the start).
  let input: HTMLInputElement | undefined;
  createEffect(() => {
    const max = duration() || 1;
    const value = Math.min(shown(), max);
    if (!input) return;
    input.max = String(max);
    input.value = String(value);
  });
  return (
    <div classList={{ progress: true, "progress-line": !!props.line }}>
      <Show when={!props.line}>
        <span class="time">{formatTime(shown())}</span>
      </Show>
      {/*
        Barre ondulée (Material 3 Expressive) : la partie écoutée est une vague qui défile
        pendant la lecture et s'aplatit en pause. Le curseur natif reste par-dessus pour la
        souris et le clavier.
      */}
      <div class="seek">
        {/* Fil de lumière : la partie écoutée, qui brille doucement. */}
        <div class="seek-played" style={{ width: `${pct()}%` }} aria-hidden="true" />
      <input
        ref={input}
        type="range"
        class="slider"
        min="0"
        step="0.1"
        style={{ "--pct": `${pct()}%` }}
        disabled={!props.snap.track || !duration()}
        aria-label={tr.seekLabel}
        onInput={(e) => setDragging(parseFloat(e.currentTarget.value))}
        onChange={(e) => {
          const v = parseFloat(e.currentTarget.value);
          props.run(() => api.seek(v));
          setTimeout(() => setDragging(null), 300);
        }}
      />
      </div>
      <Show when={!props.line}>
        <span class="time">{formatTime(props.snap.duration)}</span>
      </Show>
    </div>
  );
}

function Volume(props: { snap: Snapshot; run: Run }) {
  return (
    <div class="volume">
      <IconVolume level={props.snap.volume} />
      <input
        type="range"
        class="slider"
        min="0"
        max="1"
        step="0.01"
        value={props.snap.volume}
        style={{ "--pct": `${props.snap.volume * 100}%` }}
        aria-label={tr.volume}
        onInput={(e) => props.run(() => api.setVolume(parseFloat(e.currentTarget.value)))}
      />
    </div>
  );
}

/** Position de la pochette du lecteur. */
function playerCoverRect(): DOMRect | null {
  return document.querySelector<HTMLElement>(".now-btn .cover")?.getBoundingClientRect() ?? null;
}

/** Rayons des coins des deux pochettes (doivent suivre `.np-art` et `.cover.md` dans styles.css). */
const ART_RADIUS_PX = 22;
const PLAYER_RADIUS_PX = 10;

/**
 * Fait voler la grande pochette depuis (ou vers) celle du lecteur (technique « FLIP ») :
 * - la position et la taille passent de l'une à l'autre (`transform`) ;
 * - les coins s'arrondissent exactement comme ceux de l'autre pochette (`clip-path`, mêmes
 *   points des deux côtés) : à l'arrivée, le relais avec le vrai `border-radius` ne se voit pas.
 * Si une animation est déjà en cours (Échap pendant l'ouverture, réouverture pendant la
 * fermeture), on repart de l'état affiché à cet instant : pas de saut.
 */
function flyCover(art: HTMLElement, direction: "in" | "out", duration: number) {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  // État affiché maintenant (si la pochette est en plein vol), avant d'arrêter l'animation.
  const running = art.getAnimations();
  const style = getComputedStyle(art);
  const current = running.length ? { transform: style.transform, clipPath: style.clipPath } : null;
  running.forEach((a) => a.cancel());

  const from = playerCoverRect();
  const to = art.getBoundingClientRect(); // sans animation : la vraie place de la pochette
  if (!from || !to.width) return;
  const small = {
    transform: `translate(${from.left - to.left}px, ${from.top - to.top}px) scale(${from.width / to.width})`,
    clipPath: roundedSquare((PLAYER_RADIUS_PX / from.width) * 100),
  };
  const big = { transform: "translate(0px, 0px) scale(1)", clipPath: roundedSquare((ART_RADIUS_PX / to.width) * 100) };
  art.style.transformOrigin = "0 0";
  art.style.borderRadius = "0";
  const start = current && current.clipPath !== "none" ? current : direction === "in" ? small : big;
  const anim = art.animate([start, direction === "in" ? big : small], {
    duration,
    easing: direction === "in" ? "cubic-bezier(0.22, 1, 0.36, 1)" : "cubic-bezier(0.55, 0, 0.3, 1)",
    fill: "both",
  });
  // Arrivée dans la vue : on rend sa forme normale (coins arrondis et ombre, que le découpage
  // masquait) ; la forme d'arrivée est identique, le relais ne se voit pas. La fermeture garde la
  // dernière image jusqu'au retrait de la vue.
  if (direction === "in") {
    anim.finished
      .then(() => {
        anim.cancel();
        art.style.borderRadius = "";
        art.style.transformOrigin = "";
      })
      .catch(() => {});
  }
}

export function NowPlaying(props: { snap: Snapshot; position: number; run: Run; leaving: boolean; onClose: () => void }) {
  let art: HTMLDivElement | undefined;
  // Dès l'insertion (avant le premier affichage), pour ne jamais voir la pochette à sa place finale.
  onMount(() => art && flyCover(art, "in", 650));
  // Fermeture, ou réouverture pendant une fermeture : la pochette repart d'où elle est.
  // Mémo obligatoire : `on` relance l'effet à chaque changement d'état de l'app (par exemple
  // « en ouverture » → « ouverte ») même si `leaving` ne change pas. Sans lui, la fin de
  // l'ouverture relançait un envol depuis le lecteur (la pochette sautait une dernière fois).
  const leaving = createMemo(() => props.leaving);
  createEffect(on(leaving, (l) => art && flyCover(art, l ? "out" : "in", l ? 440 : 650), { defer: true }));
  const [lyrics, setLyrics] = createSignal<Lyrics | null | undefined>(undefined); // undefined = chargement
  /** Téléphone : les paroles prennent la place de la pochette (bouton « Paroles »). */
  const [lyricsMode, setLyricsMode] = createSignal(false);

  /**
   * Téléphone : glisser vers le bas ferme la vue (la vue suit le doigt). Pas depuis les paroles
   * (elles défilent) ni depuis un curseur.
   */
  let root: HTMLDivElement | undefined;
  let swipe: { y: number; dy: number } | null = null;
  const onTouchStart = (e: TouchEvent) => {
    if ((e.target as HTMLElement).closest(".lyrics, .slider, .np-actions")) return;
    swipe = { y: e.touches[0].clientY, dy: 0 };
  };
  const onTouchMove = (e: TouchEvent) => {
    if (!swipe || !root) return;
    swipe.dy = Math.max(0, e.touches[0].clientY - swipe.y);
    root.style.transition = "none";
    root.style.transform = `translateY(${swipe.dy}px)`;
  };
  const onTouchEnd = () => {
    if (!swipe || !root) return;
    const close = swipe.dy > 110;
    swipe = null;
    root.style.transition = "transform 0.35s cubic-bezier(0.22, 1, 0.36, 1)";
    if (close) props.onClose();
    else root.style.transform = "";
  };
  const track = () => props.snap.track;
  // Mémo obligatoire : l'état arrive 4 fois par seconde avec un nouvel objet `track`, et `on`
  // relance l'effet à chaque notification même si l'identifiant n'a pas changé. Sans ce mémo,
  // les paroles étaient effacées puis rechargées sans arrêt (défilement remis à zéro).
  const trackId = createMemo(() => track()?.id);
  const artists = useFullArtists(track);

  // Grande pochette : version HD, sinon la normale, sinon rien. Les échecs sont oubliés à
  // chaque changement de morceau (avant, une seule erreur masquait toutes les pochettes suivantes).
  const [failed, setFailed] = createSignal<string[]>([]);
  createEffect(on(trackId, () => setFailed([])));
  const artUrl = () => [bigCoverUrl(track()), coverUrl(track())].find((u) => u && !failed().includes(u)) ?? null;

  createEffect(
    on(
      trackId,
      () => {
        const t = track();
        setLyrics(undefined);
        if (!t) return;
        const id = t.id;
        api
          .lyrics(t)
          .then((l) => track()?.id === id && setLyrics(l))
          .catch(() => track()?.id === id && setLyrics(null));
      },
    ),
  );

  return (
    <div
      ref={root}
      classList={{ nowplaying: true, leaving: props.leaving, "lyrics-mode": lyricsMode() }}
      role="dialog"
      aria-label={tr.nowPlaying}
      onTouchStart={onTouchStart}
      onTouchMove={onTouchMove}
      onTouchEnd={onTouchEnd}
      onTouchCancel={onTouchEnd}
    >
      {/* Fond propre à la vue (pochette floutée sur une base opaque) : il apparaît avec elle. */}
      <div class="np-bg" aria-hidden="true">
        <Show when={bigCoverUrl(track())}>{(u) => <div class="np-bg-art" style={{ "background-image": `url("${u()}")` }} />}</Show>
      </div>
      <button class="icon-btn np-close" title={tr.close} onClick={props.onClose}>
        <IconChevronDown />
      </button>
      <Show when={track()}>
        {(t) => (
          <div class="np-grid">
            <div class="np-left">
              <div class="np-art" ref={art}>
                <Show when={artUrl()}>{(u) => <img src={u()} alt="" onError={() => setFailed((f) => [...f, u()])} />}</Show>
              </div>
              <div class="np-meta">
                <LikeButton track={t()} class="np-like" size={26} />
                <div class="np-title" title={t().title}>{t().title}</div>
                <div class="np-sub" title={[t().artist, t().album].filter(Boolean).join(" · ")}>
                  <ArtistLinks track={t()} artists={artists()} />
                  <Show when={t().album !== t().title && t().album}>
                    <span class="np-album">
                      {" · "}
                      <AlbumLink track={t()} />
                    </span>
                  </Show>
                </div>
              </div>
              <Progress snap={props.snap} position={props.position} run={props.run} />
              <Transport snap={props.snap} run={props.run} playing={props.snap.status === "playing"} big />
              {/* Téléphone seulement (caché sur ordinateur) : paroles et volume. */}
              <div class="np-actions">
                <button
                  classList={{ "np-toggle": true, on: lyricsMode() }}
                  aria-pressed={lyricsMode()}
                  onClick={() => setLyricsMode((v) => !v)}
                >
                  <IconLyrics /> {lyricsMode() ? tr.cover : tr.lyrics}
                </button>
                <Volume snap={props.snap} run={props.run} />
              </div>
            </div>
            <div class="np-right">
              <LyricsView lyrics={lyrics()} trackId={trackId()} position={props.position} run={props.run} />
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}

/** Paroles : la ligne en cours est mise en valeur et gardée au centre. Clic = aller à la ligne. */
/**
 * Décalage des paroles choisi pour un morceau (en secondes, positif = paroles plus tôt).
 * Gardé dans le navigateur de l'app : simple confort, sans conséquence s'il se perd.
 */
const offsetKey = (id: string) => `yato.lyrics-offset.${id}`;
function readOffset(id: string): number {
  try {
    return Number(localStorage.getItem(offsetKey(id))) || 0;
  } catch {
    return 0;
  }
}
function saveOffset(id: string, value: number) {
  try {
    if (value) localStorage.setItem(offsetKey(id), String(value));
    else localStorage.removeItem(offsetKey(id));
  } catch {
    // Stockage indisponible : le réglage vaut pour cette écoute seulement.
  }
}

function LyricsView(props: { lyrics: Lyrics | null | undefined; trackId: string | undefined; position: number; run: Run }) {
  let box!: HTMLDivElement;
  const lines = () => props.lyrics?.lines ?? [];

  // Les bases de paroles se contredisent parfois d'une seconde : réglage à la main, par morceau.
  const [shift, setShift] = createSignal(0);
  createEffect(on(() => props.trackId, (id) => setShift(id ? readOffset(id) : 0)));
  const nudge = (delta: number) => {
    const v = delta === 0 ? 0 : Math.round((shift() + delta) * 10) / 10;
    setShift(v);
    if (props.trackId) saveOffset(props.trackId, v);
  };
  /** Position dans le morceau, vue des paroles (avec le décalage choisi). */
  const pos = () => props.position + shift();

  // Ligne en cours : recherche dichotomique (appelée à chaque image, doit rester légère).
  // Petite avance de 0,25 s : la ligne s'allume au moment où on l'entend, pas après.
  const active = createMemo(() => {
    const l = lines();
    const t = pos() + 0.25;
    let lo = 0;
    let hi = l.length - 1;
    let found = -1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (l[mid].time <= t) {
        found = mid;
        lo = mid + 1;
      } else hi = mid - 1;
    }
    return found;
  });

  /**
   * Karaoké mot par mot : part déjà chantée d'un mot (0 à 1). Lignes passées : pleines ; lignes
   * à venir : vides. Seule la ligne en cours lit la position : les autres ne sont pas recalculées
   * à chaque image. Légère avance (0,1 s) : le mot s'allume quand on l'entend, pas après.
   */
  const fill = (line: number, w: LyricWord) => {
    const a = active();
    if (line < a) return 1;
    if (line > a) return 0;
    return Math.min(1, Math.max(0, (pos() + 0.1 - w.time) / Math.max(0.05, w.end - w.time)));
  };

  // Défilement à la main (molette, trackpad, glisser) : on arrête de suivre la ligne en cours
  // pendant quelques secondes, au lieu de ramener la vue de force. Pendant ce temps, les lignes
  // ne sont plus floutées (on cherche une ligne : tout doit être lisible).
  let userScrollUntil = 0;
  const [browsing, setBrowsing] = createSignal(false);
  let browseTimer: number | undefined;
  const onUserScroll = () => {
    userScrollUntil = performance.now() + 4000;
    setBrowsing(true);
    clearTimeout(browseTimer);
    browseTimer = window.setTimeout(() => setBrowsing(false), 4000);
  };
  onCleanup(() => clearTimeout(browseTimer));

  /**
   * Place la ligne `i` aux 2/5 de la hauteur.
   *
   * En douceur, façon cascade : la vue saute à sa place, puis chaque ligne visible part de son
   * ancienne position et glisse jusqu'à la nouvelle, avec un petit retard qui croît avec la
   * distance à la ligne en cours (les lignes suivantes arrivent les unes après les autres).
   * Les animations s'additionnent (`composite: "add"`) : un changement de ligne en plein
   * mouvement repart de la position affichée, sans à-coup. On anime `translate`, distinct de la
   * mise à l'échelle des lignes (`transform`), pour ne pas fausser le déplacement.
   */
  const follow = (i: number, smooth: boolean) => {
    const el = box?.querySelector<HTMLElement>(`[data-line="${i}"]`);
    if (!el) return;
    const target = el.offsetTop + el.offsetHeight / 2 - box.clientHeight * 0.4;
    const before = box.scrollTop;
    box.scrollTop = target;
    const delta = box.scrollTop - before;
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (!smooth || reduced || Math.abs(delta) < 1 || Math.abs(delta) > box.clientHeight) return;
    const top = box.scrollTop - Math.abs(delta) - 80;
    const bottom = box.scrollTop + box.clientHeight + Math.abs(delta) + 80;
    for (const line of box.querySelectorAll<HTMLElement>(".lyric")) {
      if (line.offsetTop + line.offsetHeight < top || line.offsetTop > bottom) continue;
      const d = Number(line.dataset.line) - i;
      line.animate([{ translate: `0 ${delta}px` }, { translate: "0 0" }], {
        duration: 820,
        delay: Math.min(Math.max(d + 1, 0), 7) * 38,
        easing: "cubic-bezier(0.2, 0.85, 0.25, 1)",
        composite: "add",
        fill: "backwards",
      });
    }
  };

  /**
   * Zone des paroles qui change de taille (téléphone : elle était cachée derrière la pochette et
   * apparaît, rotation…) : la ligne en cours est replacée aussitôt, sans animation.
   */
  const resized = new ResizeObserver(() => {
    if (box?.clientHeight && performance.now() >= userScrollUntil) follow(active(), false);
  });
  onCleanup(() => resized.disconnect());
  const attach = (el: HTMLDivElement) => {
    box = el;
    resized.disconnect();
    resized.observe(el);
  };

  createEffect(
    on([active, lines], ([i, l], prev) => {
      const newLyrics = !prev || prev[1] !== l;
      if (newLyrics) userScrollUntil = 0;
      else if (performance.now() < userScrollUntil) return;
      // Le DOM des nouvelles lignes doit être en place avant de mesurer.
      if (newLyrics) requestAnimationFrame(() => follow(i, false));
      else follow(i, true);
    }),
  );

  return (
    <Show
      when={props.lyrics !== undefined}
      fallback={<div class="lyrics-empty">{tr.searchingLyrics}</div>}
    >
      <Show
        when={lines().length > 0}
        fallback={
          <Show
            when={props.lyrics?.plain}
            fallback={
              <div class="lyrics-empty">{props.lyrics?.instrumental ? tr.instrumental : tr.noLyrics}</div>
            }
          >
            {(p) => <div class="lyrics plain" ref={box}>{p()}</div>}
          </Show>
        }
      >
        <div classList={{ lyrics: true, browsing: browsing() }} ref={attach} onWheel={onUserScroll} onTouchMove={onUserScroll}>
          <div class="lyrics-pad" />
          <For each={lines()}>
            {(line, i) => (
              <button
                data-line={i()}
                classList={{
                  "lyric": true,
                  "on": i() === active(),
                  "past": i() < active(),
                  "near": Math.abs(i() - active()) === 1,
                }}
                // Distance à la ligne en cours (pour le flou), plafonnée : au-delà, hors de l'écran.
                style={{ "--d": Math.min(Math.abs(i() - active()), 4) }}
                onClick={() => {
                  userScrollUntil = 0; // on reprend le suivi depuis la ligne choisie
                  props.run(() => api.seek(Math.max(0, line.time - shift())));
                }}
              >
                {/* Chaque mot : texte gris que le blanc recouvre de gauche à droite, à son rythme. */}
                <Show when={line.words.length > 0} fallback={<span class="lyric-text" style={{ "--p": i() < active() ? "100%" : "0%" }}>♪</span>}>
                  <For each={line.words}>
                    {(w) => {
                      const f = () => fill(i(), w);
                      // L'espace reste hors du mot : les mots (blocs en ligne) se coupent entre eux.
                      const space = /\s$/.test(w.text);
                      return (
                        <>
                          <span classList={{ "lyric-word": true, singing: f() > 0 && f() < 1 }} style={{ "--p": `${f() * 120 - 10}%` }}>
                            {w.text.trimEnd()}
                          </span>
                          {space ? " " : ""}
                        </>
                      );
                    }}
                  </For>
                </Show>
              </button>
            )}
          </For>
          <div class="lyrics-pad" />
        </div>
        <div classList={{ "lyrics-sync": true, set: shift() !== 0 }}>
          <button class="icon-btn" title={tr.lyricsEarly} onClick={() => nudge(-0.1)}>
            <IconMinus />
          </button>
          <button class="lyrics-sync-value" title={shift() ? tr.lyricsReset : tr.lyricsSync} onClick={() => nudge(0)}>
            {shift() ? `${shift() > 0 ? "+" : "−"}${Math.abs(shift()).toFixed(1).replace(".", tr.decimal)} s` : tr.sync}
          </button>
          <button class="icon-btn" title={tr.lyricsLate} onClick={() => nudge(0.1)}>
            <IconPlus />
          </button>
        </div>
      </Show>
    </Show>
  );
}
