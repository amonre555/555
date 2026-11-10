# yato

Lecteur de musique léger et rapide, écrit en Rust avec Tauri.

- **Ta bibliothèque locale** : MP3, FLAC, OGG, M4A/AAC, WAV, AIFF… scan en parallèle sur tous les cœurs, cache sur disque, recherche instantanée.
- **Une source pour les infos, une source pour le son** (comme Nuclear) :
  - **Deezer** (API publique, sans clé) : recherche, top du moment, titres et albums propres, pochettes en 1000 × 1000.
  - **YouTube Music** : au moment de jouer, yato cherche le même enregistrement (titre, artiste, durée, en écartant les versions live, remix, accélérées…) via [rustypipe](https://codeberg.org/ThetaDev/rustypipe), et le son arrive par [yt-dlp](https://github.com/yt-dlp/yt-dlp). La recherche YouTube reste disponible pour les versions absentes de Deezer.
- **Streaming progressif** : le son démarre pendant le téléchargement, et l'avance rapide est immédiate.
- **Anticipation** : le morceau suivant, le premier résultat d'une recherche et les morceaux survolés sont téléchargés à l'avance en mémoire (8 morceaux maximum). Au clic, le son démarre instantanément. Sinon, il faut environ 2 s.
- **yt-dlp géré automatiquement** : téléchargé depuis les versions officielles, avec vérification de la signature OpenPGP de yt-dlp et de l'empreinte SHA-256, puis mis à jour toutes les 12 h sans coupure. On utilise la version « dossier », qui démarre en 0,5 s au lieu de 2 s.

- **Fiches d'artistes** : photo, titres populaires, albums, singles et EP, artistes similaires. Chaque artiste d'un morceau (invités compris) est cliquable séparément.
- **Titres likés et playlists** : un cœur sur chaque morceau (listes, lecteur, vue « En lecture »), et des playlists qui mélangent bibliothèque locale, Deezer et YouTube. Clic droit ou « ⋯ » sur un morceau pour l'ajouter, glisser-déposer pour réordonner, clic sur le nom pour renommer. Tout est gardé dans `collection.json`, dans les données de l'app.
- **Fiches d'albums** : pochette, artistes, année, durée, liste des morceaux et maison de disques. Le nom d'un album est cliquable partout où il apparaît.
- **Statut Discord** : « Écoute *titre* », l'artiste, la pochette et la barre de progression. Très peu d'envois (seulement aux changements), et rien ne se passe si Discord est fermé.
- **Paroles synchronisées au mot près** : NetEase Cloud Music (format YRC) pour le minutage de chaque mot, [LRCLIB](https://lrclib.net) en parallèle (lignes, ou mots en Enhanced LRC). Sans minutage au mot, il est estimé d'après la longueur des mots, sans s'étaler sur les pauses. Un clic sur une ligne y amène la lecture.
- **Touches multimédia et panneau multimédia du système** (Windows, MPRIS sous Linux, macOS), avec titre, artiste et pochette.
- **Interface Material 3 Expressive** : toute l'interface prend les couleurs de la pochette en cours (Material You, via [material-color-utilities](https://github.com/material-foundation/material-color-utilities)), police Google Sans Flex, formes « cookie », boutons qui changent de forme, barre de progression ondulée, transitions floues.
- **Vue « En lecture »** : la pochette s'envole depuis le lecteur en changeant de forme, paroles façon karaoké : chaque mot se remplit de blanc au moment où il est chanté.

## Structure

```
crates/yato-core/   Cœur Rust, indépendant de l'interface
  src/source.rs        Trait MusicSource : ajouter une source = l'implémenter
  src/sources/         Sources fournies (local, deezer, youtube)
  src/stream.rs        Lecteur HTTP progressif
  src/player.rs        Moteur audio (rodio + symphonia) et file d'attente
  src/collection.rs    Titres likés et playlists (collection.json)
app/                 Interface (SolidJS + Vite)
app/src-tauri/       Pont Tauri : commandes, événements, protocole cover://
```

Le cœur ne connaît pas Tauri : il pourra servir à une version mobile (Tauri v2) ou à une interface native (GPUI) sans être réécrit.

## Lancer

Prérequis : Rust stable, Node.js 22+, outils de build MSVC (Windows). Rien d'autre : yt-dlp s'installe tout seul au premier lancement (dans `<données de l'app>/bin/yt-dlp`). Si l'installation échoue (hors ligne…), yato utilise un yt-dlp déjà présent sur le système (PATH ou winget).

```bash
cd app
npm install
npm run tauri dev
```

Version optimisée : `npm run tauri build`.

Sous Windows, si le dossier du projet est très profond, l'éditeur de liens peut dépasser la limite de 260 caractères. Il suffit alors de compiler ailleurs, par exemple `set CARGO_TARGET_DIR=C:\yato-target`.

Pour travailler l'interface dans un navigateur, sans Tauri : `npm run dev` puis http://localhost:1420. Le cœur est alors simulé (morceaux et paroles d'exemple), voir `app/src/dev-mock.ts`. Ce mode n'est jamais inclus dans l'app.

Tests rapides, sans interface :

```bash
cargo run -p yato-core --example yt -- "vhs mattyeux"
cargo run -p yato-core --example deezer -- "stromae alors on danse"   # recherche Deezer + lecture du son YouTube
cargo run -p yato-core --example artist -- "sweat snoop dogg"   # crédits complets, fiches d'artistes, albums
cargo run -p yato-core --example match   # relie le top Deezer et quelques recherches à YouTube Music
cargo run -p yato-core --example lyrics -- "tout oublier angele"   # paroles, et minutage des mots (synchronisé ou estimé)
cargo test -p yato-core -- --include-ignored   # dont la vérification de signature réelle
```

## Ajouter une source

Implémenter `yato_core::MusicSource` (`search`, `featured`, `resolve`) puis l'enregistrer dans `app/src-tauri/src/lib.rs` :

```rust
sources.register(Arc::new(MaSource::new(client.clone())));
```

## Licence

GPL-3.0-or-later (imposée par rustypipe, sous GPL-3).

Lire YouTube en dehors de son lecteur officiel est contraire à ses conditions d'utilisation. La source YouTube est fournie telle quelle, et chacun reste responsable de son usage.
