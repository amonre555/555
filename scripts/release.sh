#!/bin/sh
# Prepares a release: bumps the version everywhere, commits and creates the tag.
# Usage: ./scripts/release.sh 0.2.0
# First: in CHANGELOG.md, turn “Non publié” into “## 0.2.0 — YYYY-MM-DD” and commit it.
# Then: git push && git push origin v0.2.0 (the “Publish a release” workflow does the rest).
set -e
cd "$(dirname "$0")/.."

version="$1"
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "Usage: $0 X.Y.Z (for example 0.2.0)" >&2; exit 1 ;;
esac
# The GitHub release text comes from this version's section of CHANGELOG.md.
if ! grep -q "^## $version " CHANGELOG.md; then
  echo "CHANGELOG.md has no “## $version — YYYY-MM-DD” section: rename “Non publié” to it" >&2
  echo "(and commit) before releasing, so the release has its changelog." >&2
  exit 1
fi
# Untracked files don't matter: only changes to tracked files count.
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "There are uncommitted changes: commit them first." >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "Tag v$version already exists." >&2
  exit 1
fi

# The app version is the Cargo workspace one (tauri.conf.json has none: it reads it from there).
sed -i.bak "/^\[workspace.package\]/,/^\[/s/^version = \".*\"/version = \"$version\"/" Cargo.toml && rm Cargo.toml.bak
(cd app && npm version "$version" --no-git-tag-version --allow-same-version >/dev/null)
cargo update --workspace --quiet

git add Cargo.toml Cargo.lock app/package.json app/package-lock.json
git commit -q -m "555 $version"
git tag "v$version"
echo "Version $version ready. To publish it:"
echo "  git push && git push origin v$version"
