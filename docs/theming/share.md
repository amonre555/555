# Share in the gallery

The [theme gallery](https://555.amon.lol/themes/) collects themes made by 555 users. Each theme has a live preview, and people vote for the ones they like. The gallery is also built into the app: **Settings → Appearance → Themes → Browse themes**, where you can try a theme on your own 555 before keeping it.

The gallery lives in [GitHub Discussions](https://github.com/amonre555/555/discussions/categories/themes): one post per theme, and upvotes are the votes.

## Post your theme

**From 555:** write your theme in **Custom CSS**, then click **Share my CSS**. A new GitHub post opens, already filled in: add a short description (and a screenshot if you like), then publish. You need a free GitHub account.

**By hand:** open a [new post in the Themes category](https://github.com/amonre555/555/discussions/new?category=themes) and paste your theme in a `css` code block:

````md
Pink accents on pure black, rounder covers.

```css
/*
 * @name Midnight Pink
 * @description Pink accents on pure black, rounder covers.
 */
:root { --blue: #e11d74 !important; }
.cover { border-radius: 20px; }
```
````

- The **first** `css` code block of the post is the theme.
- The name comes from `@name`, or from the post title.
- The text above the code block is the description.
- Your GitHub name is shown as the author.

The gallery updates about once an hour. To update your theme, edit your post: the new CSS replaces the old one, and people who installed it can click **Update** in 555.

## Vote

Upvote a post on GitHub (the ▲ next to it) to vote for a theme. The gallery and the app list the most voted themes first.

## Rules

- CSS only, 256 KB max.
- No tracking: don't load images or fonts from your own server to count visitors.
- Keep it friendly. Posts that break these rules are locked, and locked posts leave the gallery.
