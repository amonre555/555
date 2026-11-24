#!/usr/bin/env node
// Builds site/themes.json, the theme gallery, from the "Themes" category of the repository's
// GitHub Discussions: one post = one theme, upvotes = votes. Run by .github/workflows/pages.yml
// (every hour and when a post changes). Needs GITHUB_TOKEN (read access to discussions).
//
// A post's theme is the first ```css code block in it; its name is the CSS's @name or the post
// title; the text before the code block is the description. Locked posts are left out.
//
// Usage: GITHUB_TOKEN=… node scripts/collect-themes.mjs [owner/repo] [output file]

import { writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { pathToFileURL } from "node:url";

const [owner, name] = (process.argv[2] ?? process.env.GITHUB_REPOSITORY ?? "amonre555/555").split("/");
const out = process.argv[3] ?? "site/themes.json";
const token = process.env.GITHUB_TOKEN;
const MAX_CSS = 256 * 1024;

function write(themes, note) {
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify({ generatedAt: new Date().toISOString(), themes }, null, 1) + "\n");
  console.log(`${out}: ${themes.length} theme(s)${note ? ` (${note})` : ""}`);
}

async function gql(query, variables) {
  const r = await fetch("https://api.github.com/graphql", {
    method: "POST",
    headers: { authorization: `bearer ${token}`, "content-type": "application/json", "user-agent": "555-theme-gallery" },
    body: JSON.stringify({ query, variables }),
  });
  const data = await r.json();
  if (!r.ok || data.errors) throw new Error(JSON.stringify(data.errors ?? data));
  return data.data;
}

/** First ```css block, and the text before it (without HTML comments, images and headings). */
export function parsePost(body) {
  const m = body.match(/```css[^\n]*\n([\s\S]*?)```/i);
  if (!m) return null;
  const css = m[1].trim();
  const before = body
    .slice(0, m.index)
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/<img[^>]*>/gi, "")
    .replace(/^#+\s.*$/gm, "")
    .replace(/[*_`>]/g, "")
    .trim();
  const tag = (t) => css.match(new RegExp(`^\\s*/\\*[\\s\\S]*?@${t}\\s+([^\\n*]+)`))?.[1]?.trim();
  const description = (tag("description") ?? before.split(/\n\s*\n/)[0] ?? "").replace(/\s+/g, " ").slice(0, 240);
  return { css, name: tag("name"), description: description || undefined };
}

async function main() {
  if (!token) {
    write([], "no GITHUB_TOKEN");
    return;
  }
  // Discussions turned off, or no "Themes" category yet: GitHub answers NOT_FOUND.
  const cat = await gql(
    `query($owner: String!, $name: String!, $slug: String!) { repository(owner: $owner, name: $name) { discussionCategory(slug: $slug) { id } } }`,
    { owner, name, slug: process.env.THEMES_CATEGORY ?? "themes" },
  ).catch((e) => (String(e.message).includes("NOT_FOUND") ? null : Promise.reject(e)));
  const categoryId = cat?.repository?.discussionCategory?.id;
  if (!categoryId) {
    write([], 'no "Themes" discussion category yet');
    return;
  }
  const themes = [];
  let after = null;
  for (let page = 0; page < 20; page++) {
    const d = await gql(
      `query($owner: String!, $name: String!, $cat: ID!, $after: String) {
        repository(owner: $owner, name: $name) {
          discussions(first: 100, after: $after, categoryId: $cat, orderBy: { field: CREATED_AT, direction: DESC }) {
            pageInfo { hasNextPage endCursor }
            nodes { number title url body upvoteCount locked createdAt updatedAt author { login avatarUrl(size: 64) url } }
          }
        }
      }`,
      { owner, name, cat: categoryId, after },
    );
    const list = d.repository.discussions;
    for (const p of list.nodes) {
      if (p.locked) continue;
      const parsed = parsePost(p.body ?? "");
      if (!parsed || !parsed.css || parsed.css.length > MAX_CSS) continue;
      themes.push({
        id: String(p.number),
        name: (parsed.name ?? p.title).slice(0, 80),
        author: p.author?.login ?? "ghost",
        authorAvatar: p.author?.avatarUrl,
        authorUrl: p.author?.url,
        description: parsed.description,
        css: parsed.css,
        votes: p.upvoteCount,
        url: p.url,
        createdAt: p.createdAt,
        updatedAt: p.updatedAt,
      });
    }
    if (!list.pageInfo.hasNextPage) break;
    after = list.pageInfo.endCursor;
  }
  themes.sort((a, b) => b.votes - a.votes || b.createdAt.localeCompare(a.createdAt));
  write(themes);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((e) => {
    // The rest of the site must still be published: keep an empty gallery rather than failing.
    console.error("theme gallery:", e.message);
    write([], "error");
  });
}
