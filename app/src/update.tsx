// Mises à jour automatiques : yato regarde au démarrage (puis toutes les 6 h) si une nouvelle
// version est publiée dans les Releases GitHub. Si oui, une carte le propose dans la barre
// latérale ; un clic télécharge la version (signée, vérifiée par l'extension), l'installe et
// relance l'app. Rien ne s'affiche s'il n'y a rien à faire ou si la vérification échoue
// (hors ligne, aperçu navigateur…).
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { createSignal, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { notify } from "./collection";
import { IconDownload } from "./icons";

const FIRST_CHECK_MS = 4000;
const EVERY_MS = 6 * 3600 * 1000;

type State =
  | { kind: "idle" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; update: Update; done: number; total: number | null }
  | { kind: "installing"; update: Update };

// État partagé : la carte l'affiche, la page Réglages peut lancer une vérification.
const [state, setState] = createSignal<State>({ kind: "idle" });

/**
 * Cherche une mise à jour. Renvoie `"available"` (la carte apparaît), `"none"` (yato est à
 * jour) ou `"error"` (pas de connexion…).
 */
export async function checkForUpdates(): Promise<"available" | "none" | "error"> {
  if (state().kind !== "idle") return "available";
  try {
    const update = await check();
    if (!update) return "none";
    setState({ kind: "available", update });
    return "available";
  } catch {
    return "error";
  }
}

export function UpdateCard() {
  // Échec silencieux (pas de connexion, aperçu sans Tauri…) : on réessaiera plus tard.
  const lookup = () => void checkForUpdates();

  onMount(() => {
    const first = window.setTimeout(lookup, FIRST_CHECK_MS);
    const every = window.setInterval(lookup, EVERY_MS);
    onCleanup(() => {
      clearTimeout(first);
      clearInterval(every);
    });
  });

  const install = async () => {
    const s = state();
    if (s.kind !== "available") return;
    const update = s.update;
    let done = 0;
    let total: number | null = null;
    setState({ kind: "downloading", update, done, total });
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? null;
        else if (e.event === "Progress") done += e.data.chunkLength;
        else if (e.event === "Finished") return setState({ kind: "installing", update });
        setState({ kind: "downloading", update, done, total });
      });
      setState({ kind: "installing", update });
      await relaunch();
    } catch (e) {
      notify(`Mise à jour impossible : ${e}`, "error");
      setState({ kind: "available", update });
    }
  };

  const pct = () => {
    const s = state();
    return s.kind === "downloading" && s.total ? Math.min(100, (s.done / s.total) * 100) : null;
  };

  return (
    <Show when={state().kind !== "idle" && state()}>
      {(s) => {
        const version = () => (s() as Exclude<State, { kind: "idle" }>).update.version;
        return (
          <div class="update-card" role="status">
            <div class="update-title">yato {version()}</div>
            <Switch>
              <Match when={s().kind === "available"}>
                <p class="update-text">Une nouvelle version est disponible.</p>
                <button class="primary update-btn" title={`Installer yato ${version()} et relancer`} onClick={install}>
                  <IconDownload />
                  <span>Installer et relancer</span>
                </button>
              </Match>
              <Match when={s().kind === "downloading"}>
                <p class="update-text">Téléchargement… {pct() != null ? `${Math.round(pct()!)} %` : ""}</p>
                <div class="engine-bar">
                  <div style={{ width: `${pct() ?? 30}%` }} />
                </div>
              </Match>
              <Match when={s().kind === "installing"}>
                <p class="update-text">Installation, yato va redémarrer…</p>
              </Match>
            </Switch>
          </div>
        );
      }}
    </Show>
  );
}
