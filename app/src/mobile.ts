// Téléphone : écran étroit (même seuil que `@media (max-width: 760px)` dans styles.css). Sur
// ordinateur, la fenêtre ne descend jamais aussi bas (880 px minimum) : rien n'y change.
import { createSignal } from "solid-js";

const query = window.matchMedia("(max-width: 760px)");
const [mobile, setMobile] = createSignal(query.matches);
query.addEventListener("change", (e) => setMobile(e.matches));
export const isMobile = mobile;

/** Android (bouton retour, réglages sans objet comme Discord). */
export const isAndroid = /Android/i.test(navigator.userAgent);

/**
 * Bouton « retour » d'Android : la WebView revient en arrière dans son historique, sinon ferme
 * l'appli. On garde donc toujours une entrée d'historique « tampon » : un retour la consomme
 * (`popstate`), on demande à l'appli de reculer d'un cran (fermer un menu, la vue « En lecture »,
 * revenir à la page précédente…), puis on remet le tampon. Quand il n'y a plus rien à fermer,
 * on laisse le retour sortir de l'appli.
 */
export function handleBackButton(back: () => boolean) {
  if (!isAndroid) return () => {};
  history.pushState({ buffer: true }, "");
  const onPop = () => {
    if (back()) history.pushState({ buffer: true }, "");
    else history.back();
  };
  window.addEventListener("popstate", onPop);
  return () => window.removeEventListener("popstate", onPop);
}

/**
 * Marges des barres système d'Android (barre d'état en haut, navigation en bas), données par
 * l'activité (MainActivity.kt) : elles remplacent `env(safe-area-inset-*)` dans le CSS.
 */
type Insets = { top(): number; bottom(): number };
const native = (window as unknown as { AndroidInsets?: Insets }).AndroidInsets;
function applyInsets() {
  if (!native) return;
  const root = document.documentElement.style;
  root.setProperty("--sat", `${native.top()}px`);
  root.setProperty("--sab", `${native.bottom()}px`);
}
if (native) {
  applyInsets();
  // Les marges arrivent parfois juste après le chargement, et changent avec la rotation.
  for (const ms of [100, 400, 1200]) setTimeout(applyInsets, ms);
  window.addEventListener("resize", applyInsets);
}

/**
 * Clavier ouvert (la page rétrécit nettement) : onglets et mini-lecteur se cachent, pour laisser
 * la place aux résultats. Classe `kb-open` sur <html>.
 */
let tallest = window.innerHeight;
function watchKeyboard() {
  tallest = Math.max(tallest, window.innerHeight);
  document.documentElement.classList.toggle("kb-open", isMobile() && window.innerHeight < tallest * 0.78);
}
window.addEventListener("resize", watchKeyboard);
// Rotation : la hauteur de référence change.
screen.orientation?.addEventListener("change", () => {
  tallest = 0;
  setTimeout(watchKeyboard, 300);
});
