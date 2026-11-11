// Page Réglages, et fenêtre d'import de playlist.
import { getVersion } from "@tauri-apps/api/app";
import { createSignal, For, onMount, Show, type JSX } from "solid-js";
import { api, type Settings } from "./api";
import { clearHistory, collection, importPlaylist, notify } from "./collection";
import { IconClose, IconLink } from "./icons";
import { checkForUpdates } from "./update";
import { isAndroid } from "./mobile";
import { lang, LANGUAGES, setLang, t, type Lang } from "./i18n";

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

export function SettingsView(props: { onImport: () => void; onLibrary: () => void; onHistory: () => void }) {
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
    if (r === "none") notify(t.upToDate);
    else if (r === "error") notify(t.updateCheckFailed, "error");
    else notify(t.updateFound);
  };

  const crossfadeLabel = (s: number) => (s === 0 ? t.off : `${s} s`);

  return (
    <div class="settings-page">
      <div class="page-head">
        <h1>{t.settings}</h1>
      </div>
      <Show when={settings()}>
        {(s) => (
          <>
            <section class="settings-group">
              <h2 class="section-title">{t.general}</h2>
              <Row title={t.language} hint={t.languageHint}>
                <div class="segmented" role="radiogroup" aria-label={t.language}>
                  <For each={LANGUAGES}>
                    {(l) => (
                      <button
                        role="radio"
                        aria-checked={lang() === l.id}
                        classList={{ on: lang() === l.id }}
                        onClick={() => setLang(l.id as Lang)}
                      >
                        {l.name}
                      </button>
                    )}
                  </For>
                </div>
              </Row>
            </section>

            <section class="settings-group">
              <h2 class="section-title">{t.playback}</h2>
              <Row title={t.crossfade} hint={t.crossfadeHint}>
                <div class="setting-range">
                  <input
                    type="range"
                    class="slider"
                    min="0"
                    max="12"
                    step="1"
                    value={s().crossfade}
                    style={{ "--pct": `${(s().crossfade / 12) * 100}%` }}
                    aria-label={t.crossfadeLabel}
                    onInput={(e) => setSettings({ ...s(), crossfade: Number(e.currentTarget.value) })}
                    onChange={(e) => change({ crossfade: Number(e.currentTarget.value) })}
                  />
                  <span class="setting-value">{crossfadeLabel(s().crossfade)}</span>
                </div>
              </Row>
              <Row title={t.resume} hint={t.resumeHint}>
                <Toggle on={s().resume} label={t.resume} onChange={(on) => change({ resume: on })} />
              </Row>
            </section>

            <section class="settings-group">
              <h2 class="section-title">{t.library}</h2>
              <Row title={t.importPlaylist} hint={t.importHint}>
                <button class="ghost" onClick={props.onImport}>
                  <IconLink /> {t.import}
                </button>
              </Row>
              <Row title={t.musicFolders} hint={t.musicFoldersHint}>
                <button class="ghost" onClick={props.onLibrary}>{t.openLibrary}</button>
              </Row>
              <Row title={t.listeningHistory} hint={t.historyHint(collection().history.length)}>
                <div class="actions">
                  <button class="ghost" onClick={props.onHistory}>{t.openHistory}</button>
                  <button class="ghost" onClick={clearHistory} disabled={collection().history.length === 0}>
                    {t.clear}
                  </button>
                </div>
              </Row>
            </section>

            {/* Android : ni Discord ni mises à jour intégrées (l'APK s'installe à la main). */}
            <Show when={!isAndroid}>
              <section class="settings-group">
                <h2 class="section-title">Discord</h2>
                <Row title={t.showListening} hint={t.discordHint}>
                  <Toggle on={s().discord} label={t.discordStatus} onChange={(on) => change({ discord: on })} />
                </Row>
              </section>
            </Show>

            <section class="settings-group">
              <h2 class="section-title">555</h2>
              <Row title={t.version(version() || "-")} hint={isAndroid ? "" : t.updatesHint}>
                <Show when={!isAndroid}>
                  <button class="ghost" onClick={lookForUpdate} disabled={checking()}>
                    {checking() ? t.checking : t.checkUpdates}
                  </button>
                </Show>
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
  let input: HTMLTextAreaElement | undefined;
  onMount(() => requestAnimationFrame(() => input?.focus()));
  /** Plusieurs liens collés (morceaux Spotify) : le champ s'agrandit et dit combien. */
  const songLinks = () => (url().match(/(\/track\/|spotify:track:)[A-Za-z0-9]{22}/g) ?? []).length;

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
          <h2>{t.importPlaylist}</h2>
          <button type="button" class="icon-btn" title={t.closeLabel} onClick={props.onClose} disabled={busy()}>
            <IconClose />
          </button>
        </div>
        <p class="muted small">{t.importText}</p>
        <textarea
          ref={input}
          class="dialog-input"
          classList={{ multi: songLinks() > 1 }}
          rows={1}
          inputMode="url"
          placeholder="https://open.spotify.com/playlist/…"
          value={url()}
          onInput={(e) => setUrl(e.currentTarget.value)}
          onKeyDown={(e) => {
            // Entrée valide, comme dans un champ d'une ligne.
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              e.currentTarget.form?.requestSubmit();
            }
          }}
          disabled={busy()}
          spellcheck={false}
        />
        <Show when={songLinks() > 0 && !busy()}>
          <span class="muted small">{t.songLinks(songLinks())}</span>
        </Show>
        <Show when={busy()}>
          <div class="dialog-progress">
            <div class="engine-bar">
              <div style={{ width: progress() && progress()![1] ? `${(progress()![0] / progress()![1]) * 100}%` : "15%" }} />
            </div>
            <span class="muted small">
              {progress() ? t.findingTracks(progress()![0], progress()![1]) : t.readingPlaylist}
            </span>
          </div>
        </Show>
        <div class="dialog-actions">
          <button type="button" class="ghost" onClick={props.onClose} disabled={busy()}>
            {t.cancel}
          </button>
          <button type="submit" class="primary" disabled={busy() || !url().trim()}>
            {busy() ? t.importing : t.import}
          </button>
        </div>
      </form>
    </div>
  );
}
