# CSS variables

Variables are the easiest and most stable way to restyle 555: set them on `:root` and the whole app follows.

```css
:root {
  --r-lg: 6px;   /* squarer cards and panels */
  --blue: #16a34a;  /* green buttons */
}
```

## Accent

The accent is the color of the play buttons, switches, the progress bar, selected chips and glows.

| Variable | Default | Used for |
| --- | --- | --- |
| `--blue` | `#2433ff` | Main accent (buttons, switches, progress, glows). |
| `--blue-deep` | `#0d14a8` | Darker end of the accent gradients. |
| `--blue-light` | `#6f7bff` | Lighter accent, used for small highlights. |

::: warning Fixed color mode
When **Settings → Appearance → Colors** is set to **Fixed**, 555 sets `--blue` and `--blue-deep` to the chosen color directly on the page, which beats a theme. To force your accent in every mode, add `!important`:
`--blue: #16a34a !important;`
:::

## Palette (from the cover)

555 builds a full [Material 3](https://m3.material.io/styles/color/roles) dark palette from the cover of the song playing (or from the fixed color), and fades between palettes. These variables hold it. Because 555 sets them on the page while it plays, override them with `!important`:

```css
:root {
  --md-primary: #ffd23b !important;
}
```

| Variable | Used for |
| --- | --- |
| `--md-primary` | Accent text and icons, current song in lists. |
| `--md-on-primary` | Text on top of `--md-primary`. |
| `--md-primary-container` | Tinted backgrounds, the visualizer's main color. |
| `--md-on-primary-container` | Text on top of `--md-primary-container`. |
| `--md-secondary-container` | Softer tinted backgrounds. |
| `--md-on-secondary-container` | Text on top of it. |
| `--md-tertiary` | Second accent. |
| `--md-tertiary-container` | The visualizer's second color. |
| `--md-surface` | Base surface tint. |
| `--md-surface-container-lowest` … `--md-surface-container-highest` | Five levels of panels, from darkest to lightest (`-lowest`, `-low`, plain, `-high`, `-highest`). |
| `--md-on-surface` | Main text on surfaces. |
| `--md-on-surface-variant` | Secondary text on surfaces. |
| `--md-outline-variant` | Thin separators. |
| `--md-error-container`, `--md-on-error-container` | Error messages. |

::: tip Lyrics colors
Lyrics fill word by word: sung words use `--md-on-surface`, words still to come use `--md-on-surface-variant`. Set them on the current line to color it without `!important`:

```css
.lyric.on {
  --md-on-surface: #2de2ff;
  --md-on-surface-variant: #ff2bd6;
}
```
:::

## Text and surfaces

| Variable | Default | Used for |
| --- | --- | --- |
| `--bg` | `#000` | Page background. |
| `--text` | `#f4f4f7` | Main text. |
| `--text-2` | `#a3a5b8` | Secondary text (artists, hints). |
| `--text-3` | `#64667a` | Faint text (durations, labels). |
| `--surface` | `rgba(255,255,255,.05)` | Buttons, rows and fields at rest. |
| `--surface-2` | `rgba(255,255,255,.1)` | The same, hovered or active. |
| `--panel` | `rgba(6,6,14,.7)` | Sidebar and player panels. |
| `--glass` | `rgba(8,8,18,.6)` | Frosted overlays. |
| `--line` | `rgba(255,255,255,.1)` | Hairline borders. |
| `--hairline` | `inset 0 0 0 1px var(--line)` | Border as a `box-shadow`. |
| `--shadow` | soft drop shadow | Raised panels, menus, dialogs. |
| `--grain` | SVG noise | The film-grain texture over the app. |

## Shape, type and layout

| Variable | Default | Used for |
| --- | --- | --- |
| `--font` | Manrope | All text. |
| `--display` | Unbounded | Titles and the 555 logo. |
| `--r-sm` | `8px` | Small corners (rows, chips). |
| `--r-md` | `14px` | Medium corners (fields, panels). |
| `--r-lg` | `22px` | Large corners (cards, covers, dialogs). |
| `--r-pill` | `999px` | Fully rounded buttons. |
| `--gap` | `12px` | Space between the main panels. |
| `--sidebar-w` | `236px` | Sidebar width on computers. |
| `--player-h` | `68px` | Player bar height on computers. |
| `--soft` | ease-out curve | Most animations. |

## Example: a different font

```css
@import url("https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400..700&display=swap");

:root {
  --font: "Space Grotesk", system-ui, sans-serif;
  --display: "Space Grotesk", system-ui, sans-serif;
}
```
