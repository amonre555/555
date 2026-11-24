import { defineConfig } from "vitepress";

// Published at https://555.amon.lol/docs/ by .github/workflows/pages.yml.
export default defineConfig({
  title: "555 Docs",
  description: "Documentation for 555, the free and open-source music player: theming, custom CSS and the theme gallery.",
  base: "/docs/",
  lang: "en",
  cleanUrls: true,
  appearance: "force-dark",
  lastUpdated: true,
  head: [
    ["link", { rel: "icon", href: "/assets/icon.svg", type: "image/svg+xml" }],
    ["meta", { name: "theme-color", content: "#000000" }],
  ],
  themeConfig: {
    logo: { src: "/logo.svg", alt: "555" },
    siteTitle: "Docs",
    nav: [
      { text: "Theming", link: "/theming/" },
      { text: "Theme gallery", link: "https://555.amon.lol/themes/" },
      { text: "Download", link: "https://555.amon.lol/" },
    ],
    sidebar: [
      { text: "Introduction", items: [{ text: "What is 555?", link: "/" }] },
      {
        text: "Theming",
        items: [
          { text: "Getting started", link: "/theming/" },
          { text: "Theme files", link: "/theming/theme-files" },
          { text: "CSS variables", link: "/theming/variables" },
          { text: "Selectors", link: "/theming/selectors" },
          { text: "Examples", link: "/theming/examples" },
          { text: "Share in the gallery", link: "/theming/share" },
        ],
      },
    ],
    socialLinks: [{ icon: "github", link: "https://github.com/amonre555/555" }],
    search: { provider: "local" },
    editLink: { pattern: "https://github.com/amonre555/555/edit/main/docs/:path", text: "Edit this page on GitHub" },
    footer: { message: "Free and open source.", copyright: "555" },
  },
});
