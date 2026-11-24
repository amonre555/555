// Réglages → Apparence → Thèmes : thèmes installés, import d'un .css, partage, et la galerie
// de la communauté (on essaie un thème en direct avant de le garder).

import { createResource, createSignal, For, Show } from "solid-js";
import { appearance } from "./appearance";
import { notify } from "./collection";
import { t } from "./i18n";
import { IconClose } from "./icons";
import {
  activeThemeId, fetchGallery, importThemeFile, installedThemes, installTheme, openGalleryPage, removeTheme,
  setActiveTheme, shareTheme, trying, tryTheme, type GalleryTheme,
} from "./themes";

export function ThemesPanel() {
  const [gallery, setGallery] = createSignal(false);
  let file: HTMLInputElement | undefined;

  const onFile = async (e: Event & { currentTarget: HTMLInputElement }) => {
    const f = e.currentTarget.files?.[0];
    e.currentTarget.value = "";
    if (!f) return;
    try {
      const theme = await importThemeFile(f);
      notify(t.themeInstalled(theme.name));
    } catch (err) {
      notify(String(err).includes("too-big") ? t.themeTooBig : t.themeImportFailed, "error");
    }
  };

  const share = async () => {
    const css = appearance().css.trim();
    if (!css) return notify(t.shareNeedsCss, "error");
    try {
      if ((await shareTheme(css)) === "copied") notify(t.shareCopied);
    } catch {
      notify(t.shareFailed, "error");
    }
  };

  return (
    <div class="themes">
      <div class="setting-text">
        <div class="setting-title">{t.themes}</div>
        <div class="setting-hint">{t.themesHint}</div>
      </div>
      <div class="theme-list" role="radiogroup" aria-label={t.themes}>
        <ThemeRow name={t.noTheme} on={activeThemeId() === null} onPick={() => setActiveTheme(null)} />
        <For each={installedThemes()}>
          {(th) => (
            <ThemeRow
              name={th.name}
              author={th.author}
              on={activeThemeId() === th.id}
              onPick={() => setActiveTheme(th.id)}
              onRemove={() => removeTheme(th.id)}
            />
          )}
        </For>
      </div>
      <div class="actions theme-actions">
        <button class="primary" onClick={() => setGallery(true)}>{t.browseThemes}</button>
        <button class="ghost" onClick={() => file?.click()}>{t.importTheme}</button>
        <button class="ghost" onClick={share} disabled={!appearance().css.trim()} title={t.shareThemeHint}>
          {t.shareTheme}
        </button>
        <input ref={file} type="file" accept=".css,text/css" hidden onChange={onFile} />
      </div>
      <Show when={gallery()}>
        <GalleryDialog onClose={() => setGallery(false)} />
      </Show>
      <Show when={trying()}>{(th) => <TryBar name={th().name} />}</Show>
    </div>
  );
}

function ThemeRow(props: { name: string; author?: string; on: boolean; onPick: () => void; onRemove?: () => void }) {
  return (
    <div classList={{ "theme-row": true, on: props.on }}>
      <button class="theme-pick" role="radio" aria-checked={props.on} onClick={props.onPick}>
        <span class="theme-dot" />
        <span class="theme-name">{props.name}</span>
        <Show when={props.author}>
          <span class="theme-author">{t.byAuthor(props.author!)}</span>
        </Show>
      </button>
      <Show when={props.onRemove}>
        <button class="icon-btn" title={t.removeTheme} aria-label={t.removeTheme} onClick={props.onRemove}>
          <IconClose />
        </button>
      </Show>
    </div>
  );
}

/** Galerie : thèmes postés dans les Discussions GitHub, les plus votés d'abord. */
function GalleryDialog(props: { onClose: () => void }) {
  const [themes] = createResource(fetchGallery);
  const installed = (g: GalleryTheme) => installedThemes().some((x) => x.galleryId === g.id);
  const asTheme = (g: GalleryTheme) => ({ id: `g-${g.id}`, name: g.name, author: g.author, description: g.description, css: g.css, galleryId: g.id });

  return (
    <div class="dialog-layer" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="dialog gallery" onKeyDown={(e) => e.key === "Escape" && props.onClose()}>
        <div class="dialog-head">
          <h2>{t.themeGallery}</h2>
          <button type="button" class="icon-btn" title={t.closeLabel} onClick={props.onClose}>
            <IconClose />
          </button>
        </div>
        <p class="muted small">{t.themeGalleryText}</p>
        <div class="gallery-list">
          <Show when={!themes.loading} fallback={<div class="muted small">{t.loading}</div>}>
            <Show when={!themes.error} fallback={<div class="muted small">{t.galleryUnavailable}</div>}>
              <Show when={themes()?.length} fallback={<div class="muted small">{t.galleryEmpty}</div>}>
                <For each={themes()}>
                  {(g) => (
                    <div class="gallery-item">
                      <div class="gallery-text">
                        <div class="gallery-name">{g.name}</div>
                        <div class="muted small">
                          {t.byAuthor(g.author)} · {t.votes(g.votes)}
                        </div>
                        <Show when={g.description}>
                          <div class="gallery-desc small">{g.description}</div>
                        </Show>
                      </div>
                      <div class="gallery-buttons">
                        <button
                          class="ghost"
                          onClick={() => {
                            tryTheme(asTheme(g));
                            props.onClose();
                          }}
                        >
                          {t.tryTheme}
                        </button>
                        <button class="primary" onClick={() => (installTheme(asTheme(g)), notify(t.themeInstalled(g.name)))}>
                          {installed(g) ? t.updateTheme : t.installTheme}
                        </button>
                      </div>
                    </div>
                  )}
                </For>
              </Show>
            </Show>
          </Show>
        </div>
        <div class="dialog-actions">
          <button class="ghost" onClick={() => openGalleryPage()}>{t.openGalleryWeb}</button>
        </div>
      </div>
    </div>
  );
}

/** Barre d'aperçu : le thème essayé est appliqué à toute l'appli tant qu'on ne choisit pas. */
function TryBar(props: { name: string }) {
  return (
    <div class="try-bar" role="status">
      <span>{t.tryingTheme(props.name)}</span>
      <div class="actions">
        <button class="ghost" onClick={() => tryTheme(null)}>{t.cancel}</button>
        <button
          class="primary"
          onClick={() => {
            const th = trying();
            if (th) installTheme(th);
          }}
        >
          {t.keepTheme}
        </button>
      </div>
    </div>
  );
}
