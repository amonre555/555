// Navigation partagée : n'importe quel composant peut ouvrir la fiche d'un artiste,
// l'application (App.tsx) réagit à la demande.
import { createSignal } from "solid-js";
import type { ArtistRef } from "./api";

const [artistRequest, setArtistRequest] = createSignal<{ artist: ArtistRef; at: number } | null>(null);

export { artistRequest };

/** Ouvre la fiche d'un artiste (même s'il est déjà affiché : `at` force la mise à jour). */
export function openArtist(artist: ArtistRef) {
  setArtistRequest({ artist, at: performance.now() });
}
