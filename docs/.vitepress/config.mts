import { defineConfig } from "vitepress";

// Published at https://555.amon.lol/docs/ by .github/workflows/pages.yml.
export default defineConfig({
  title: "555 Docs",
  description: "Documentation for 555, the free and open-source music player: installing, using and customizing it.",
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
    // Same links as the website's navigation bar (site/assets/nav.css).
    nav: [
      { text: "Features", link: "https://555.amon.lol/#features", target: "_self", noIcon: true },
      { text: "Download", link: "https://555.amon.lol/#download", target: "_self", noIcon: true },
      { text: "FAQ", link: "https://555.amon.lol/#faq", target: "_self", noIcon: true },
      { text: "Themes", link: "https://555.amon.lol/themes/", target: "_self", noIcon: true },
      { text: "Docs", link: "/", activeMatch: "^/" },
    ],
    sidebar: [
      {
        text: "Guide",
        items: [
          { text: "Install", link: "/guide/install" },
          { text: "Using 555", link: "/guide/using" },
        ],
      },
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
