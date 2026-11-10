#!/bin/sh
# Prépare une version : change le numéro partout, commite et crée le tag.
# Usage : ./scripts/release.sh 0.2.0
# Ensuite : git push && git push origin v0.2.0 (le workflow « Publier une version » fait le reste).
set -e
cd "$(dirname "$0")/.."

version="$1"
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "Usage : $0 X.Y.Z (par exemple 0.2.0)" >&2; exit 1 ;;
esac
if [ -n "$(git status --porcelain)" ]; then
  echo "Des changements ne sont pas commités : commite-les d'abord." >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "Le tag v$version existe déjà." >&2
  exit 1
fi

# La version de l'app est celle du workspace Cargo (tauri.conf.json n'en a pas : il la lit là).
sed -i.bak "/^\[workspace.package\]/,/^\[/s/^version = \".*\"/version = \"$version\"/" Cargo.toml && rm Cargo.toml.bak
(cd app && npm version "$version" --no-git-tag-version --allow-same-version >/dev/null)
cargo update --workspace --quiet

git add Cargo.toml Cargo.lock app/package.json app/package-lock.json
git commit -q -m "yato $version"
git tag "v$version"
echo "Version $version prête. Pour la publier :"
echo "  git push && git push origin v$version"
