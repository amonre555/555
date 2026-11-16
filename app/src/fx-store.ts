// Réglages des effets (égaliseur, réverbération, vitesse), partagés entre le panneau de la barre
// du lecteur et la page Réglages. Chaque changement s'entend tout de suite et est enregistré
// avec les autres réglages.

import { createSignal } from "solid-js";
import { api, FLAT_FX, type FxSettings } from "./api";

const [fx, setSignal] = createSignal<FxSettings>(FLAT_FX);
export { fx };

let loaded = false;

/** Lit les réglages enregistrés (une seule fois). */
export function loadFx() {
  if (loaded) return;
  loaded = true;
  api
    .getSettings()
    .then((s) => setSignal({ ...FLAT_FX, ...s.fx }))
    .catch(() => (loaded = false));
}

let writing = false;
let dirty = false;

/** Applique et enregistre. Pendant qu'on fait glisser un curseur, les écritures se regroupent. */
export function setFx(next: FxSettings) {
  setSignal(next);
  dirty = true;
  if (!writing) void flush();
}

export const patchFx = (patch: Partial<FxSettings>) => setFx({ ...fx(), ...patch });

async function flush() {
  writing = true;
  try {
    while (dirty) {
      dirty = false;
      const settings = await api.getSettings();
      await api.setSettings({ ...settings, fx: fx() });
    }
  } catch {
    // Réglage non enregistré : il reste appliqué dans l'interface, on retentera au prochain.
  } finally {
    writing = false;
  }
}
