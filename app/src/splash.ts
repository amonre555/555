// Écran de lancement (dessiné dans index.html) : on le retire quand l'accueil est prêt, mais pas
// avant la fin du tracé du logo, et au plus tard après quelques secondes (pas de réseau…).
const MIN_MS = 1250;
const MAX_MS = 4000;
let done = false;

export function dismissSplash() {
  if (done) return;
  done = true;
  const el = document.getElementById("splash");
  if (!el) return;
  const wait = Math.max(0, MIN_MS - performance.now());
  setTimeout(() => {
    el.classList.add("out");
    setTimeout(() => el.remove(), 600);
  }, wait);
}

setTimeout(dismissSplash, MAX_MS);
