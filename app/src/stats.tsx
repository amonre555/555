// Page Stats : temps d'écoute, classements et rythme d'écoute sur une période (ce mois-ci,
// une année, depuis toujours), et l'entrée du récap de l'année (recap.tsx).
import { createEffect, createMemo, createResource, createSignal, For, on, Show } from "solid-js";
import { api, albumRefOf, type Period, type Snapshot, type Stats, type TopArtist, type Track } from "./api";
import { notify } from "./collection";
import { IconChart, IconTrash, IconUser, IconWaves } from "./icons";
import { lang, locale, t } from "./i18n";
import { openAlbum, openArtist } from "./nav";
import { Cover, Loader, TrackList } from "./ui";
import "./stats.css";

// ---------- Mise en forme, partagée avec le récap ----------

export const minutesOf = (seconds: number) => Math.round(seconds / 60);

/** « 12 h 05 min », ou « 42 min » sous l'heure. */
export function duration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.round((seconds % 3600) / 60);
  return h > 0 ? t.hours(h, String(m).padStart(2, "0")) : t.minutes(m);
}

export const monthName = (m: number, style: "long" | "short" = "long") =>
  new Intl.DateTimeFormat(locale(), { month: style }).format(new Date(2024, m - 1, 1));

/** Jours de la semaine, lundi en premier (le 1er janvier 2024 était un lundi). */
const weekdayName = (i: number) => new Intl.DateTimeFormat(locale(), { weekday: "short" }).format(new Date(2024, 0, 1 + i));

export const dayName = (d: { year: number; month: number; day: number }, withYear = false) =>
  new Intl.DateTimeFormat(locale(), { day: "numeric", month: "long", year: withYear ? "numeric" : undefined }).format(
    new Date(d.year, d.month - 1, d.day),
  );

export const number = (n: number) => n.toLocaleString(locale());

/** Décembre : le récap de l'année s'ouvre, et il est mis en avant (`?december` dans l'aperçu, pour l'essayer). */
export const recapSeason = () =>
  new Date().getMonth() === 11 || (import.meta.env.DEV && new URLSearchParams(location.search).has("december"));

/** Le récap d'une année n'existe qu'en fin d'année (décembre), puis pour toujours une fois l'année finie. */
export const recapAvailable = (year: number) => year < new Date().getFullYear() || (year === new Date().getFullYear() && recapSeason());

// ---------- Photos d'artistes (fiche Deezer), chargées une fois ----------

const pictures = new Map<string, () => string | null>();

/** Photo d'un artiste ; `null` tant qu'elle n'est pas arrivée (ou s'il n'y en a pas). */
export function artistPicture(a: { name: string; id: string | null }): () => string | null {
  const key = a.id ?? a.name.toLowerCase();
  const known = pictures.get(key);
  if (known) return known;
  const [pic, setPic] = createSignal<string | null>(null);
  pictures.set(key, pic);
  api.artist({ name: a.name, id: a.id }).then((p) => setPic(p.picture), () => {});
  return pic;
}

/** Photo d'un artiste, ou la pochette de son morceau le plus écouté en attendant. */
export function ArtistPhoto(props: { artist: TopArtist; class?: string }) {
  const pic = createMemo(() => artistPicture(props.artist));
  return (
    <div class={`stat-photo ${props.class ?? ""}`}>
      <Show when={pic()() ?? props.artist.track.artwork} fallback={<IconUser />}>
        {(u) => <img src={u()} alt="" loading="lazy" decoding="async" />}
      </Show>
    </div>
  );
}

// ---------- Page ----------

type Kind = "month" | "year" | "all";

export function StatsView(props: { snap: Snapshot; onPlay: (tracks: Track[], index: number) => void; onRecap: (year: number) => void }) {
  const now = new Date();
  const [kind, setKind] = createSignal<Kind>("year");
  const [year, setYear] = createSignal(now.getFullYear());
  const period = createMemo<Period>(() =>
    kind() === "all" ? { kind: "all" } : kind() === "month" ? { kind: "month", year: now.getFullYear(), month: now.getMonth() + 1 } : { kind: "year", year: year() },
  );
  const [stats, { refetch }] = createResource(period, (p) => api.stats(p));
  // Une écoute est notée quand le morceau change : on met la page à jour à ce moment-là.
  createEffect(on(() => props.snap.track?.id, () => refetch(), { defer: true }));

  /** Dernières données reçues : la page ne clignote pas quand on change de période. */
  const [shown, setShown] = createSignal<Stats | null>(null);
  createEffect(() => {
    const s = stats();
    if (s) setShown(s);
  });
  const years = () => {
    const list = shown()?.years ?? [];
    return list.includes(now.getFullYear()) ? list : [now.getFullYear(), ...list];
  };
  const recapYear = () => (kind() === "year" ? year() : now.getFullYear());

  const [confirm, setConfirm] = createSignal(false);
  const clear = async () => {
    if (!confirm()) {
      setConfirm(true);
      setTimeout(() => setConfirm(false), 4000);
      return;
    }
    setConfirm(false);
    try {
      await api.clearStats();
      notify(t.statsCleared);
      refetch();
    } catch (e) {
      notify(String(e), "error");
    }
  };

  return (
    <div class="stats-page">
      <div class="page-head">
        <div>
          <h1>{t.statsTitle}</h1>
          <Show when={shown()}>{(s) => <p class="muted">{t.plays(s().plays)} · {duration(s().seconds)}</p>}</Show>
        </div>
        <div class="actions stats-period" role="group" aria-label={t.periodLabel}>
          <div class="segmented">
            <For each={[["month", t.periodMonth], ["year", t.periodYear], ["all", t.periodAll]] as [Kind, string][]}>
              {([k, label]) => (
                <button classList={{ on: kind() === k }} aria-pressed={kind() === k} onClick={() => setKind(k)}>
                  {k === "year" && year() !== now.getFullYear() ? String(year()) : label}
                </button>
              )}
            </For>
          </div>
          <Show when={kind() === "year" && years().length > 1}>
            <select class="year-select" aria-label={t.yearLabel} value={year()} onChange={(e) => setYear(Number(e.currentTarget.value))}>
              <For each={years()}>{(y) => <option value={y}>{y}</option>}</For>
            </select>
          </Show>
        </div>
      </div>

      <Show when={recapAvailable(recapYear()) && (shown()?.years ?? []).includes(recapYear())}>
        <RecapCard year={recapYear()} onOpen={() => props.onRecap(recapYear())} />
      </Show>

      <Show when={shown()} fallback={<Loader />}>
        {(s) => (
          <Show
            when={s().plays > 0}
            fallback={
              <div class="empty-state">
                <IconChart size={26} />
                <h2>{(s().years.length ? t.statsEmptyPeriod : t.statsEmpty)}</h2>
                <Show when={!s().years.length}>
                  <p class="muted">{t.statsEmptyText}</p>
                </Show>
              </div>
            }
          >
            <div class="stat-tiles" style={{ "--n": 5 }}>
              <Tile label={t.listeningTime} value={duration(s().seconds)} big />
              <Tile label={t.playsLabel} value={number(s().plays)} />
              <Tile label={t.tracksLabel} value={number(s().tracks)} />
              <Tile label={t.artistsLabel} value={number(s().artists)} />
              <Tile label={t.albumsLabel} value={number(s().albums)} />
            </div>

            <div class="stats-columns">
              <section class="section">
                <h2 class="section-title">{t.topTracks}</h2>
                <TrackList
                  tracks={s().topTracks.map((x) => x.track)}
                  current={props.snap.track?.id}
                  status={props.snap.status}
                  onPlay={(i) => props.onPlay(s().topTracks.map((x) => x.track), i)}
                  aside={(i) => t.plays(s().topTracks[i]?.plays ?? 0)}
                />
              </section>
              <section class="section">
                <h2 class="section-title">{t.topArtists}</h2>
                <ol class="top-artists">
                  <For each={s().topArtists}>
                    {(a, i) => (
                      <li style={{ "--i": i() }}>
                        <button class="top-artist" onClick={(e) => openArtist({ name: a.name, id: a.id }, e.currentTarget.querySelector("img") ?? undefined)}>
                          <span class="top-rank">{i() + 1}</span>
                          <ArtistPhoto artist={a} />
                          <span class="top-name">{a.name}</span>
                          <span class="top-value">{t.minutesCount(minutesOf(a.seconds))}</span>
                        </button>
                      </li>
                    )}
                  </For>
                </ol>
              </section>
            </div>

            <Show when={s().topAlbums.length > 0}>
              <section class="section">
                <h2 class="section-title">{t.topAlbums}</h2>
                <div class="grid">
                  <For each={s().topAlbums}>
                    {(a, i) => (
                      <button
                        class="card"
                        style={{ "--i": i() }}
                        onClick={(e) => {
                          const ref = a.id ? { id: a.id, title: a.title, artist: a.artist } : albumRefOf(a.track);
                          if (ref) openAlbum(ref, e.currentTarget.querySelector("img") ?? undefined);
                        }}
                      >
                        <div class="card-art">
                          <Cover track={a.track} size="lg" />
                        </div>
                        <div class="card-title">{a.title}</div>
                        <div class="card-sub">{a.artist}</div>
                        <div class="card-note">{t.minutesCount(minutesOf(a.seconds))}</div>
                      </button>
                    )}
                  </For>
                </div>
              </section>
            </Show>

            <div class="stats-columns">
              <section class="section">
                <Show
                  when={s().period.kind !== "month"}
                  fallback={
                    <>
                      <h2 class="section-title">{t.byWeekday}</h2>
                      <Bars values={s().byWeekday} label={(i) => weekdayName(i)} />
                    </>
                  }
                >
                  <h2 class="section-title">{t.byMonth}</h2>
                  <Bars values={s().byMonth} label={(i) => monthName(i + 1, "short")} />
                </Show>
              </section>
              <section class="section">
                <h2 class="section-title">{t.byHour}</h2>
                <Bars values={s().byHour} label={(i) => (i % 6 === 0 ? t.hourLabel(i) : "")} tipLabel={(i) => t.hourLabel(i)} />
              </section>
            </div>

            <div class="stat-tiles insights">
              <Show when={s().busiestDay}>
                {(d) => <Tile label={t.busiestDay} value={dayName(d(), s().period.kind === "all")} note={t.minutesCount(minutesOf(d().seconds))} />}
              </Show>
              <Show when={s().favoriteHour != null}>
                <Tile label={t.favoriteHour} value={t.hourLabel(s().favoriteHour!)} note={t.recapHourKind(s().favoriteHour!)} />
              </Show>
              <Tile label={t.longestStreak} value={t.daysCount(s().longestStreak)} note={`${t.activeDays} · ${number(s().activeDays)}`} />
              <Show when={s().hasEarlier}>
                <Tile label={t.newArtists} value={number(s().newArtists)} note={`/ ${number(s().artists)}`} />
              </Show>
            </div>

            <div class="stats-foot">
              <span class="muted small">
                {s()
                  .bySource.map((x) => `${t.sourceNames[x.source] ?? x.source} ${Math.round((x.seconds / Math.max(1, s().seconds)) * 100)} %`)
                  .join(" · ")}
              </span>
              <button classList={{ ghost: true, danger: confirm() }} onClick={clear}>
                <IconTrash /> {confirm() ? t.confirmDelete : t.clearStats}
              </button>
            </div>
          </Show>
        )}
      </Show>
    </div>
  );
}

function Tile(props: { label: string; value: string; note?: string; big?: boolean }) {
  return (
    <div classList={{ "stat-tile": true, big: !!props.big }}>
      <div class="eyebrow">{props.label}</div>
      <div class="stat-value">{props.value}</div>
      <Show when={props.note}>
        <div class="muted small">{props.note}</div>
      </Show>
    </div>
  );
}

/** Histogramme d'une seule série : une barre par mois, heure ou jour ; la plus haute ressort. */
export function Bars(props: { values: number[]; label: (i: number) => string; tipLabel?: (i: number) => string; class?: string }) {
  const max = () => Math.max(1, ...props.values);
  const top = () => props.values.indexOf(Math.max(...props.values));
  return (
    <div class={`bars ${props.class ?? ""}`} style={{ "--n": props.values.length }} lang={lang()}>
      <For each={props.values}>
        {(v, i) => (
          <div class="bar-col" data-tip={`${(props.tipLabel ?? props.label)(i())} · ${t.minutesCount(minutesOf(v))}`}>
            <div classList={{ bar: true, top: i() === top() && v > 0 }} style={{ "--h": v / max(), "--i": i() }} />
            <span class="bar-label">{props.label(i())}</span>
          </div>
        )}
      </For>
    </div>
  );
}

/** Carte d'entrée du récap de l'année (page Stats, et Découvrir en décembre). */
export function RecapCard(props: { year: number; onOpen: () => void; onDismiss?: () => void; title?: string }) {
  return (
    <div class="recap-card">
      <div class="recap-card-rings" aria-hidden="true">
        <i /><i /><i /><i />
      </div>
      <div class="recap-card-body">
        <div class="eyebrow">555 · {props.year}</div>
        <h2>{props.title ?? t.recapCardTitle(props.year)}</h2>
        <p class="muted">{t.recapCardText}</p>
      </div>
      <div class="actions">
        <Show when={props.onDismiss}>
          <button class="ghost" onClick={props.onDismiss}>{t.later}</button>
        </Show>
        <button class="primary" onClick={props.onOpen}>
          <IconWaves size={18} /> {t.openRecap(props.year)}
        </button>
      </div>
    </div>
  );
}
