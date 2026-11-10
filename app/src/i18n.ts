// Interface language. English by default; the choice is kept in the app's local storage.
// `t.key` is read inside JSX, so every text updates as soon as the language changes.
import { createSignal } from "solid-js";

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
const pluralFr = (n: number, one: string, many: string) => `${n} ${n > 1 ? many : one}`;

const en = {
  // Navigation
  home: "Home",
  discover: "Discover",
  library: "Library",
  liked: "Liked songs",
  history: "History",
  playlists: "Playlists",
  newPlaylist: "New playlist",
  settings: "Settings",
  importPlaylistHint: "Import a playlist (Spotify, Deezer, YouTube Music)",
  searchPlaceholder: "Songs, artists, albums…",
  back: "Back",

  // Search
  noResults: (q: string) => `No results for “${q}”.`,
  topResult: "Top result",
  songs: "Songs",
  moreSongs: "More songs",
  moreFrom: (source: string) => `More from ${source}`,

  // Library
  yourMusic: "Your music",
  tracksCount: (n: number) => plural(n, "track", "tracks"),
  playAll: "Play all",
  rescan: "Rescan",
  removeFolder: "Remove this folder",
  scanning: "Scanning…",
  addFolder: "Add a folder",
  chooseFolder: "Choose a music folder",
  libraryEmptyTitle: "Your music, instantly",
  libraryEmptyText: "Add a folder: yato reads the tags on every core of your processor.",
  myLibrary: "My library",

  // Discover
  featuredError: "Couldn't load the current charts.",
  numberOne: "#1 right now",
  recentlyPlayed: "Recently played",
  seeAll: "See all",
  rightNow: "Right now",
  fullChart: "Full chart",
  nowPlaying: "Now playing",
  listen: "Listen",

  // Track lists
  loading: "Loading",
  play: "Play",
  moreOptions: "More options",
  seeArtist: (name: string) => `Go to ${name}`,
  seeAlbum: (title: string) => `Go to album “${title}”`,
  playAlbum: (title: string) => `Play “${title}”`,
  addToLiked: "Add to Liked songs",
  removeFromLiked: "Remove from Liked songs",
  addedToLiked: "Added to Liked songs",
  removedFromLiked: "Removed from Liked songs",
  removeFromThisPlaylist: "Remove from this playlist",
  addToPlaylist: "Add to playlist",
  addedTo: (name: string) => `Added to “${name}”`,
  addedManyTo: (n: number, name: string) => `${n} songs added to “${name}”`,
  alreadyIn: (name: string) => `Already in “${name}”`,
  ytInstalling: (pct: number) => `Installing the YouTube engine… ${pct}%`,
  ytInstallingHint: "yt-dlp is downloaded from its official releases, and its signature is verified.",
  ytError: (msg: string) => `YouTube unavailable: ${msg}`,

  // Artist & album
  artist: "Artist",
  artistError: (name: string) => `Couldn't load ${name}.`,
  popular: "Popular",
  showLess: "Show less",
  showMore: "Show more",
  albums: "Albums",
  singlesEps: "Singles and EPs",
  similarArtists: "Similar artists",
  albumError: (title: string) => `Couldn't load the album “${title}”.`,
  addAlbumToPlaylist: "Add the album to a playlist",
  kind: { album: "Album", single: "Single", ep: "EP", compile: "Compilation" } as Record<string, string>,
  songsCount: (n: number) => plural(n, "song", "songs"),
  hours: (h: number, m: string) => `${h} hr ${m} min`,
  minutes: (m: number) => `${m} min`,

  // Playlists
  playlist: "Playlist",
  playlistGone: "This playlist no longer exists.",
  rename: "Rename",
  playlistName: "Playlist name",
  addWholePlaylist: "Add the whole playlist to another one",
  deletePlaylist: "Delete playlist",
  confirmDelete: "Confirm deletion",
  emptyPlaylist: "Empty playlist",
  emptyPlaylistText: "Right-click or “⋯” on a song, then “Add to playlist”.",
  noLiked: "No liked songs yet",
  noLikedText: "Tap the heart next to a song to find it here.",
  defaultPlaylist: (n: number) => `My playlist #${n}`,

  // History
  mostPlayed: "Most played",
  historyEmpty: "Nothing played yet",
  historyEmptyText: "Songs you listen to for more than 20 seconds show up here.",
  plays: (n: number) => plural(n, "play", "plays"),
  today: "Today",
  yesterday: "Yesterday",
  clearHistory: "Clear history",
  historyCleared: "History cleared",

  // Player
  nothingPlaying: "Nothing playing",
  openNowPlaying: "Open the Now playing view",
  lyricsAndNowPlaying: "Lyrics and Now playing",
  repeatAll: "Repeat queue",
  repeatOne: "Repeat this song",
  repeatOff: "Stop repeating",
  shuffleOff: "Turn off shuffle",
  shuffle: "Shuffle",
  previous: "Previous (Alt+←)",
  playPause: "Play / pause (Space)",
  next: "Next (Alt+→)",
  seekLabel: "Position in the song",
  volume: "Volume",
  close: "Close (Esc)",
  searchingLyrics: "Looking for lyrics…",
  instrumental: "♪ Instrumental",
  noLyrics: "No lyrics for this song",
  lyricsEarly: "Lyrics come too early: delay them by 0.1 s",
  lyricsLate: "Lyrics come too late: advance them by 0.1 s",
  lyricsReset: "Reset to the original sync",
  lyricsSync: "Lyrics sync",
  sync: "Sync",
  decimal: ".",

  // Settings
  playback: "Playback",
  crossfade: "Crossfade",
  crossfadeHint: "The next song starts while this one ends.",
  crossfadeLabel: "Crossfade length",
  off: "Off",
  resume: "Resume playback on launch",
  resumeHint: "Bring back the last song, the queue and the position.",
  importPlaylist: "Import a playlist",
  importHint: "From a Spotify, Deezer or YouTube Music link.",
  import: "Import",
  musicFolders: "Music folders",
  musicFoldersHint: "Your local music folders are managed in the Library.",
  openLibrary: "Open the Library",
  listeningHistory: "Listening history",
  historyHint: (n: number) => `${plural(n, "play", "plays")} in your History.`,
  openHistory: "Open",
  clear: "Clear",
  showListening: "Show what I'm listening to",
  discordHint: "“Listening to …” in your Discord status, with the cover and progress.",
  discordStatus: "Discord status",
  version: (v: string) => `Version ${v}`,
  updatesHint: "Updates are checked on launch, then every 6 hours.",
  checking: "Checking…",
  checkUpdates: "Check for updates",
  upToDate: "yato is up to date",
  updateCheckFailed: "Couldn't check for updates (connection?)",
  updateFound: "A new version is available: see the bottom of the sidebar",
  general: "General",
  language: "Language",
  languageHint: "Language of the interface.",

  // Import dialog
  importText: "Paste the link of a public Spotify, Deezer or YouTube Music playlist (or a Spotify album).",
  closeLabel: "Close",
  findingTracks: (done: number, total: number) => `Finding songs… ${done} / ${total}`,
  readingPlaylist: "Reading the playlist…",
  cancel: "Cancel",
  importing: "Importing…",
  imported: (name: string, added: number, missing: number) =>
    `“${name}” imported: ${plural(added, "song", "songs")}${missing ? `, ${missing} not found` : ""}`,
  thePlaylist: "the playlist",

  // Updates
  newVersion: "A new version is available.",
  installAndRestart: "Install and restart",
  installTitle: (v: string) => `Install yato ${v} and restart`,
  downloading: "Downloading…",
  installing: "Installing, yato will restart…",
  updateFailed: (e: string) => `Update failed: ${e}`,
};

type Dict = typeof en;

const fr: Dict = {
  home: "Accueil",
  discover: "Découvrir",
  library: "Bibliothèque",
  liked: "Titres likés",
  history: "Historique",
  playlists: "Playlists",
  newPlaylist: "Nouvelle playlist",
  settings: "Réglages",
  importPlaylistHint: "Importer une playlist (Spotify, Deezer, YouTube Music)",
  searchPlaceholder: "Titres, artistes, albums…",
  back: "Retour",

  noResults: (q) => `Aucun résultat pour « ${q} ».`,
  topResult: "Meilleur résultat",
  songs: "Titres",
  moreSongs: "Plus de titres",
  moreFrom: (source) => `Plus de résultats ${source}`,

  yourMusic: "Ta musique",
  tracksCount: (n) => pluralFr(n, "morceau", "morceaux"),
  playAll: "Tout lire",
  rescan: "Rescanner",
  removeFolder: "Retirer ce dossier",
  scanning: "Analyse en cours…",
  addFolder: "Ajouter un dossier",
  chooseFolder: "Choisir un dossier de musique",
  libraryEmptyTitle: "Ta musique, sans attendre",
  libraryEmptyText: "Ajoute un dossier : yato lit les tags sur tous les cœurs de ton processeur.",
  myLibrary: "Ma bibliothèque",

  featuredError: "Impossible de charger le top du moment.",
  numberOne: "N°1 du moment",
  recentlyPlayed: "Écoutés récemment",
  seeAll: "Tout voir",
  rightNow: "En ce moment",
  fullChart: "Tout le top",
  nowPlaying: "En lecture",
  listen: "Écouter",

  loading: "Chargement",
  play: "Lire",
  moreOptions: "Plus d'options",
  seeArtist: (name) => `Voir la fiche de ${name}`,
  seeAlbum: (title) => `Voir l'album « ${title} »`,
  playAlbum: (title) => `Lire « ${title} »`,
  addToLiked: "Ajouter aux titres likés",
  removeFromLiked: "Retirer des titres likés",
  addedToLiked: "Ajouté aux titres likés",
  removedFromLiked: "Retiré des titres likés",
  removeFromThisPlaylist: "Retirer de cette playlist",
  addToPlaylist: "Ajouter à une playlist",
  addedTo: (name) => `Ajouté à « ${name} »`,
  addedManyTo: (n, name) => `${n} titres ajoutés à « ${name} »`,
  alreadyIn: (name) => `Déjà dans « ${name} »`,
  ytInstalling: (pct) => `Installation du moteur YouTube… ${pct} %`,
  ytInstallingHint: "yt-dlp est téléchargé depuis ses versions officielles, et sa signature est vérifiée.",
  ytError: (msg) => `YouTube indisponible : ${msg}`,

  artist: "Artiste",
  artistError: (name) => `Impossible de charger la fiche de ${name}.`,
  popular: "Titres populaires",
  showLess: "Afficher moins",
  showMore: "Afficher plus",
  albums: "Albums",
  singlesEps: "Singles et EP",
  similarArtists: "Artistes similaires",
  albumError: (title) => `Impossible de charger l'album « ${title} ».`,
  addAlbumToPlaylist: "Ajouter l'album à une playlist",
  kind: { album: "Album", single: "Single", ep: "EP", compile: "Compilation" },
  songsCount: (n) => pluralFr(n, "titre", "titres"),
  hours: (h, m) => `${h} h ${m} min`,
  minutes: (m) => `${m} min`,

  playlist: "Playlist",
  playlistGone: "Cette playlist n'existe plus.",
  rename: "Renommer",
  playlistName: "Nom de la playlist",
  addWholePlaylist: "Ajouter toute la playlist à une autre",
  deletePlaylist: "Supprimer la playlist",
  confirmDelete: "Confirmer la suppression",
  emptyPlaylist: "Playlist vide",
  emptyPlaylistText: "Clic droit ou « ⋯ » sur un morceau, puis « Ajouter à une playlist ».",
  noLiked: "Aucun titre liké pour l'instant",
  noLikedText: "Touche le cœur à côté d'un morceau pour le retrouver ici.",
  defaultPlaylist: (n) => `Ma playlist n° ${n}`,

  mostPlayed: "Les plus écoutés",
  historyEmpty: "Rien d'écouté pour l'instant",
  historyEmptyText: "Les morceaux écoutés plus de 20 secondes apparaissent ici.",
  plays: (n) => pluralFr(n, "écoute", "écoutes"),
  today: "Aujourd'hui",
  yesterday: "Hier",
  clearHistory: "Effacer l'historique",
  historyCleared: "Historique effacé",

  nothingPlaying: "Rien en lecture",
  openNowPlaying: "Ouvrir la vue En lecture",
  lyricsAndNowPlaying: "Paroles et vue En lecture",
  repeatAll: "Répéter la file",
  repeatOne: "Répéter ce morceau",
  repeatOff: "Ne plus répéter",
  shuffleOff: "Désactiver la lecture aléatoire",
  shuffle: "Lecture aléatoire",
  previous: "Précédent (Alt+←)",
  playPause: "Lecture / pause (Espace)",
  next: "Suivant (Alt+→)",
  seekLabel: "Position dans le morceau",
  volume: "Volume",
  close: "Fermer (Échap)",
  searchingLyrics: "Recherche des paroles…",
  instrumental: "♪ Instrumental",
  noLyrics: "Pas de paroles pour ce morceau",
  lyricsEarly: "Les paroles arrivent trop tôt : les retarder de 0,1 s",
  lyricsLate: "Les paroles arrivent trop tard : les avancer de 0,1 s",
  lyricsReset: "Remettre la synchronisation d'origine",
  lyricsSync: "Synchronisation des paroles",
  sync: "Synchro",
  decimal: ",",

  playback: "Lecture",
  crossfade: "Fondu enchaîné",
  crossfadeHint: "Le morceau suivant commence pendant que celui-ci se termine.",
  crossfadeLabel: "Durée du fondu enchaîné",
  off: "Désactivé",
  resume: "Reprendre l'écoute au lancement",
  resumeHint: "Retrouver le dernier morceau, la file d'attente et la position.",
  importPlaylist: "Importer une playlist",
  importHint: "Depuis un lien Spotify, Deezer ou YouTube Music.",
  import: "Importer",
  musicFolders: "Dossiers de musique",
  musicFoldersHint: "Les dossiers de ta musique locale se gèrent dans la Bibliothèque.",
  openLibrary: "Ouvrir la Bibliothèque",
  listeningHistory: "Historique d'écoute",
  historyHint: (n) => `${pluralFr(n, "écoute", "écoutes")} dans l'historique.`,
  openHistory: "Ouvrir",
  clear: "Effacer",
  showListening: "Afficher ce que j'écoute",
  discordHint: "« Écoute … » dans ton statut Discord, avec la pochette et la progression.",
  discordStatus: "Statut Discord",
  version: (v) => `Version ${v}`,
  updatesHint: "Les mises à jour sont vérifiées au lancement, puis toutes les 6 h.",
  checking: "Vérification…",
  checkUpdates: "Rechercher une mise à jour",
  upToDate: "yato est à jour",
  updateCheckFailed: "Impossible de vérifier les mises à jour (connexion ?)",
  updateFound: "Une nouvelle version est disponible : voir en bas de la barre latérale",
  general: "Général",
  language: "Langue",
  languageHint: "Langue de l'interface.",

  importText: "Colle le lien d'une playlist publique Spotify, Deezer ou YouTube Music (ou d'un album Spotify).",
  closeLabel: "Fermer",
  findingTracks: (done, total) => `Recherche des morceaux… ${done} / ${total}`,
  readingPlaylist: "Lecture de la playlist…",
  cancel: "Annuler",
  importing: "Import…",
  imported: (name, added, missing) =>
    `« ${name} » importée : ${pluralFr(added, "titre", "titres")}${missing ? `, ${pluralFr(missing, "introuvable", "introuvables")}` : ""}`,
  thePlaylist: "la playlist",

  newVersion: "Une nouvelle version est disponible.",
  installAndRestart: "Installer et relancer",
  installTitle: (v) => `Installer yato ${v} et relancer`,
  downloading: "Téléchargement…",
  installing: "Installation, yato va redémarrer…",
  updateFailed: (e) => `Mise à jour impossible : ${e}`,
};

export const LANGUAGES = [
  { id: "en", name: "English" },
  { id: "fr", name: "Français" },
] as const;
export type Lang = (typeof LANGUAGES)[number]["id"];

const DICTS: Record<Lang, Dict> = { en, fr };
const KEY = "yato.lang";

function initial(): Lang {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved && saved in DICTS) return saved as Lang;
  } catch {
    // Storage unavailable: default language.
  }
  return "en";
}

const [lang, setLangSignal] = createSignal<Lang>(initial());
export { lang };

export function setLang(l: Lang) {
  setLangSignal(l);
  document.documentElement.lang = l;
  try {
    localStorage.setItem(KEY, l);
  } catch {
    // The choice holds for this session only.
  }
}
document.documentElement.lang = lang();

/** Texts in the current language. Reactive: read `t.key` inside JSX or an effect. */
export const t: Dict = new Proxy({} as Dict, {
  get: (_, key: string) => DICTS[lang()][key as keyof Dict],
});

/** BCP 47 locale for dates. */
export const locale = () => (lang() === "fr" ? "fr-FR" : "en-US");
