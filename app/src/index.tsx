import { render } from "solid-js/web";
// Google Sans Flex (licence OFL), intégrée : l'app reste utilisable hors ligne. La version
// complète expose l'axe « rond » (ROND), qu'on utilise pour des titres plus doux.
import "@fontsource-variable/google-sans-flex/full.css";
import "./styles.css";
import { installShapes } from "./shapes";

installShapes();

async function start() {
  // Aperçu dans un navigateur, hors de Tauri : on simule le cœur Rust (développement seulement).
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    await import("./dev-mock");
  }
  const { default: App } = await import("./App");
  render(() => <App />, document.getElementById("root")!);
}

start();
