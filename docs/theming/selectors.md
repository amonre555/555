# Selectors

When variables aren't enough, target the parts of the app by class. This is a map of the main ones.

::: warning
Class names can change between versions of 555. Keep your selectors short (`.np-title`, not `.app .nowplaying .np-left .np-meta .np-title`) and prefer [variables](./variables) when you can.
:::

## Layout

| Selector | What it is |
| --- | --- |
| `.app` | The whole window. Gets extra classes: `.view-discover`, `.view-search`, `.view-library`, `.view-settings`… for the current page, `.has-track` when a song is loaded, `.playing-on` while playing, `.expanded` when Now playing is open. |
| `.sidebar` | Left column on computers (logo, navigation, playlists). |
| `.sidenav` | The navigation links inside it. |
| `.main` | Everything right of the sidebar. |
| `.topbar` | Top row with the search field. |
| `.content` | The scrolling page area. |
| `.page`, `.page-head` | A page and its big title. |
| `.player` | Player bar at the bottom (mini player on phones). |
| `.tabbar` | Bottom tabs on phones. |

## Player bar

| Selector | What it is |
| --- | --- |
| `.now-btn`, `.now-title`, `.now-sub` | Current song: cover, title, artists. Click it to open Now playing. |
| `.transport`, `.buttons` | Previous / play / next, shuffle and repeat. |
| `.play-btn` | The round play/pause button (also in Now playing). |
| `.progress-line`, `.seek`, `.seek-played` | The progress bar and its played part. |
| `.side-controls` | Right side: lyrics, queue, output, equalizer, volume. |
| `.volume` | The volume control (`.volume .slider` is the slider). |
| `.player-time` | Elapsed / total time. |

## Now playing and lyrics

| Selector | What it is |
| --- | --- |
| `.nowplaying` | The full-screen Now playing view. Gets `.lyrics-mode` on phones when lyrics replace the cover. |
| `.np-bg` | Blurred cover background. |
| `.np-left`, `.np-right` | Cover and controls column / lyrics column. |
| `.np-art` | The big cover. |
| `.np-title`, `.np-sub`, `.np-album` | Title, artists, album. |
| `.np-actions` | Queue and volume under the controls (and the lyrics button on phones). |
| `.lyrics` | The lyrics list. |
| `.lyric` | One line of lyrics. The current line has `.on`, sung lines `.past`; each word is a `.lyric-word`. |
| `.lyrics.plain` | Lyrics without timing (plain text). |

## Lists and cards

| Selector | What it is |
| --- | --- |
| `.section`, `.section-title`, `.section-sub` | A block on a page and its title. |
| `.card`, `.card-art`, `.card-title`, `.card-sub` | Song, album and playlist cards. `.card.playing` is the one playing. |
| `.card-play` | Round play button on a card. |
| `.artist-card`, `.artist-photo` | Artist cards and photos. |
| `.row` | One song in a list. `.row-title`, `.row-sub`, `.row-album`, `.row-time`, `.row-like`, `.row-more`. |
| `.cover` | Every cover image (`.cover.sm`, `.md`, `.lg`, `.xl` by size). |
| `.hero`, `.hero-title` | Big header of album, artist and playlist pages. |

## Controls

| Selector | What it is |
| --- | --- |
| `.primary` | Main buttons (blue). |
| `.ghost` | Secondary buttons. |
| `.icon-btn` | Round icon buttons. |
| `.slider` | Every slider (`input[type=range]`). |
| `.switch`, `.switch.on` | On/off switches in Settings. |
| `.segmented` | Choice pills (language, color mode…). |
| `.menu`, `.menu-item` | Right-click / "⋯" menus. |
| `.dialog` | Pop-up windows. |
| `.search` | The search field. |
