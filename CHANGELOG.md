# Changelog

What's new in 555, version by version. The latest is at the top.

## 0.5.0 — 2026-11-24

### New

- Appearance (Settings → Appearance): keep the colors of the cover playing, or pick a fixed color that also tints the buttons. A slider sets the color intensity, from black and white to very vivid.
- Custom CSS: restyle anything in 555 from Settings → Appearance. Changes apply as you type.
- Themes: a theme is a single .css file. Import one, switch between your themes, or share yours in the community gallery in one click.
- Theme gallery: browse themes made by other people right inside 555, try one on the whole app, then keep it or not. Also on the website, with live previews and a playground: https://555.amon.lol/themes/
- Documentation: installing, using and theming 555, with every CSS variable and class explained: https://555.amon.lol/docs/
- On phones, a long press on a song card or on the mini player opens its menu: Play next, Add to queue, playlists.

### Safety

- Gallery themes can't load anything from other servers (only Google Fonts), so a theme can't track who uses it.
- If a theme breaks the app, Ctrl+Shift+0 (⌘⇧0 on a Mac) turns themes and custom CSS off until the next launch.

## 0.4.2 — 2026-11-24

### New

- On computers, the lyrics and Now playing screen has a volume slider and the queue button under the playback controls, like on phones. No need to leave the lyrics to turn the music down.

## 0.4.1 — 2026-11-20

### New

- Search finds artists too. Type an artist's name and they show up as the top result, with a row of matching artists below the songs. One tap opens their page.

### Fixes

- On phones, search and the player could fail with a red error box right after launch, most often the first time after an update. The app now waits until it's fully ready before showing its window.

## 0.4.0 — 2026-11-18

### New

- 555 on iPhone and iPad (iOS 15 or later), out of beta. It isn't on the App Store: you install it with SideStore, a free app that signs it with your own Apple Account. Step-by-step guide: https://555.amon.lol/ios.html
- On iPhone, music keeps playing with the screen locked, in the background and with the silent switch on. The lock screen, the Dynamic Island and Control Center show the song, the cover and the controls.
- Updates on iPhone: add the 555 source in SideStore once, and new versions install from the phone. SideStore renews the app by itself every 7 days.

## 0.3.11 — 2026-11-18

### Fixes

- The visualizer behind Now playing takes the colors of the cover again, instead of staying partly blue.

## 0.3.10 — 2026-11-18

### Fixes

- Android: music plays from the loudspeaker again (or your headphones), instead of the earpiece used for phone calls.

## 0.3.9 — 2026-11-16

### New

- Queue: "Play next" and "Add to queue" in a song's menu. Queued songs play before the rest of the list. A new button in the player bar shows the queue: drag songs to reorder them, or remove them.
- Right-click a song anywhere (lists, search, cards, the player bar) to open the same menu as the "…" button.
- Output device: a new button in the player bar lists your speakers, headphones, paired Bluetooth devices and AirPlay outputs. Pick one and the music moves there without stopping. 555 remembers your choice, and falls back to the system output if the device goes away.
- Explicit songs now show an "E" badge next to their title, when Deezer or the file's tags say so.

### Improvements

- Jumping far into a track, or picking up where you left off after a restart, now starts much faster: 555 downloads from the point you chose instead of waiting for the whole beginning.
- The player bar stays tidy at every window size: nothing overlaps anymore, and lyrics and the queue have their own clear icons.

### Fixes

- Importing a playlist now only follows real Deezer short links: a look-alike link can no longer send 555 to another address.
- Seeking in a track while it loads now always starts it from the last point you picked, even late in the loading.
- With crossfade on, pausing just as the next track gets ready no longer starts the sound again a moment later.

## 0.3.8 — 2026-11-16

### New

- Equalizer: a new button in the player bar opens a curve you shape by dragging three points (bass, mids, treble). Double-click a point to reset it.
- Presets: Slowed + reverb, Sped up and Bass boost in one click, or tune each slider yourself, with reverb and speed.
- Speed: from slowed to sped up, the pitch follows the speed like a record. Times, the progress bar and the lyrics (line by line and word by word) stay in sync.
- Visualizer: soft lights behind Now playing that move with the bass, mids and treble. Turn it on or off in Settings → Sound.

## 0.3.7 — 2026-11-13

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
