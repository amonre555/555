// Briques d'interface réutilisables : pochettes, listes, états vides.
import { createSignal, For, Match, Show, Switch } from "solid-js";
import { api, artistsOf, coverUrl, formatTime, type ArtistRef, type Snapshot, type Track, type YtDlpStatus } from "./api";
import { openArtist } from "./nav";
import { IconNote, IconPlay } from "./icons";

export function Loader() {
  return (
    <div class="loader" aria-label="Chargement">
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
export function ArtistLinks(props: { track: Track; artists?: ArtistRef[] }) {
  const list = () => props.artists ?? artistsOf(props.track);
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
              title={`Voir la fiche de ${a.name}`}
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
}) {
  return (
    <div classList={{ tracklist: true, "with-album": !!props.showAlbum }} role="list">
      <For each={props.tracks}>
        {(t, i) => {
          const isCurrent = () => props.current === t.id;
          return (
            <div
              role="listitem"
              classList={{ row: true, current: isCurrent() }}
              onDblClick={() => props.onPlay(i())}
              onKeyDown={(e) => e.key === "Enter" && props.onPlay(i())}
              onMouseEnter={() => hoverPrefetch(t)}
              onMouseLeave={cancelHover}
              tabIndex={0}
            >
              <div class="row-num">
                <span class="num">
                  <Show when={isCurrent() && props.status === "playing"} fallback={i() + 1}>
                    <span class="eq"><i /><i /><i /></span>
                  </Show>
                </span>
                <button class="row-play" title="Lire" onClick={() => props.onPlay(i())}>
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
              <Show when={props.showAlbum}>
                <div class="row-album">{t.album ?? ""}</div>
              </Show>
              <div class="row-time">{formatTime(t.duration)}</div>
            </div>
          );
        }}
      </For>
    </div>
  );
}

/** Carte de morceau (grilles « Découvrir »), avec bouton lecture au survol. */
export function TrackCard(props: { track: Track; playing: boolean; onPlay: () => void }) {
  return (
    <button
      classList={{ card: true, playing: props.playing }}
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
          <div class="engine" title="yt-dlp est téléchargé depuis ses versions officielles, et sa signature est vérifiée.">
            <div class="engine-label">Installation du moteur YouTube… {Math.round(st().progress * 100)} %</div>
            <div class="engine-bar"><div style={{ width: `${st().progress * 100}%` }} /></div>
          </div>
        )}
      </Match>
      <Match when={s().state === "error" && (s() as { message: string })}>
        {(st) => (
          <div class="engine error" title={st().message}>
            YouTube indisponible : {st().message}
          </div>
        )}
      </Match>
    </Switch>
  );
}
