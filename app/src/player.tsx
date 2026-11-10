// Lecteur : fond vivant, barre de lecture flottante, vue « En lecture » avec paroles.
import { createEffect, createMemo, createSignal, For, on, onMount, Show } from "solid-js";
import { api, bigCoverUrl, coverUrl, formatTime, type ArtistRef, type Lyrics, type Snapshot, type Track } from "./api";
import { IconChevronDown, IconLyrics, IconNext, IconPause, IconPlay, IconPrev, IconVolume } from "./icons";
import { AlbumLink, ArtistLinks, Cover } from "./ui";
import { themeFromCover } from "./theme";
import { cookie, PLAYER_COOKIE, roundedSquare } from "./shapes";

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
        <Show when={props.snap.track} fallback={<div class="muted small">Rien en lecture</div>}>
          {(t) => (
            <button class="now-btn" onClick={props.onExpand} title="Ouvrir la vue En lecture">
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
      </div>

      <div class="transport">
        <Transport snap={props.snap} run={props.run} playing={playing()} />
        <Progress snap={props.snap} position={props.position} run={props.run} />
      </div>

      <div class="side-controls">
        <button
          classList={{ "icon-btn": true, active: props.expanded }}
          title="Paroles et vue En lecture"
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

function Transport(props: { snap: Snapshot; run: Run; playing: boolean; big?: boolean }) {
  return (
    <div classList={{ buttons: true, big: !!props.big }}>
      <button class="icon-btn" title="Précédent (Alt+←)" onClick={() => props.run(api.previous)} disabled={!props.snap.track}>
        <IconPrev />
      </button>
      <button
        classList={{ "play-btn": true, loading: props.snap.status === "loading", playing: props.playing }}
        title="Lecture / pause (Espace)"
        onClick={() => props.run(api.toggle)}
        disabled={!props.snap.track && props.snap.queueLen === 0}
      >
        <Show when={props.playing} fallback={<IconPlay size={props.big ? 30 : 22} />}>
          <IconPause size={props.big ? 30 : 22} />
        </Show>
      </button>
      <button class="icon-btn" title="Suivant (Alt+→)" onClick={() => props.run(api.next)} disabled={!props.snap.track}>
        <IconNext />
      </button>
    </div>
  );
}

function Progress(props: { snap: Snapshot; position: number; run: Run }) {
  const [dragging, setDragging] = createSignal<number | null>(null);
  const shown = () => dragging() ?? props.position;
  const duration = () => props.snap.duration ?? 0;
  const pct = () => (duration() > 0 ? (shown() / duration()) * 100 : 0);
  return (
    <div class="progress">
      <span class="time">{formatTime(shown())}</span>
      {/*
        Barre ondulée (Material 3 Expressive) : la partie écoutée est une vague qui défile
        pendant la lecture et s'aplatit en pause. Le curseur natif reste par-dessus pour la
        souris et le clavier.
      */}
      <div classList={{ seek: true, flowing: props.snap.status === "playing" && dragging() === null }}>
        <div class="seek-played" style={{ width: `${pct()}%` }} aria-hidden="true">
          <span class="seek-wave" />
          <span class="seek-flat" />
        </div>
      <input
        type="range"
        class="slider"
        min="0"
        max={duration() || 1}
        step="0.1"
        value={shown()}
        style={{ "--pct": `${pct()}%` }}
        disabled={!props.snap.track || !duration()}
        aria-label="Position dans le morceau"
        onInput={(e) => setDragging(parseFloat(e.currentTarget.value))}
        onChange={(e) => {
          const v = parseFloat(e.currentTarget.value);
          props.run(() => api.seek(v));
          setTimeout(() => setDragging(null), 300);
        }}
      />
      </div>
      <span class="time">{formatTime(props.snap.duration)}</span>
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
        aria-label="Volume"
        onInput={(e) => props.run(() => api.setVolume(parseFloat(e.currentTarget.value)))}
      />
    </div>
  );
}

/** Vue plein écran : grande pochette, commandes, et paroles synchronisées. */
/** Position de la pochette du lecteur, hors rotation (la forme cookie tourne en continu). */
function playerCoverRect(): DOMRect | null {
  const cover = document.querySelector<HTMLElement>(".now-btn .cover");
  const parent = cover?.offsetParent;
  if (!cover || !(parent instanceof HTMLElement)) return null;
  const p = parent.getBoundingClientRect();
  return new DOMRect(p.left + cover.offsetLeft, p.top + cover.offsetTop, cover.offsetWidth, cover.offsetHeight);
}

/** Angle actuel de la forme cookie du lecteur (elle tourne en continu pendant la lecture). */
function playerCoverAngle(): number {
  const cover = document.querySelector<HTMLElement>(".now-btn .cover");
  if (!cover) return 0;
  const rotate = getComputedStyle(cover).rotate; // « 123.4deg » ou « none »
  return rotate && rotate !== "none" ? parseFloat(rotate) : 0;
}

/** Vitesse de rotation de la forme cookie (doit suivre l'animation `spin` de styles.css). */
const COOKIE_DEG_PER_MS = 360 / 24000;

/** Rayon des coins de la grande pochette (doit suivre `.np-art` dans styles.css). */
const ART_RADIUS_PX = 48;

/**
 * Fait voler la grande pochette depuis (ou vers) celle du lecteur (technique « FLIP ») :
 * - la position et la taille passent de l'une à l'autre (`transform`) ;
 * - la forme se transforme entre le cookie et un carré aux coins exactement identiques à ceux de
 *   la pochette (`clip-path`, mêmes points des deux côtés), avec l'angle qu'aura le cookie.
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
  const spinning = document.querySelector(".app.playing-on") !== null;
  // Angle du cookie au moment où la grande pochette le rejoint.
  const angle = playerCoverAngle() + (direction === "out" && spinning ? duration * COOKIE_DEG_PER_MS : 0);
  const small = {
    transform: `translate(${from.left - to.left}px, ${from.top - to.top}px) scale(${from.width / to.width})`,
    clipPath: cookie(PLAYER_COOKIE.petals, PLAYER_COOKIE.depth, angle),
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
    <div classList={{ nowplaying: true, leaving: props.leaving }} role="dialog" aria-label="En lecture">
      {/* Fond propre à la vue (pochette floutée sur une base opaque) : il apparaît avec elle. */}
      <div class="np-bg" aria-hidden="true">
        <Show when={bigCoverUrl(track())}>{(u) => <div class="np-bg-art" style={{ "background-image": `url("${u()}")` }} />}</Show>
      </div>
      <button class="icon-btn np-close" title="Fermer (Échap)" onClick={props.onClose}>
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
            </div>
            <div class="np-right">
              <LyricsView lyrics={lyrics()} position={props.position} run={props.run} />
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}

/** Paroles : la ligne en cours est mise en valeur et gardée au centre. Clic = aller à la ligne. */
function LyricsView(props: { lyrics: Lyrics | null | undefined; position: number; run: Run }) {
  let box!: HTMLDivElement;
  const lines = () => props.lyrics?.lines ?? [];

  // Ligne en cours : recherche dichotomique (appelée à chaque image, doit rester légère).
  // Petite avance de 0,25 s : la ligne s'allume au moment où on l'entend, pas après.
  const active = createMemo(() => {
    const l = lines();
    const t = props.position + 0.25;
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
   * Karaoké : part déjà chantée de la ligne en cours (0 à 1), du début de la ligne au début de
   * la suivante. Lue seulement par la ligne en cours : les autres ne sont pas recalculées.
   */
  const sung = () => {
    const l = lines();
    const i = active();
    if (i < 0) return 0;
    const start = l[i].time;
    const end = l[i + 1]?.time ?? start + 5;
    return Math.min(1, Math.max(0, (props.position + 0.25 - start) / Math.max(0.5, end - start)));
  };

  // Défilement à la main (molette, trackpad, glisser) : on arrête de suivre la ligne en cours
  // pendant quelques secondes, au lieu de ramener la vue de force.
  let userScrollUntil = 0;
  const onUserScroll = () => (userScrollUntil = performance.now() + 4000);

  /** Place la ligne `i` aux 2/5 de la hauteur. Instantané à l'ouverture ou au changement de paroles. */
  const follow = (i: number, smooth: boolean) => {
    const el = box?.querySelector<HTMLElement>(`[data-line="${i}"]`);
    if (!el) return;
    const top = el.offsetTop + el.offsetHeight / 2 - box.clientHeight * 0.4;
    box.scrollTo({ top, behavior: smooth ? "smooth" : "auto" });
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
      fallback={<div class="lyrics-empty">Recherche des paroles…</div>}
    >
      <Show
        when={lines().length > 0}
        fallback={
          <Show
            when={props.lyrics?.plain}
            fallback={
              <div class="lyrics-empty">{props.lyrics?.instrumental ? "♪ Instrumental" : "Pas de paroles pour ce morceau"}</div>
            }
          >
            {(p) => <div class="lyrics plain" ref={box}>{p()}</div>}
          </Show>
        }
      >
        <div class="lyrics" ref={box} onWheel={onUserScroll} onTouchMove={onUserScroll}>
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
                onClick={() => {
                  userScrollUntil = 0; // on reprend le suivi depuis la ligne choisie
                  props.run(() => api.seek(line.time));
                }}
              >
                {/* Texte gris que le blanc recouvre de gauche à droite (ligne par ligne si elle se coupe). */}
                <span
                  class="lyric-text"
                  style={{ "--p": `${(i() === active() ? sung() : i() < active() ? 1 : 0) * 100}%` }}
                >
                  {line.text || "♪"}
                </span>
              </button>
            )}
          </For>
          <div class="lyrics-pad" />
        </div>
      </Show>
    </Show>
  );
}
