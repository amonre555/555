// Lecteur : fond vivant, barre de lecture flottante, vue « En lecture » avec paroles.
import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import { api, bigCoverUrl, formatTime, type Lyrics, type Snapshot } from "./api";
import { IconChevronDown, IconLyrics, IconNext, IconPause, IconPlay, IconPrev, IconVolume } from "./icons";
import { Cover } from "./ui";

type Run = (a: () => Promise<Snapshot>) => void;

/**
 * Fond de toute l'app : la pochette en cours, très floutée et saturée, qui dérive lentement.
 * Chaque changement de pochette se fait en fondu (l'ancienne reste dessous pendant le fondu).
 */
export function Backdrop(props: { snap: Snapshot }) {
  const url = createMemo(() => bigCoverUrl(props.snap.track));
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
  return (
    <footer class="player glass">
      <div class="now">
        <Show when={props.snap.track} fallback={<div class="muted small">Rien en lecture</div>}>
          {(t) => (
            <button class="now-btn" onClick={props.onExpand} title="Ouvrir la vue En lecture">
              <Cover track={t()} size="md" />
              <div class="now-text">
                <div class="now-title">{t().title}</div>
                <div class="now-sub">{t().artist}</div>
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
        classList={{ "play-btn": true, loading: props.snap.status === "loading" }}
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
export function NowPlaying(props: { snap: Snapshot; position: number; run: Run; onClose: () => void }) {
  const [lyrics, setLyrics] = createSignal<Lyrics | null | undefined>(undefined); // undefined = chargement
  const track = () => props.snap.track;

  createEffect(
    on(
      () => track()?.id,
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
    <div class="nowplaying" role="dialog" aria-label="En lecture">
      <button class="icon-btn np-close" title="Fermer (Échap)" onClick={props.onClose}>
        <IconChevronDown />
      </button>
      <Show when={track()}>
        {(t) => (
          <div class="np-grid">
            <div class="np-left">
              <div class="np-art">
                <img src={bigCoverUrl(t()) ?? ""} alt="" onError={(e) => (e.currentTarget.style.visibility = "hidden")} />
              </div>
              <div class="np-meta">
                <div class="np-title">{t().title}</div>
                <div class="np-sub">
                  {t().artist}
                  <Show when={t().album}>{(a) => <span class="np-album"> · {a()}</span>}</Show>
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

  createEffect(
    on(active, (i) => {
      const el = box?.querySelector<HTMLElement>(`[data-line="${i}"]`);
      if (el) box.scrollTo({ top: el.offsetTop - box.clientHeight * 0.38, behavior: "smooth" });
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
        <div class="lyrics" ref={box}>
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
                onClick={() => props.run(() => api.seek(line.time))}
              >
                {line.text || "♪"}
              </button>
            )}
          </For>
          <div class="lyrics-pad" />
        </div>
      </Show>
    </Show>
  );
}
