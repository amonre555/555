# Changelog

Les changements de 555, version par version. La plus récente est en haut.

## Non publié

### Corrections

- Si le fichier de la collection ou des réglages est abîmé, 555 en garde une copie de côté avant de repartir de zéro : un nouveau like ne fait plus perdre les anciens.
- L'aperçu de l'image choisie pour une playlist s'affiche bien.
- Une recherche qui échoue n'affiche plus un chargement sans fin.
- Espace et Alt+flèches ne déclenchent plus la lecture ou le changement de morceau pendant que tu écris dans un champ (recherche, import…).
- Un long morceau ne se coupe plus en cours de route quand la connexion ralentit.

## 0.3.6 — 2026-11-12

### Corrections

- Reprendre une écoute au redémarrage ne bloque plus le lecteur : plus de son figé, et tu peux lancer un autre morceau pendant le chargement.
- Avancer dans un morceau pendant qu'il charge le fait démarrer à cet endroit.

## 0.3.5 — 2026-11-12

### Nouveautés

- Android : 555 se met à jour tout seul. Au lancement puis toutes les 6 h, il vérifie s'il existe une nouvelle version et propose de l'installer.
- Android : « Rechercher une mise à jour » dans les réglages.

## 0.3.4 — 2026-11-11

### Améliorations

- Le bon morceau est trouvé plus souvent : même titre, même artiste, durée proche, et sans confondre une version live, instrumentale ou remix avec l'originale.
- 555 ne consomme plus de processeur quand la musique est en pause.

## 0.3.3 — 2026-11-11

### Nouveautés

- Découvrir propose des recommandations faites pour toi, calculées sur ton appareil d'après tes écoutes, tes likes, tes playlists et les morceaux que tu passes : mix du jour, « Parce que tu écoutes… », nouveautés de tes artistes, artistes à découvrir, redécouvertes et tendances.

### Améliorations

- Pas de doublons sur la page Découvrir, et pas de titres que tu connais déjà dans les découvertes.
- La page Découvrir s'affiche progressivement et se recharge plus vite.
- Les carrousels tiennent sur une ligne ; sur téléphone, la page ne défile plus de travers.
- Une fiche d'artiste retrouvée par son nom affiche le bon artiste plutôt qu'un homonyme.

## 0.3.2 — 2026-11-11

### Nouveautés

- Les playlists importées gardent leur image d'origine, et tu peux choisir ta propre image pour une playlist ou la retirer.
- Coller des liens de morceaux Spotify dans la fenêtre d'import les ajoute à tes titres likés.

### Améliorations

- L'import de playlists Spotify retrouve beaucoup plus de morceaux, y compris les rééditions et ceux absents de Deezer.

## 0.3.1 — 2026-11-11

### Améliorations

- Android : vraie icône 555 sur l'écran d'accueil, compatible avec les icônes à thème d'Android 13, et dans la notification.

## 0.3.0 — 2026-11-11

### Nouveautés

- 555 arrive sur Android (8 ou plus récent), avec une interface pensée pour le téléphone : onglets en bas, mini-lecteur, recherche plein écran, vue « En lecture » avec paroles.
- Sur Android, la musique continue écran éteint, avec notification, contrôles sur l'écran verrouillé et boutons du casque. Elle se met en pause pendant un appel ou si tu débranches le casque.
- Nouvel écran de lancement : le logo 555 se dessine en lumière bleue.

### Corrections

- La barre de progression ne déborde plus.
- La page Stats ne se recharge plus sans arrêt pendant la lecture.
- Plus de doublons dans l'historique.

## 0.2.2 — 2026-11-11

### Nouveautés

- Page Stats : temps d'écoute, tes morceaux, artistes et albums les plus écoutés, et ton rythme d'écoute par mois et par heure, sur le mois, l'année ou depuis toujours.
- Récap de l'année animé, disponible en décembre et pour les années passées.
- Recherches récentes : tes 10 dernières recherches, à relancer d'un clic ou à effacer.

### Améliorations

- Le premier morceau démarre presque instantanément, même au tout premier lancement (auparavant jusqu'à 15 secondes d'attente).
- Les premiers morceaux du top, des résultats de recherche et de l'écoute reprise sont préparés à l'avance.

## 0.2.1 — 2026-11-10

### Améliorations

- Windows : un seul installateur, le `-setup.exe`, qui sert aussi aux mises à jour.

## 0.2.0 — 2026-11-10

### Nouveautés

- L'app s'appelle désormais 555, avec un nouveau logo et une nouvelle identité bleu électrique et noir. Tes données et les mises à jour ne changent pas.
- L'interface est disponible en anglais (par défaut) et en français, au choix dans les réglages.
- Page Historique : chaque écoute, regroupée par jour, avec les morceaux les plus écoutés.
- Statut Discord au nom de 555.

### Améliorations

- Les morceaux démarrent bien plus vite (environ 0,3 s).
- Barre de lecture repensée : plus fine, avec la progression en haut.
- Découvrir est plus simple, et l'Historique n'a plus de surlignage en double.

### Corrections

- Le curseur de progression est à la bonne place quand tu reprends une écoute.

## 0.1.2 — 2026-11-10

### Nouveautés

- Lecture aléatoire et répétition (de la file ou du morceau).
- Fondu enchaîné entre les morceaux, réglable de 1 à 12 secondes.
- Reprise de l'écoute au lancement : file, morceau, position, volume, aléatoire et répétition.
- « Écoutés récemment » sur Découvrir, effaçable dans les réglages.
- Import de playlists depuis un lien Deezer, YouTube Music ou Spotify.
- Nouveaux réglages : fondu enchaîné, reprise de l'écoute, statut Discord, historique, import et recherche de mise à jour.

### Améliorations

- Les listes ne clignotent plus à chaque like ou écoute.

## 0.1.1 — 2026-11-10

Première version publiée.

### Nouveautés

- Recherche et écoute de musique en ligne, et lecture de ta bibliothèque locale.
- Fiches d'artistes (titres populaires, albums, singles, artistes similaires) et fiches d'albums ; chaque nom d'artiste et d'album est cliquable.
- Titres likés et playlists, qui peuvent mélanger musique locale et en ligne ; un album entier s'ajoute en un clic.
- Paroles synchronisées façon karaoké, au mot près quand c'est possible, avec un réglage d'avance ou de retard retenu pour chaque titre.
- Vue « En lecture » immersive, aux couleurs de la pochette, avec animations.
- Statut Discord : morceau écouté, artiste, pochette et progression.
- Touches multimédia et panneau de lecture du système.
- Installateurs pour macOS (Apple Silicon et Intel) et Windows, et mises à jour automatiques.

### Corrections

- Le son fonctionne avec des AirPods.
- Les paroles défilent correctement, et le suivi se met en pause quand tu les fais défiler à la main.
- Dans la vue « En lecture », les titres longs ne cachent plus les boutons.
- Une pochette qui ne charge pas n'empêche plus l'affichage des suivantes.
