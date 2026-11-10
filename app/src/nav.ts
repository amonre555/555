// Navigation partagée : n'importe quel composant peut ouvrir la fiche d'un artiste ou d'un
// album ; l'application (App.tsx) réagit à la demande.
import { createSignal } from "solid-js";
import type { AlbumRef, ArtistRef } from "./api";

export type Page = { kind: "artist"; artist: ArtistRef } | { kind: "album"; album: AlbumRef };

/** `from` : image cliquée (pochette, photo), qui se transforme en celle de la fiche. */
const [pageRequest, setPageRequest] = createSignal<{ page: Page; from?: Element; at: number } | null>(null);

export { pageRequest };

/** Ouvre une fiche (même si elle est déjà affichée : `at` force la mise à jour). */
function open(page: Page, from?: Element) {
  setPageRequest({ page, from, at: performance.now() });
}

export const openArtist = (artist: ArtistRef, from?: Element) => open({ kind: "artist", artist }, from);
export const openAlbum = (album: AlbumRef, from?: Element) => open({ kind: "album", album }, from);
