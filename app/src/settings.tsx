// Page Réglages, et fenêtre d'import de playlist.
import { getVersion } from "@tauri-apps/api/app";
import { createSignal, onMount, Show, type JSX } from "solid-js";
import { api, type Settings } from "./api";
import { clearHistory, collection, importPlaylist, notify } from "./collection";
import { IconClose, IconLink } from "./icons";
import { checkForUpdates } from "./update";

/** Une ligne de réglage : intitulé, explication, et la commande à droite. */
function Row(props: { title: string; hint?: string; children: JSX.Element }) {
  return (
    <div class="setting">
      <div class="setting-text">
        <div class="setting-title">{props.title}</div>
        <Show when={props.hint}>
          <div class="setting-hint">{props.hint}</div>
        </Show>
      </div>
      <div class="setting-control">{props.children}</div>
    </div>
  );
}

function Toggle(props: { on: boolean; label: string; onChange: (on: boolean) => void }) {
  return (
    <button class={`switch ${props.on ? "on" : ""}`} role="switch" aria-checked={props.on} aria-label={props.label} onClick={() => props.onChange(!props.on)}>
      <span class="switch-thumb" />
    </button>
  );
}

export function SettingsView(props: { onImport: () => void; onLibrary: () => void }) {
  const [settings, setSettings] = createSignal<Settings | null>(null);
  const [version, setVersion] = createSignal("");
  const [checking, setChecking] = createSignal(false);

  onMount(async () => {
    try {
      setSettings(await api.getSettings());
    } catch (e) {
      notify(String(e), "error");
    }
    getVersion().then(setVersion).catch(() => {});
  });

  /** Change un réglage : appliqué et enregistré tout de suite. */
  const change = async (patch: Partial<Settings>) => {
    const cur = settings();
    if (!cur) return;
    const next = { ...cur, ...patch };
    setSettings(next);
    try {
      setSettings(await api.setSettings(next));
    } catch (e) {
      setSettings(cur);
      notify(String(e), "error");
    }
  };

  const lookForUpdate = async () => {
    setChecking(true);
    const r = await checkForUpdates();
    setChecking(false);
    if (r === "none") notify("yato est à jour");
    else if (r === "error") notify("Impossible de vérifier les mises à jour (connexion ?)", "error");
    else notify("Une nouvelle version est disponible : voir en bas de la barre latérale");
  };

  const crossfadeLabel = (s: number) => (s === 0 ? "Désactivé" : `${s} s`);

  return (
    <div class="settings-page">
      <div class="page-head">
        <h1>Réglages</h1>
      </div>
      <Show when={settings()}>
        {(s) => (
          <>
            <section class="settings-group">
              <h2 class="section-title">Lecture</h2>
              <Row title="Fondu enchaîné" hint="Le morceau suivant commence pendant que celui-ci se termine.">
                <div class="setting-range">
                  <input
                    type="range"
                    class="slider"
                    min="0"
                    max="12"
                    step="1"
                    value={s().crossfade}
                    style={{ "--pct": `${(s().crossfade / 12) * 100}%` }}
                    aria-label="Durée du fondu enchaîné"
                    onInput={(e) => setSettings({ ...s(), crossfade: Number(e.currentTarget.value) })}
                    onChange={(e) => change({ crossfade: Number(e.currentTarget.value) })}
                  />
                  <span class="setting-value">{crossfadeLabel(s().crossfade)}</span>
                </div>
              </Row>
              <Row title="Reprendre l'écoute au lancement" hint="Retrouver le dernier morceau, la file d'attente et la position.">
                <Toggle on={s().resume} label="Reprendre l'écoute au lancement" onChange={(on) => change({ resume: on })} />
              </Row>
            </section>

            <section class="settings-group">
              <h2 class="section-title">Bibliothèque</h2>
              <Row title="Importer une playlist" hint="Depuis un lien Spotify, Deezer ou YouTube Music.">
                <button class="ghost" onClick={props.onImport}>
                  <IconLink /> Importer
                </button>
              </Row>
              <Row title="Dossiers de musique" hint="Les dossiers de ta musique locale se gèrent dans la Bibliothèque.">
                <button class="ghost" onClick={props.onLibrary}>Ouvrir la Bibliothèque</button>
              </Row>
              <Row
                title="Historique d'écoute"
                hint={`${collection().history.length} morceau${collection().history.length > 1 ? "x" : ""} dans « Écoutés récemment ».`}
              >
                <button class="ghost" onClick={clearHistory} disabled={collection().history.length === 0}>
                  Effacer
                </button>
              </Row>
            </section>

            <section class="settings-group">
              <h2 class="section-title">Discord</h2>
              <Row title="Afficher ce que j'écoute" hint="« Écoute … » dans ton statut Discord, avec la pochette et la progression.">
                <Toggle on={s().discord} label="Statut Discord" onChange={(on) => change({ discord: on })} />
              </Row>
            </section>

            <section class="settings-group">
              <h2 class="section-title">yato</h2>
              <Row title={`Version ${version() || "–"}`} hint="Les mises à jour sont vérifiées au lancement, puis toutes les 6 h.">
                <button class="ghost" onClick={lookForUpdate} disabled={checking()}>
                  {checking() ? "Vérification…" : "Rechercher une mise à jour"}
                </button>
              </Row>
            </section>
          </>
        )}
      </Show>
    </div>
  );
}

/** Fenêtre d'import : un lien, puis la recherche des morceaux, puis la playlist s'ouvre. */
export function ImportDialog(props: { onClose: () => void; onDone: (id: string) => void }) {
  const [url, setUrl] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [progress, setProgress] = createSignal<[number, number] | null>(null);
  let input: HTMLInputElement | undefined;
  onMount(() => requestAnimationFrame(() => input?.focus()));

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!url().trim() || busy()) return;
    setBusy(true);
    setProgress(null);
    const id = await importPlaylist(url().trim(), (done, total) => setProgress([done, total]));
    setBusy(false);
    if (id) props.onDone(id);
  };

  return (
    <div class="dialog-layer" onMouseDown={(e) => e.target === e.currentTarget && !busy() && props.onClose()}>
      <form class="dialog" onSubmit={submit} onKeyDown={(e) => e.key === "Escape" && !busy() && props.onClose()}>
        <div class="dialog-head">
          <h2>Importer une playlist</h2>
          <button type="button" class="icon-btn" title="Fermer" onClick={props.onClose} disabled={busy()}>
            <IconClose />
          </button>
        </div>
        <p class="muted small">Colle le lien d'une playlist publique Spotify, Deezer ou YouTube Music (ou d'un album Spotify).</p>
        <input
          ref={input}
          class="dialog-input"
          type="text"
          inputMode="url"
          placeholder="https://open.spotify.com/playlist/…"
          value={url()}
          onInput={(e) => setUrl(e.currentTarget.value)}
          disabled={busy()}
          spellcheck={false}
        />
        <Show when={busy()}>
          <div class="dialog-progress">
            <div class="engine-bar">
              <div style={{ width: progress() && progress()![1] ? `${(progress()![0] / progress()![1]) * 100}%` : "15%" }} />
            </div>
            <span class="muted small">
              {progress() ? `Recherche des morceaux… ${progress()![0]} / ${progress()![1]}` : "Lecture de la playlist…"}
            </span>
          </div>
        </Show>
        <div class="dialog-actions">
          <button type="button" class="ghost" onClick={props.onClose} disabled={busy()}>
            Annuler
          </button>
          <button type="submit" class="primary" disabled={busy() || !url().trim()}>
            {busy() ? "Import…" : "Importer"}
          </button>
        </div>
      </form>
    </div>
  );
}
