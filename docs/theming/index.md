# Getting started

555 is built with web technologies, so everything you see is styled with CSS. You can add your own CSS on top of it, from a one-line tweak to a complete new look.

There are three layers, applied in this order (the last one wins):

1. **555's own styles**, with the colors from **Settings → Appearance** (cover colors or a fixed color, and the intensity).
2. **The active theme**, if you picked one in **Settings → Appearance → Themes**.
3. **Your custom CSS**, from **Settings → Appearance → Custom CSS**.

## Your first change

Open **Settings → Appearance**, scroll to **Custom CSS**, and type:

```css
.np-title {
  font-style: italic;
}
```

Open the Now playing screen: the song title is in italics. Changes apply as you type, and they're saved on your device.

To undo everything, empty the box.

## Finding what to style

- **[CSS variables](./variables)** change colors, fonts, corners and sizes everywhere at once. Start there: they're the most stable part, and one line goes a long way.
- **[Selectors](./selectors)** list the classes of each part of the app (sidebar, player, cards, lyrics…) for more targeted changes.
- On a computer, you can look at the app's structure live: run 555 from the source code (`npm run tauri dev`), then right-click → **Inspect element**.

::: tip Phones
555 switches to its phone layout below 760 px wide (and on Android, iPhone and iPad). Target it with `@media (max-width: 760px) { … }`.
:::

## Make it a theme

Once you like your CSS, turn it into a [theme file](./theme-files) to keep several looks around and switch between them, and [share it in the gallery](./share).
