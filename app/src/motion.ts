// Transitions entre les vues. Cas courant : la nouvelle page sort du flou, animée directement sur
// le vrai contenu. Image cliquée qui devient celle de la fiche (pochette, photo) : API View
// Transitions, seule capable de ce morphing. Si l'utilisateur a réduit les animations, le
// changement est immédiat.

type ViewTransition = { finished: Promise<void> };
type WithTransitions = Document & { startViewTransition?: (update: () => void) => ViewTransition };

const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");

/** Genre de transition, lu par le CSS (`html[data-vt="…"]`). */
export type TransitionKind = "page" | "back";

let running: ViewTransition | null = null;

/** Arrivée d'une page (même mouvement que `blur-in` / `blur-in-back` dans styles.css). */
const PAGE_IN: Record<TransitionKind, Keyframe[]> = {
  page: [
    { opacity: 0, filter: "blur(14px)", transform: "scale(1.015) translateY(10px)" },
    { opacity: 1, filter: "blur(0px)", transform: "none" },
  ],
  back: [
    { opacity: 0, filter: "blur(14px)", transform: "scale(0.985) translateY(-10px)" },
    { opacity: 1, filter: "blur(0px)", transform: "none" },
  ],
};
/**
 * Téléphone : même arrivée, sans flou. Flouter toute la page à chaque image coûte cher aux
 * petits processeurs (saccades au changement d'onglet).
 */
const PAGE_IN_LIGHT: Record<TransitionKind, Keyframe[]> = {
  page: [{ opacity: 0, transform: "translateY(10px)" }, { opacity: 1, transform: "none" }],
  back: [{ opacity: 0, transform: "translateY(-10px)" }, { opacity: 1, transform: "none" }],
};
const light = window.matchMedia("(max-width: 760px)");
let pageAnim: Animation | null = null;

/**
 * Applique `update` avec une transition animée quand c'est possible.
 *
 * Barre latérale, liens, retour : rien ne recouvre l'écran pendant l'animation. La souris garde
 * son curseur et ses survols, et un clic pendant l'animation affiche simplement la page suivante,
 * qui sort du flou à son tour.
 *
 * Morphing d'image (`nameForTransition` appelé juste avant) : View Transitions. Pendant ces
 * 0,6 s, le navigateur affiche des captures de la page, qui ne réagissent pas à la souris.
 */
export function transition(update: () => void, kind: TransitionKind = "page") {
  const doc = document as WithTransitions;
  if (reduced.matches || document.hidden) {
    update();
    return;
  }
  // Téléphone : pas de morphing d'image (captures de toute la page, trop lourdes).
  const morph = !light.matches && document.querySelector("[data-vt-temp]") !== null;
  if (morph && doc.startViewTransition) {
    const root = document.documentElement;
    root.dataset.vt = kind;
    const t = doc.startViewTransition(update);
    running = t;
    t.finished.finally(() => {
      // Interrompue par une autre : c'est la suivante qui nettoiera, sinon on effacerait le nom
      // qu'elle vient de poser sur l'image cliquée.
      if (running !== t) return;
      delete root.dataset.vt;
      running = null;
      // Les noms posés pour la transition (pochette cliquée…) ne doivent pas resservir.
      document.querySelectorAll<HTMLElement>("[data-vt-temp]").forEach((el) => {
        el.style.viewTransitionName = "";
        delete el.dataset.vtTemp;
      });
    });
    return;
  }
  update();
  const page = document.querySelector<HTMLElement>(".content > .page");
  if (!page) return;
  pageAnim?.cancel();
  pageAnim = light.matches
    ? page.animate(PAGE_IN_LIGHT[kind], { duration: 280, easing: "cubic-bezier(0.25, 1, 0.3, 1)" })
    : page.animate(PAGE_IN[kind], { duration: 450, easing: "cubic-bezier(0.25, 1, 0.3, 1)" });
}

/** Donne à `el` le nom partagé avec l'élément d'arrivée, le temps d'une transition. */
export function nameForTransition(el: Element | null | undefined, name: string) {
  if (!(el instanceof HTMLElement)) return;
  el.style.viewTransitionName = name;
  el.dataset.vtTemp = "";
}
