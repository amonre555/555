// Récap de l'année : une suite d'écrans plein cadre qui défilent seuls (ou au clic, aux flèches).
// Chaque écran arrive comme une onde : il s'ouvre en cercle depuis l'endroit du clic, pendant que
// le précédent recule dans le flou. Les éléments de chaque écran entrent ensuite l'un après
// l'autre (classe `rv`, délai `--d`).
import { createEffect, createMemo, createResource, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { api, bigCoverUrl, type Stats, type Track } from "./api";
import { IconClose, IconPause, IconPlay, IconWaves } from "./icons";
import { locale, t } from "./i18n";
import { ArtistPhoto, artistPicture, Bars, dayName, minutesOf, monthName, number } from "./stats";
import { Cover, Loader } from "./ui";
import "./recap.css";

/** Temps passé sur chaque écran, en millisecondes. */
const SLIDE_MS = 6800;

const reduced = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

type Theme = "blue" | "night" | "coral" | "violet" | "lime" | "cyan";
type Slide = { id: string; theme: Theme; art?: Track; render: () => JSX.Element };

type Props = { year: number; onClose: () => void; onPlay: (tracks: Track[], index: number) => void };

export function Recap(props: Props) {
  const [stats] = createResource(() => props.year, (year) => api.stats({ kind: "year", year }));
  const slides = createMemo<Slide[]>(() => {
    const s = stats();
    return s ? buildSlides(s, props) : [];
  });

  // Photos des artistes du top : demandées dès l'ouverture, pour qu'elles soient là à temps.
  createEffect(() => stats()?.topArtists.slice(0, 5).forEach((a) => artistPicture(a)));

  const [index, setIndex] = createSignal(0);
  /** Écran qui s'en va, le temps de sa sortie. */
  const [leaving, setLeaving] = createSignal<{ i: number; dir: 1 | -1; key: number } | null>(null);
  const [origin, setOrigin] = createSignal({ x: 50, y: 50, dir: 1 as 1 | -1, key: 0 });
  const [paused, setPaused] = createSignal(false);
  const [elapsed, setElapsed] = createSignal(0);
  let leaveTimer: number | undefined;

  const last = () => slides().length - 1;
  const go = (to: number, at?: { x: number; y: number }) => {
    const from = index();
    if (to < 0 || to > last() || to === from) return;
    const dir = to > from ? 1 : -1;
    clearTimeout(leaveTimer);
    const key = origin().key + 1;
    setLeaving({ i: from, dir, key });
    setOrigin({ x: at?.x ?? 50, y: at?.y ?? 50, dir, key });
    setIndex(to);
    setElapsed(0);
    leaveTimer = window.setTimeout(() => setLeaving(null), 1000);
  };
  const next = (at?: { x: number; y: number }) => go(index() + 1, at);
  const prev = (at?: { x: number; y: number }) => go(index() - 1, at);
  const restart = () => go(0);

  // Défilement automatique (sauf sur le dernier écran, qui attend).
  let raf = 0;
  let tickAt = performance.now();
  const tick = (now: number) => {
    const dt = now - tickAt;
    tickAt = now;
    if (!paused() && !stats.loading && index() < last()) {
      const e = elapsed() + dt;
      if (e >= SLIDE_MS) next();
      else setElapsed(e);
    }
    raf = requestAnimationFrame(tick);
  };

  // Clavier : pris avant les raccourcis de l'app (Espace ne doit pas mettre la musique en pause).
  const onKey = (e: KeyboardEvent) => {
    const keys: Record<string, () => void> = {
      Escape: props.onClose,
      ArrowRight: () => next(),
      ArrowLeft: () => prev(),
      " ": () => setPaused((p) => !p),
    };
    const action = keys[e.key];
    if (!action) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    action();
  };

  onMount(() => {
    raf = requestAnimationFrame(tick);
    window.addEventListener("keydown", onKey, true);
  });
  onCleanup(() => {
    cancelAnimationFrame(raf);
    clearTimeout(leaveTimer);
    window.removeEventListener("keydown", onKey, true);
  });

  // Clic : à gauche, écran précédent ; ailleurs, suivant. Appui long : pause, le temps de lire.
  let downAt = 0;
  const onDown = (e: PointerEvent) => {
    if ((e.target as HTMLElement).closest("button, a, select")) return;
    downAt = performance.now();
    setPaused(true);
  };
  const onUp = (e: PointerEvent) => {
    if (!downAt) return;
    const held = performance.now() - downAt > 320;
    downAt = 0;
    setPaused(false);
    if (held) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const at = { x: ((e.clientX - r.left) / r.width) * 100, y: ((e.clientY - r.top) / r.height) * 100 };
    if (at.x < 30) prev(at);
    else next(at);
  };

  return (
    <div class="recap" role="dialog" aria-modal="true" aria-label={t.recapCardTitle(props.year)}>
      <Show when={!stats.loading} fallback={<div class="recap-loading"><Loader /></div>}>
        <Show
          when={(stats()?.plays ?? 0) > 0}
          fallback={
            <div class="recap-empty">
              <IconWaves size={34} />
              <p>{t.recapNoData(props.year)}</p>
              <button class="primary" onClick={props.onClose}>{t.closeLabel}</button>
            </div>
          }
        >
          <div class="recap-stage" onPointerDown={onDown} onPointerUp={onUp} onPointerCancel={() => ((downAt = 0), setPaused(false))}>
            <Show when={leaving()} keyed>
              {(l) => <SlideView slide={slides()[l.i]} leaving={l.dir} />}
            </Show>
            <Show when={origin()} keyed>
              {(o) => <SlideView slide={slides()[index()]} entering={o} />}
            </Show>
          </div>

          <div class="recap-chrome">
            <div class="recap-progress" aria-hidden="true">
              <For each={slides()}>
                {(_, i) => (
                  <span>
                    <i style={{ transform: `scaleX(${i() < index() ? 1 : i() > index() ? 0 : index() === last() ? 1 : elapsed() / SLIDE_MS})` }} />
                  </span>
                )}
              </For>
            </div>
            <div class="recap-bar">
              <img src="/wordmark.svg" alt="555" height="16" />
              <span class="recap-year">{props.year}</span>
              <span class="spacer" />
              <Show when={index() < last()}>
                <button class="recap-btn" title={paused() ? t.recapResume : t.recapPause} onClick={() => setPaused((p) => !p)}>
                  {paused() ? <IconPlay size={16} /> : <IconPause size={16} />}
                </button>
              </Show>
              <button class="recap-btn" title={t.close} onClick={props.onClose}>
                <IconClose />
              </button>
            </div>
          </div>
          <Show when={index() === last()}>
            <div class="recap-outro-actions">
              <button class="primary" onClick={() => props.onPlay(stats()!.topTracks.map((x) => x.track), 0)}>
                <IconPlay size={16} /> {t.recapPlayTop}
              </button>
              <button class="ghost" onClick={restart}>{t.recapReplay}</button>
              <button class="ghost" onClick={props.onClose}>{t.closeLabel}</button>
            </div>
          </Show>
        </Show>
      </Show>
    </div>
  );
}

/** Un écran. Il entre en s'ouvrant en cercle depuis `entering` (ou se referme, en arrière). */
function SlideView(props: { slide: Slide | undefined; entering?: { x: number; y: number; dir: 1 | -1 }; leaving?: 1 | -1 }) {
  let el: HTMLElement | undefined;
  // Au montage, pas dans `ref` : Solid crée l'élément dans le document inerte de son gabarit, où
  // une animation lancée resterait figée sur sa première image.
  onMount(() => {
    if (!el || reduced()) return;
    const ease = "cubic-bezier(0.65, 0, 0.2, 1)";
    if (props.entering) {
      const { x, y, dir } = props.entering;
      if (dir > 0) {
        el.animate(
          [{ clipPath: `circle(0% at ${x}% ${y}%)` }, { clipPath: `circle(150% at ${x}% ${y}%)` }],
          { duration: 1000, easing: ease, fill: "both" },
        );
      } else {
        // En arrière : l'écran précédent attend dessous et revient au premier plan.
        el.animate(
          [{ transform: "scale(1.08)", filter: "blur(16px) brightness(0.5)" }, { transform: "none", filter: "none" }],
          { duration: 1000, easing: ease, fill: "both" },
        );
      }
    } else if (props.leaving) {
      el.style.zIndex = props.leaving > 0 ? "0" : "2";
      el.animate(
        props.leaving > 0
          ? [{ transform: "none", filter: "none" }, { transform: "scale(0.9)", filter: "blur(18px) brightness(0.45)" }]
          : [{ clipPath: "circle(150% at 50% 50%)" }, { clipPath: "circle(0% at 50% 50%)" }],
        { duration: 1000, easing: ease, fill: "both" },
      );
    }
  });
  return (
    <Show when={props.slide}>
      {(s) => (
        <section ref={el} classList={{ "recap-slide": true, [`theme-${s().theme}`]: true, leaving: !!props.leaving }} data-slide={s().id}>
          <Show when={s().art}>
            {(art) => <div class="recap-art-bg" style={{ "background-image": `url("${bigCoverUrl(art()) ?? ""}")` }} />}
          </Show>
          <div class="recap-glow" />
          <div class="recap-content">{s().render()}</div>
        </section>
      )}
    </Show>
  );
}

// ---------- Décor ----------

/** Ondes qui passent lentement en fond. */
function Waves(props: { class?: string }) {
  const wave = (amp: number, len: number) => {
    let d = `M 0 50`;
    for (let x = 0; x < 2400; x += len) d += ` q ${len / 4} ${-amp} ${len / 2} 0 t ${len / 2} 0`;
    return d;
  };
  return (
    <svg class={`recap-waves ${props.class ?? ""}`} viewBox="0 0 2400 100" preserveAspectRatio="none" aria-hidden="true">
      <path d={wave(26, 300)} />
      <path d={wave(16, 200)} />
      <path d={wave(36, 400)} />
    </svg>
  );
}

/** Cercles qui s'éloignent d'un point, comme une onde dans l'eau. */
function Ripples() {
  return (
    <div class="recap-ripples" aria-hidden="true">
      <i /><i /><i /><i />
    </div>
  );
}

/** Nombre qui défile jusqu'à sa valeur. */
function CountUp(props: { value: number; delay?: number }) {
  const [shown, setShown] = createSignal(reduced() ? props.value : 0);
  let raf = 0;
  onMount(() => {
    if (reduced()) return;
    const start = performance.now() + (props.delay ?? 0);
    const step = (now: number) => {
      const p = Math.min(1, Math.max(0, (now - start) / 1800));
      setShown(Math.round(props.value * (1 - Math.pow(2, -10 * p))));
      if (p < 1) raf = requestAnimationFrame(step);
      else setShown(props.value);
    };
    raf = requestAnimationFrame(step);
  });
  onCleanup(() => cancelAnimationFrame(raf));
  return <>{number(shown())}</>;
}

/** Horloge de 24 h : une barre par heure, l'heure préférée en lumière. */
function Clock(props: { hours: number[]; favorite: number }) {
  const max = () => Math.max(1, ...props.hours);
  return (
    <svg class="recap-clock" viewBox="-110 -110 220 220" aria-hidden="true">
      <circle r="36" class="clock-face" />
      <For each={props.hours}>
        {(v, h) => {
          const a = ((h() / 24) * 360 - 90) * (Math.PI / 180);
          const len = 8 + (v / max()) * 58;
          const [x1, y1] = [Math.cos(a) * 42, Math.sin(a) * 42];
          const [x2, y2] = [Math.cos(a) * (42 + len), Math.sin(a) * (42 + len)];
          return (
            <line
              x1={x1} y1={y1} x2={x2} y2={y2} pathLength="1"
              classList={{ "clock-bar": true, fav: h() === props.favorite }}
              style={{ "--i": h() }}
            />
          );
        }}
      </For>
      <text class="clock-label" y="7">{t.hourLabel(props.favorite)}</text>
    </svg>
  );
}

// ---------- Les écrans ----------

function buildSlides(s: Stats, props: Props): Slide[] {
  const y = props.year;
  const minutes = minutesOf(s.seconds);
  const out: Slide[] = [];
  const top = s.topTracks[0];
  const artist = s.topArtists[0];
  const album = s.topAlbums[0];

  out.push({
    id: "intro",
    theme: "blue",
    render: () => (
      <div class="slide-intro">
        <Ripples />
        <Waves />
        <div class="eyebrow rv" style={{ "--d": 1 }}>555</div>
        <div class="recap-huge rv" style={{ "--d": 2 }}>{y}</div>
        <h1 class="recap-title rv" style={{ "--d": 4 }}>{t.recapIntro}</h1>
        <p class="recap-sub rv" style={{ "--d": 6 }}>{t.recapIntroSub}</p>
      </div>
    ),
  });

  if (s.seconds > 0) {
    const days = (s.seconds / 86_400).toLocaleString(locale(), { maximumFractionDigits: 1 });
    out.push({
      id: "minutes",
      theme: "night",
      render: () => (
        <div class="slide-center">
          <Waves class="low" />
          <p class="recap-lead rv" style={{ "--d": 1 }}>{t.recapListened}</p>
          <div class="recap-number rv" style={{ "--d": 2 }}>
            <CountUp value={minutes} delay={300} />
          </div>
          <p class="recap-lead rv" style={{ "--d": 3 }}>{t.recapMinutes}</p>
          <p class="recap-sub rv" style={{ "--d": 7 }}>{t.recapMinutesSub(Math.round(s.seconds / 3600), days)}</p>
        </div>
      ),
    });
  }

  if (s.first) {
    const f = s.first;
    const date = new Intl.DateTimeFormat(locale(), { day: "numeric", month: "long" }).format(new Date(f.at));
    out.push({
      id: "first",
      theme: "violet",
      art: f.track,
      render: () => (
        <div class="slide-split">
          <div class="recap-cover rv" style={{ "--d": 2 }}>
            <Cover track={f.track} size="xl" />
          </div>
          <div>
            <p class="recap-lead rv" style={{ "--d": 1 }}>{t.recapFirst(date)}</p>
            <h2 class="recap-name rv" style={{ "--d": 3 }}>{f.track.title}</h2>
            <p class="recap-sub rv" style={{ "--d": 4 }}>{f.track.artist}</p>
          </div>
        </div>
      ),
    });
  }

  if (artist) {
    out.push({
      id: "artist",
      theme: "coral",
      art: artist.track,
      render: () => (
        <div class="slide-center">
          <p class="eyebrow rv" style={{ "--d": 1 }}>{t.recapArtist}</p>
          <div class="recap-artist rv" style={{ "--d": 2 }}>
            <Ripples />
            <ArtistPhoto artist={artist} class="xl" />
          </div>
          <h2 class="recap-name big rv" style={{ "--d": 4 }}>{artist.name}</h2>
          <p class="recap-sub rv" style={{ "--d": 6 }}>{t.recapArtistSub(minutesOf(artist.seconds), artist.plays)}</p>
        </div>
      ),
    });
  }

  if (s.topArtists.length >= 3) {
    out.push({
      id: "artists",
      theme: "night",
      render: () => (
        <div class="slide-list">
          <h2 class="recap-title rv" style={{ "--d": 1 }}>{t.recapArtists}</h2>
          <ol class="recap-rank">
            <For each={s.topArtists.slice(0, 5)}>
              {(a, i) => (
                <li class="rv" style={{ "--d": 2 + i() * 1.2 }}>
                  <span class="recap-rank-n">{i() + 1}</span>
                  <ArtistPhoto artist={a} />
                  <span class="recap-rank-name">{a.name}</span>
                  <span class="recap-rank-value">{t.minutesCount(minutesOf(a.seconds))}</span>
                </li>
              )}
            </For>
          </ol>
        </div>
      ),
    });
  }

  if (top) {
    out.push({
      id: "track",
      theme: "cyan",
      art: top.track,
      render: () => (
        <div class="slide-split reverse">
          <div>
            <p class="eyebrow rv" style={{ "--d": 1 }}>{t.recapTrack}</p>
            <h2 class="recap-name big rv" style={{ "--d": 3 }}>{top.track.title}</h2>
            <p class="recap-sub rv" style={{ "--d": 4 }}>{top.track.artist}</p>
            <p class="recap-stat rv" style={{ "--d": 6 }}>{t.recapTrackSub(top.plays)}</p>
          </div>
          <div class="recap-cover spin-in rv" style={{ "--d": 2 }}>
            <Cover track={top.track} size="xl" />
          </div>
        </div>
      ),
    });
  }

  if (s.topTracks.length >= 3) {
    out.push({
      id: "tracks",
      theme: "blue",
      render: () => (
        <div class="slide-list">
          <h2 class="recap-title rv" style={{ "--d": 1 }}>{t.recapTracks}</h2>
          <ol class="recap-rank">
            <For each={s.topTracks.slice(0, 5)}>
              {(x, i) => (
                <li class="rv" style={{ "--d": 2 + i() * 1.2 }}>
                  <span class="recap-rank-n">{i() + 1}</span>
                  <Cover track={x.track} size="md" />
                  <span class="recap-rank-name">
                    {x.track.title}
                    <small>{x.track.artist}</small>
                  </span>
                  <span class="recap-rank-value">{t.plays(x.plays)}</span>
                </li>
              )}
            </For>
          </ol>
        </div>
      ),
    });
  }

  if (album && album.plays >= 2) {
    out.push({
      id: "album",
      theme: "lime",
      art: album.track,
      render: () => (
        <div class="slide-center">
          <p class="eyebrow rv" style={{ "--d": 1 }}>{t.recapAlbum}</p>
          <div class="recap-cover stack rv" style={{ "--d": 2 }}>
            <Cover track={album.track} size="xl" />
          </div>
          <h2 class="recap-name rv" style={{ "--d": 4 }}>{album.title}</h2>
          <p class="recap-sub rv" style={{ "--d": 5 }}>
            {album.artist} · {t.minutesCount(minutesOf(album.seconds))}
          </p>
        </div>
      ),
    });
  }

  if (s.byMonth.filter((v) => v > 0).length >= 2 && s.busiestMonth) {
    const m = monthName(s.busiestMonth);
    out.push({
      id: "rhythm",
      theme: "violet",
      render: () => (
        <div class="slide-chart">
          <h2 class="recap-title rv" style={{ "--d": 1 }}>{t.recapRhythm}</h2>
          <div class="rv" style={{ "--d": 2 }}>
            <Bars values={s.byMonth} label={(i) => monthName(i + 1, "short")} class="recap-bars" />
          </div>
          <p class="recap-sub rv" style={{ "--d": 6 }}>{t.recapBusiestMonth(m)}</p>
        </div>
      ),
    });
  }

  if (s.favoriteHour != null) {
    const h = s.favoriteHour;
    out.push({
      id: "hour",
      theme: h >= 6 && h < 18 ? "coral" : "night",
      render: () => (
        <div class="slide-split">
          <div class="rv" style={{ "--d": 2 }}>
            <Clock hours={s.byHour} favorite={h} />
          </div>
          <div>
            <h2 class="recap-name big rv" style={{ "--d": 1 }}>{t.recapHourKind(h)}</h2>
            <p class="recap-sub rv" style={{ "--d": 4 }}>{t.recapHourSub(t.hourLabel(h))}</p>
          </div>
        </div>
      ),
    });
  }

  if (s.artists >= 3) {
    out.push({
      id: "curious",
      theme: "lime",
      render: () => (
        <div class="slide-center">
          <p class="eyebrow rv" style={{ "--d": 1 }}>{t.recapCurious}</p>
          <div class="recap-number mid rv" style={{ "--d": 2 }}>
            <CountUp value={s.artists} delay={250} />
          </div>
          <p class="recap-lead rv" style={{ "--d": 3 }}>{t.recapArtistsLabel}</p>
          <Show when={s.hasEarlier && s.newArtists > 0}>
            <p class="recap-sub rv" style={{ "--d": 5 }}>{t.recapNewArtists(s.newArtists)}</p>
          </Show>
          <p class="recap-stat rv" style={{ "--d": 7 }}>{t.recapTracksCount(s.tracks)}</p>
        </div>
      ),
    });
  }

  if (s.busiestDay) {
    const d = s.busiestDay;
    out.push({
      id: "day",
      theme: "cyan",
      render: () => (
        <div class="slide-center">
          <Waves class="low" />
          <p class="eyebrow rv" style={{ "--d": 1 }}>{t.recapDay}</p>
          <div class="recap-huge date rv" style={{ "--d": 2 }}>{dayName(d)}</div>
          <p class="recap-sub rv" style={{ "--d": 4 }}>{t.recapDaySub(minutesOf(d.seconds), d.plays)}</p>
          <Show when={s.longestStreak >= 2}>
            <p class="recap-stat rv" style={{ "--d": 6 }}>{t.recapStreak(s.longestStreak)}</p>
          </Show>
        </div>
      ),
    });
  }

  out.push({
    id: "outro",
    theme: "blue",
    render: () => (
      <div class="slide-outro">
        <Waves />
        <div class="recap-poster rv" style={{ "--d": 1 }}>
          <div class="poster-head">
            <img src="/wordmark.svg" alt="555" height="18" />
            <span>{y}</span>
          </div>
          <Show when={artist}>
            <div class="poster-hero">
              <ArtistPhoto artist={artist!} class="lg" />
              <div>
                <div class="eyebrow">{t.recapArtist}</div>
                <div class="poster-name">{artist!.name}</div>
              </div>
            </div>
          </Show>
          <div class="poster-cols">
            <div>
              <div class="eyebrow">{t.topArtists}</div>
              <ol>
                <For each={s.topArtists.slice(0, 5)}>{(a) => <li>{a.name}</li>}</For>
              </ol>
            </div>
            <div>
              <div class="eyebrow">{t.topTracks}</div>
              <ol>
                <For each={s.topTracks.slice(0, 5)}>{(x) => <li>{x.track.title}</li>}</For>
              </ol>
            </div>
          </div>
          <div class="poster-figures">
            <div>
              <div class="eyebrow">{t.recapMinutes}</div>
              <strong>{number(minutes)}</strong>
            </div>
            <div>
              <div class="eyebrow">{t.artistsLabel}</div>
              <strong>{number(s.artists)}</strong>
            </div>
            <div>
              <div class="eyebrow">{t.tracksLabel}</div>
              <strong>{number(s.tracks)}</strong>
            </div>
          </div>
        </div>
        <div class="outro-text">
          <h1 class="recap-title rv" style={{ "--d": 3 }}>{t.recapOutro(y)}</h1>
          <p class="recap-sub rv" style={{ "--d": 4 }}>{t.recapOutroSub}</p>
        </div>
      </div>
    ),
  });

  return out;
}
