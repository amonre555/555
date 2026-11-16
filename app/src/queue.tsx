// File d'attente : bouton de la barre du lecteur (et de la vue « En lecture » sur téléphone)
// qui ouvre un panneau. Les morceaux de la file passent avant la suite de la liste ; on peut
// les retirer, les déplacer (glisser-déposer) ou les lancer tout de suite.
import { createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { api, type Snapshot, type Track } from "./api";
import { openTrackMenu } from "./collection";
import { IconClose, IconPlaylist } from "./icons";
import { t as tr } from "./i18n";
import { ArtistLinks, Cover, ExplicitBadge } from "./ui";

type Run = (a: () => Promise<Snapshot>) => void;

/** Panneau flottant ouvert par un bouton, fermé par un clic à côté ou Échap. */
export function Popover(props: { icon: JSX.Element; title: string; active?: boolean; class?: string; children: (close: () => void) => JSX.Element }) {
  const [open, setOpen] = createSignal(false);
  let root!: HTMLDivElement;
  onMount(() => {
    const outside = (e: PointerEvent) => open() && !root.contains(e.target as Node) && !(e.target as HTMLElement).closest?.(".menu-layer") && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && open() && setOpen(false);
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", esc);
    onCleanup(() => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", esc);
    });
  });
  return (
    <div class="eq-pop" ref={root}>
      <button classList={{ "icon-btn": true, active: open() || !!props.active }} title={props.title} aria-expanded={open()} onClick={() => setOpen(!open())}>
        {props.icon}
      </button>
      <Show when={open()}>
        <div class={`eq-popover ${props.class ?? ""}`} role="dialog" aria-label={props.title}>
          {props.children(() => setOpen(false))}
        </div>
      </Show>
    </div>
  );
}

export function QueueButton(props: { snap: Snapshot; run: Run }) {
  return (
    <Popover icon={<IconPlaylist size={20} />} title={tr.queue} active={props.snap.upNext.length > 0} class="queue-popover">
      {() => <QueuePanel snap={props.snap} run={props.run} />}
    </Popover>
  );
}

function QueuePanel(props: { snap: Snapshot; run: Run }) {
  const [dragFrom, setDragFrom] = createSignal<number | null>(null);
  const [dragOver, setDragOver] = createSignal<number | null>(null);
  const endDrag = () => (setDragFrom(null), setDragOver(null));
  return (
    <>
      <div class="eq-head">
        <span class="eq-title">{tr.queue}</span>
        <button class="eq-reset" onClick={() => props.run(api.clearQueued)} disabled={props.snap.upNext.length === 0}>
          {tr.clearQueue}
        </button>
      </div>
      <div class="queue-scroll">
        <Show when={props.snap.track}>
          {(t) => (
            <>
              <div class="menu-label">{tr.nowPlayingLabel}</div>
              <QueueRow track={t()} current />
            </>
          )}
        </Show>
        <div class="menu-label">{tr.nextInQueue}</div>
        <Show when={props.snap.upNext.length > 0} fallback={<p class="queue-empty">{tr.queueEmpty}</p>}>
          <For each={props.snap.upNext}>
            {(q, i) => (
              <QueueRow
                track={q.track}
                draggable
                classList={{
                  dragging: dragFrom() === i(),
                  "drop-before": dragOver() === i() && dragFrom() !== null && dragFrom()! > i(),
                  "drop-after": dragOver() === i() && dragFrom() !== null && dragFrom()! < i(),
                }}
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
                  if (from !== null && from !== i()) props.run(() => api.moveQueued(q.key, i()));
                  endDrag();
                }}
                onDragEnd={endDrag}
                onPlay={() => props.run(() => api.playQueued(q.key))}
                onRemove={() => props.run(() => api.removeQueued(q.key))}
                onMoveUp={i() > 0 ? () => props.run(() => api.moveQueued(q.key, i() - 1)) : undefined}
              />
            )}
          </For>
        </Show>
        <Show when={props.snap.upcoming.length > 0}>
          <div class="menu-label">{tr.nextFromList}</div>
          <For each={props.snap.upcoming}>{(t) => <QueueRow track={t} />}</For>
        </Show>
      </div>
    </>
  );
}

function QueueRow(props: {
  track: Track;
  current?: boolean;
  draggable?: boolean;
  classList?: Record<string, boolean>;
  onDragStart?: (e: DragEvent) => void;
  onDragOver?: (e: DragEvent) => void;
  onDrop?: (e: DragEvent) => void;
  onDragEnd?: () => void;
  onPlay?: () => void;
  onRemove?: () => void;
  onMoveUp?: () => void;
}) {
  return (
    <div
      classList={{ "queue-row": true, current: !!props.current, ...props.classList }}
      draggable={props.draggable}
      onDragStart={props.onDragStart}
      onDragOver={props.onDragOver}
      onDrop={props.onDrop}
      onDragEnd={props.onDragEnd}
      onDblClick={props.onPlay}
      onContextMenu={(e) => openTrackMenu(e, [props.track])}
    >
      <Cover track={props.track} />
      <div class="row-main">
        <div class="row-title">
          <ExplicitBadge track={props.track} />
          {props.track.title}
        </div>
        <div class="row-sub">
          <ArtistLinks track={props.track} />
        </div>
      </div>
      <Show when={props.onMoveUp}>
        <button class="icon-btn queue-up" title={tr.moveUp} onClick={props.onMoveUp}>
          <span aria-hidden="true">↑</span>
        </button>
      </Show>
      <Show when={props.onRemove}>
        <button class="icon-btn" title={tr.removeFromQueue} onClick={props.onRemove}>
          <IconClose />
        </button>
      </Show>
    </div>
  );
}
