// Listening history page: every play, grouped by day, with the time of each play.
import { createMemo, createSignal, For, Show } from "solid-js";
import { formatTime, type Played, type Snapshot, type Track } from "./api";
import { clearHistory, collection } from "./collection";
import { IconClock, IconPlay, IconTrash } from "./icons";
import { locale, t } from "./i18n";
import { TrackCard, TrackList } from "./ui";

type Props = { snap: Snapshot; onPlay: (tracks: Track[], index: number) => void };

const dayKey = (ms: number) => {
  const d = new Date(ms);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
};

function dayLabel(ms: number): string {
  const now = new Date();
  const today = dayKey(now.getTime());
  const yesterday = dayKey(now.getTime() - 86_400_000);
  const key = dayKey(ms);
  if (key === today) return t.today;
  if (key === yesterday) return t.yesterday;
  const d = new Date(ms);
  return new Intl.DateTimeFormat(locale(), {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: d.getFullYear() === now.getFullYear() ? undefined : "numeric",
  }).format(d);
}

const clock = (ms: number) => new Intl.DateTimeFormat(locale(), { hour: "numeric", minute: "2-digit" }).format(new Date(ms));

export function HistoryView(props: Props) {
  const history = () => collection().history;

  /**
   * Chaque morceau une seule fois, à sa dernière écoute (l'historique va du plus récent au plus
   * ancien). Le nombre d'écoutes reste compté dans « Les plus écoutés » et les stats.
   */
  const days = createMemo(() => {
    const out: { key: string; at: number; plays: Played[] }[] = [];
    const seen = new Set<string>();
    for (const p of history()) {
      if (seen.has(p.track.id)) continue;
      seen.add(p.track.id);
      const key = dayKey(p.at);
      const last = out[out.length - 1];
      if (last?.key === key) last.plays.push(p);
      else out.push({ key, at: p.at, plays: [p] });
    }
    return out;
  });

  /** Songs played at least twice, most played first. */
  const mostPlayed = createMemo(() => {
    const count = new Map<string, { track: Track; n: number }>();
    for (const p of history()) {
      const c = count.get(p.track.id);
      if (c) c.n++;
      else count.set(p.track.id, { track: p.track, n: 1 });
    }
    return [...count.values()].filter((c) => c.n > 1).sort((a, b) => b.n - a.n).slice(0, 6);
  });

  const listened = createMemo(() => history().reduce((a, p) => a + (p.track.duration ?? 0), 0));

  const [confirm, setConfirm] = createSignal(false);
  const clear = () => {
    if (!confirm()) {
      setConfirm(true);
      setTimeout(() => setConfirm(false), 4000);
      return;
    }
    setConfirm(false);
    clearHistory();
  };

  return (
    <div class="history-page">
      <div class="page-head">
        <div>
          <h1>{t.history}</h1>
          <Show when={history().length > 0}>
            <p class="muted">
              {t.plays(history().length)} · {formatTime(listened())}
            </p>
          </Show>
        </div>
        <Show when={history().length > 0}>
          <div class="actions">
            <button class="primary" onClick={() => props.onPlay(history().map((p) => p.track), 0)}>
              <IconPlay size={16} /> {t.playAll}
            </button>
            <button classList={{ ghost: true, danger: confirm() }} onClick={clear}>
              <IconTrash /> {confirm() ? t.confirmDelete : t.clearHistory}
            </button>
          </div>
        </Show>
      </div>

      <Show
        when={history().length > 0}
        fallback={
          <div class="empty-state">
            <IconClock size={26} />
            <h2>{t.historyEmpty}</h2>
            <p class="muted">{t.historyEmptyText}</p>
          </div>
        }
      >
        <Show when={mostPlayed().length >= 3}>
          <section class="section">
            <h2 class="section-title">{t.mostPlayed}</h2>
            <div class="grid recent">
              <For each={mostPlayed()}>
                {(c, i) => (
                  <TrackCard
                    track={c.track}
                    index={i()}
                    playing={props.snap.track?.id === c.track.id}
                    onPlay={() => props.onPlay(mostPlayed().map((m) => m.track), i())}
                    note={t.plays(c.n)}
                  />
                )}
              </For>
            </div>
          </section>
        </Show>

        <For each={days()}>
          {(day) => {
            const tracks = () => day.plays.map((p) => p.track);
            return (
              <section class="section">
                <h2 class="section-title history-day">
                  {dayLabel(day.at)}
                  <span class="muted count-inline">{day.plays.length}</span>
                </h2>
                {/* No "now playing" highlight: a song played several times would light up everywhere. */}
                <TrackList
                  tracks={tracks()}
                  status={props.snap.status}
                  onPlay={(i) => props.onPlay(tracks(), i)}
                  aside={(i) => clock(day.plays[i].at)}
                />
              </section>
            );
          }}
        </For>
      </Show>
    </div>
  );
}
