// Mises à jour automatiques : yato regarde au démarrage (puis toutes les 6 h) si une nouvelle
// version est publiée dans les Releases GitHub. Si oui, une carte le propose dans la barre
// latérale ; un clic télécharge la version (signée, vérifiée par l'extension), l'installe et
// relance l'app. Rien ne s'affiche s'il n'y a rien à faire ou si la vérification échoue
// (hors ligne, aperçu navigateur…).
//
// Android : pas de plugin updater. Le cœur lit la dernière Release et télécharge son APK, puis
// Android l'installe (src-tauri/src/android_update.rs). Une fenêtre bloquante remplace la carte :
// un toucher sur « Mettre à jour » et c'est fini.
import { Channel, invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { createSignal, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { notify } from "./collection";
import { IconDownload } from "./icons";
import { t } from "./i18n";
import { isAndroid } from "./mobile";

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
  if (isAndroid) return checkApk();
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

/** Vérifie au lancement puis toutes les 6 h. Échec silencieux : on réessaiera plus tard. */
function checkRegularly() {
  const lookup = () => void checkForUpdates();
  onMount(() => {
    const first = window.setTimeout(lookup, FIRST_CHECK_MS);
    const every = window.setInterval(lookup, EVERY_MS);
    onCleanup(() => {
      clearTimeout(first);
      clearInterval(every);
    });
  });
}

export function UpdateCard() {
  if (isAndroid) return null;
  checkRegularly();

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
      notify(t.updateFailed(String(e)), "error");
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
            <div class="update-title">555 {version()}</div>
            <Switch>
              <Match when={s().kind === "available"}>
                <p class="update-text">{t.newVersion}</p>
                <button class="primary update-btn" title={t.installTitle(version())} onClick={install}>
                  <IconDownload />
                  <span>{t.installAndRestart}</span>
                </button>
              </Match>
              <Match when={s().kind === "downloading"}>
                <p class="update-text">{t.downloading} {pct() != null ? `${Math.round(pct()!)} %` : ""}</p>
                <div class="engine-bar">
                  <div style={{ width: `${pct() ?? 30}%` }} />
                </div>
              </Match>
              <Match when={s().kind === "installing"}>
                <p class="update-text">{t.installing}</p>
              </Match>
            </Switch>
          </div>
        );
      }}
    </Show>
  );
}

// ---- Android ----

type ApkUpdate = { version: string; url: string; size: number };
type ApkState =
  | { kind: "available"; update: ApkUpdate; error?: string }
  | { kind: "downloading"; update: ApkUpdate; done: number; total: number }
  | { kind: "permission"; update: ApkUpdate };

const [apk, setApk] = createSignal<ApkState | null>(null);
// « Plus tard » n'apparaît qu'après un échec : on ne bloque pas 555 si l'installation est
// impossible (APK signé avec une autre clé, pas de place…).
const [postponed, setPostponed] = createSignal(false);

async function checkApk(): Promise<"available" | "none" | "error"> {
  if (apk()) {
    // Repoussée après un échec : la fenêtre revient (Réglages, ou 6 h plus tard).
    setPostponed(false);
    return "available";
  }
  try {
    const update = await invoke<ApkUpdate | null>("apk_update_check");
    if (!update) return "none";
    setPostponed(false);
    setApk({ kind: "available", update });
    return "available";
  } catch {
    return "error";
  }
}

async function installApk() {
  const s = apk();
  if (!s || s.kind === "downloading") return;
  const update = s.update;
  const onProgress = new Channel<{ done: number; total: number }>();
  onProgress.onmessage = ({ done, total }) => {
    if (apk()?.kind === "downloading") setApk({ kind: "downloading", update, done, total });
  };
  setApk({ kind: "downloading", update, done: 0, total: update.size });
  try {
    // Ne revient qu'en cas d'échec : réussie, la mise à jour remplace l'appli. Une fois l'APK
    // téléchargé (100 %), la fenêtre attend la confirmation d'Android.
    await invoke("apk_update_install", { update, onProgress });
    setApk({ kind: "available", update });
  } catch (e) {
    const msg = String(e);
    if (msg === "permission") setApk({ kind: "permission", update });
    else setApk({ kind: "available", update, error: msg === "cancelled" ? t.apkCancelled : t.updateFailed(msg) });
  }
}

/** Fenêtre bloquante de mise à jour, Android seulement. */
export function ApkUpdateDialog() {
  if (!isAndroid) return null;
  checkRegularly();

  // Retour du réglage « Installer des applis inconnues » : on reprend tout seul.
  const back = () => document.visibilityState === "visible" && apk()?.kind === "permission" && void installApk();
  document.addEventListener("visibilitychange", back);
  onCleanup(() => document.removeEventListener("visibilitychange", back));

  const pct = () => {
    const s = apk();
    return s?.kind === "downloading" && s.total ? Math.min(100, (s.done / s.total) * 100) : 0;
  };

  return (
    <Show when={!postponed() && apk()}>
      {(s) => (
        <div class="dialog-layer update-layer">
          <div class="dialog" role="alertdialog" aria-modal="true">
            <div class="dialog-head">
              <h2>{t.apkTitle(s().update.version)}</h2>
            </div>
            <Switch>
              <Match when={s().kind === "available"}>
                <p class="muted small">{(s() as { error?: string }).error ?? t.apkText}</p>
              </Match>
              <Match when={s().kind === "permission"}>
                <p class="muted small">{t.apkAllow}</p>
              </Match>
              <Match when={s().kind === "downloading" && pct() < 100}>
                <div class="dialog-progress">
                  <div class="engine-bar">
                    <div style={{ width: `${Math.max(pct(), 3)}%` }} />
                  </div>
                  <span class="muted small">
                    {t.downloading} {Math.round(pct())} %
                  </span>
                </div>
              </Match>
              <Match when={s().kind === "downloading"}>
                <p class="muted small">{t.apkConfirm}</p>
              </Match>
            </Switch>
            <Show when={s().kind === "available" || s().kind === "permission"}>
              <div class="dialog-actions">
                <Show when={s().kind === "available" && (s() as { error?: string }).error}>
                  <button class="ghost" onClick={() => setPostponed(true)}>
                    {t.later}
                  </button>
                </Show>
                <button class="primary" onClick={() => void installApk()}>
                  <IconDownload />
                  <span>
                    {s().kind === "permission" ? t.apkAllowed : (s() as { error?: string }).error ? t.retry : t.apkUpdate}
                  </span>
                </button>
              </div>
            </Show>
          </div>
        </div>
      )}
    </Show>
  );
}
