// Transitions entre les vues, via l'API View Transitions (WebKit récent, WebView2) : l'ancienne
// page s'efface, la nouvelle arrive en glissant ; un élément nommé des deux côtés (pochette,
// photo) se transforme de l'un à l'autre. Sans l'API, ou si l'utilisateur a réduit les
// animations, le changement est immédiat.

type ViewTransition = { finished: Promise<void> };
type WithTransitions = Document & { startViewTransition?: (update: () => void) => ViewTransition };

const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");

/** Genre de transition, lu par le CSS (`html[data-vt="…"]`). */
export type TransitionKind = "page" | "back";

let running: ViewTransition | null = null;

/** Applique `update` avec une transition animée quand c'est possible. */
export function transition(update: () => void, kind: TransitionKind = "page") {
  const doc = document as WithTransitions;
  if (!doc.startViewTransition || reduced.matches || document.hidden) {
    update();
    return;
  }
  const root = document.documentElement;
  root.dataset.vt = kind;
  const t = doc.startViewTransition(update);
  running = t;
  t.finished.finally(() => {
    if (running === t) {
      delete root.dataset.vt;
      running = null;
    }
    // Les noms posés pour la transition (pochette cliquée…) ne doivent pas resservir.
    document.querySelectorAll<HTMLElement>("[data-vt-temp]").forEach((el) => {
      el.style.viewTransitionName = "";
      delete el.dataset.vtTemp;
    });
  });
}

/** Donne à `el` le nom partagé avec l'élément d'arrivée, le temps d'une transition. */
export function nameForTransition(el: Element | null | undefined, name: string) {
  if (!(el instanceof HTMLElement)) return;
  el.style.viewTransitionName = name;
  el.dataset.vtTemp = "";
}
