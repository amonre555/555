import { render } from "solid-js/web";
// Fonts (OFL license), bundled so the app works offline: Unbounded for titles and the brand,
// Manrope for everything else.
import "@fontsource-variable/unbounded";
import "@fontsource-variable/manrope";
import "./styles.css";

async function start() {
  // Aperçu dans un navigateur, hors de Tauri : on simule le cœur Rust. En développement, et dans
  // la démo du site (`vite build --mode demo`, aperçus de la galerie de thèmes) ; jamais dans l'appli.
  if ((import.meta.env.DEV || import.meta.env.VITE_DEMO === "1") && !("__TAURI_INTERNALS__" in window)) {
    await import("./dev-mock");
  }
  const { default: App } = await import("./App");
  render(() => <App />, document.getElementById("root")!);
}

start();
