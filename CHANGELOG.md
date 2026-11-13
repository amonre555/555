# Changelog

What's new in 555, version by version. The latest is at the top.

## Unreleased

### Fixes

- If your collection or settings file gets damaged, 555 keeps a copy aside before starting fresh: a new like no longer wipes out the old ones.
- The preview of the image you pick for a playlist now shows up.
- A failed search no longer shows a loading spinner forever.
- Space and Alt+arrows no longer play, pause or skip while you're typing in a field (search, import…).
- When a server stops responding, lyrics, Discover and searches no longer hang forever: 555 gives up after 30 s.

## 0.3.6 — 2026-11-12

### Fixes

- Resuming a listening session after a restart no longer freezes the player: no more stuck sound, and you can start another track while it loads.
- Seeking in a track while it loads now starts it from that point.

## 0.3.5 — 2026-11-12

### New

- Android: 555 updates itself. At launch and every 6 hours, it checks for a new version and offers to install it.
- Android: “Check for updates” in the settings.

## 0.3.4 — 2026-11-11

### Improvements

- The right track is found more often: same title, same artist, similar length, and no more mixing up a live, instrumental or remix version with the original.
- 555 no longer uses any CPU while the music is paused.

## 0.3.3 — 2026-11-11

### New

- Discover now offers recommendations made for you, computed on your device from your listening, likes, playlists and the tracks you skip: daily mix, “Because you listen to…”, new releases from your artists, artists to discover, rediscoveries and trends.

### Improvements

- No duplicates on the Discover page, and no tracks you already know among the discoveries.
- The Discover page appears progressively and reloads faster.
- Carousels fit on a single line; on phones, the page no longer scrolls sideways.
- An artist page looked up by name shows the right artist rather than a namesake.

## 0.3.2 — 2026-11-11

### New

- Imported playlists keep their original image, and you can pick your own image for a playlist or remove it.
- Pasting Spotify track links into the import window adds them to your liked songs.

### Improvements

- Importing Spotify playlists finds many more tracks, including remasters and tracks missing from Deezer.

## 0.3.1 — 2026-11-11

### Improvements

- Android: a real 555 icon on the home screen, compatible with Android 13 themed icons, and in the notification.

## 0.3.0 — 2026-11-11

### New

- 555 comes to Android (8 or later), with an interface designed for phones: tabs at the bottom, mini player, full-screen search, “Now Playing” view with lyrics.
- On Android, music keeps playing with the screen off, with a notification, lock-screen controls and headset buttons. It pauses during a call or when you unplug your headphones.
- New launch screen: the 555 logo draws itself in blue light.

### Fixes

- The progress bar no longer overflows.
- The Stats page no longer keeps reloading during playback.
- No more duplicates in the history.

## 0.2.2 — 2026-11-11

### New

- Stats page: listening time, your most played tracks, artists and albums, and your listening rhythm by month and by hour, over the month, the year or all time.
- Animated year in review, available in December and for past years.
- Recent searches: your last 10 searches, to run again in one click or clear.

### Improvements

- The first track starts almost instantly, even on the very first launch (it used to take up to 15 seconds).
- The first tracks of the charts, search results and resumed session are prepared in advance.

## 0.2.1 — 2026-11-10

### Improvements

- Windows: a single installer, the `-setup.exe`, which is also used for updates.

## 0.2.0 — 2026-11-10

### New

- The app is now called 555, with a new logo and a new electric blue and black look. Your data and updates stay the same.
- The interface is available in English (default) and French, to choose in the settings.
- History page: every listen, grouped by day, with your most played tracks.
- Discord status under the 555 name.

### Improvements

- Tracks start much faster (about 0.3 s).
- Redesigned playback bar: slimmer, with the progress at the top.
- Discover is simpler, and History no longer highlights twice.

### Fixes

- The progress slider is in the right place when you resume a listening session.

## 0.1.2 — 2026-11-10

### New

- Shuffle and repeat (the queue or the track).
- Crossfade between tracks, adjustable from 1 to 12 seconds.
- Resume where you left off at launch: queue, track, position, volume, shuffle and repeat.
- “Recently played” on Discover, which you can clear in the settings.
- Import playlists from a Deezer, YouTube Music or Spotify link.
- New settings: crossfade, resume listening, Discord status, history, import and check for updates.

### Improvements

- Lists no longer flicker with every like or listen.

## 0.1.1 — 2026-11-10

First published version.

### New

- Search and stream music online, and play your local library.
- Artist pages (popular tracks, albums, singles, similar artists) and album pages; every artist and album name is clickable.
- Liked songs and playlists, which can mix local and online music; a whole album can be added in one click.
- Karaoke-style synced lyrics, word by word when possible, with an earlier/later adjustment remembered for each track.
- Immersive “Now Playing” view in the colors of the cover art, with animations.
- Discord status: current track, artist, cover art and progress.
- Media keys and the system's playback panel.
- Installers for macOS (Apple Silicon and Intel) and Windows, and automatic updates.

### Fixes

- Sound works with AirPods.
- Lyrics scroll properly, and following pauses when you scroll them by hand.
- In the “Now Playing” view, long titles no longer hide the buttons.
- A cover that fails to load no longer prevents the next ones from showing.
