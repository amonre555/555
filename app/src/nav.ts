// Navigation partagée : n'importe quel composant peut ouvrir la fiche d'un artiste ou d'un
// album ; l'application (App.tsx) réagit à la demande.
import { createSignal } from "solid-js";
import type { AlbumRef, ArtistRef } from "./api";

export type Page = { kind: "artist"; artist: ArtistRef } | { kind: "album"; album: AlbumRef };

const [pageRequest, setPageRequest] = createSignal<{ page: Page; at: number } | null>(null);

export { pageRequest };

/** Ouvre une fiche (même si elle est déjà affichée : `at` force la mise à jour). */
function open(page: Page) {
  setPageRequest({ page, at: performance.now() });
}

export const openArtist = (artist: ArtistRef) => open({ kind: "artist", artist });
export const openAlbum = (album: AlbumRef) => open({ kind: "album", album });
