// Recherches récentes : les ~10 dernières recherches (gardées par le cœur, sans doublon, la plus
// récente en tête), proposées sous le champ de recherche quand il est vide et qu'il a le focus.
import { createSignal, For } from "solid-js";
import { api } from "./api";
import { IconClock, IconClose } from "./icons";
import { t } from "./i18n";

const [recent, setRecent] = createSignal<string[]>([]);
export { recent };

const apply = (p: Promise<string[]>) => p.then(setRecent).catch(() => {});

export const loadRecentSearches = () => apply(api.recentSearches());

/** Note une recherche « aboutie » (Entrée, résultat lu ou ouvert). */
export function rememberSearch(query: string) {
  const q = query.trim();
  if (q && recent()[0] !== q) apply(api.addRecentSearch(q));
}

/**
 * Liste déroulante sous le champ. Les clics passent par `mousedown` (avec `preventDefault`) :
 * le champ garde le focus, et la liste ne disparaît pas avant d'avoir reçu le clic.
 */
export function RecentSearches(props: { active: number; onPick: (q: string) => void; onHover: (i: number) => void }) {
  const hold = (e: MouseEvent) => e.preventDefault();
  return (
    <div class="recent-searches" role="listbox" aria-label={t.recentSearches} onMouseDown={hold}>
      <div class="recent-head">
        <span class="eyebrow">{t.recentSearches}</span>
        <button class="link-btn" onMouseDown={hold} onClick={() => apply(api.clearRecentSearches())}>
          {t.clearAll}
        </button>
      </div>
      <For each={recent()}>
        {(q, i) => (
          <div
            classList={{ "recent-item": true, active: props.active === i() }}
            role="option"
            aria-selected={props.active === i()}
            style={{ "--i": i() }}
            onMouseEnter={() => props.onHover(i())}
            onClick={() => props.onPick(q)}
          >
            <IconClock size={16} />
            <span class="recent-text">{q}</span>
            <button
              class="recent-x"
              title={t.removeSearch(q)}
              aria-label={t.removeSearch(q)}
              onClick={(e) => {
                e.stopPropagation();
                apply(api.removeRecentSearch(q));
              }}
            >
              <IconClose />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
