# Theme files

A theme is a plain `.css` file. Its first comment gives its name, author and description:

```css
/*
 * @name Midnight Pink
 * @author your-name
 * @description Pink accents on pure black, rounder covers.
 */

:root {
  --blue: #e11d74;
  --blue-deep: #7a0c3d;
}

.cover {
  border-radius: 20px;
}
```

| Tag | Required | What it does |
| --- | --- | --- |
| `@name` | Recommended | The name shown in 555 and in the gallery. Without it, the file name is used. |
| `@author` | No | Shown as "by …". In the gallery, your GitHub name is used. |
| `@description` | No | One line about the theme. |

## Install a theme

**Settings → Appearance → Themes → Import .css**, then pick the file. The theme is installed and turned on right away. Only one theme is active at a time; pick **None** to go back to 555's look. The **×** next to a theme removes it.

Your custom CSS stays applied on top of the active theme, so you can tweak a theme without editing it.

## If a theme breaks the app

Press **Ctrl+Shift+0** (**⌘⇧0** on a Mac) to turn the theme and your custom CSS off until the next launch, then pick **None** in **Settings → Appearance → Themes**, or fix your CSS.

## Limits

- 256 KB per theme.
- Images can be embedded as `data:` URIs. In your own custom CSS you can also use any `https://` address, but [gallery themes](./share) can't: see the rules there.
- Fonts: [Google Fonts](https://fonts.google.com) work (`@import url("https://fonts.googleapis.com/css2?family=…")`), as well as fonts installed on the device and `data:` fonts. Other font hosts are blocked by the app's security rules.
- Themes can't run code: CSS only.
- Class names can change between versions of 555. CSS variables are much more stable: prefer them when you can.
