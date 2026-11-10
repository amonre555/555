#!/bin/sh
# Builds icon.icns (macOS icon) from icon-macos.svg. Needs rsvg-convert (brew install librsvg).
set -e
cd "$(dirname "$0")"
set_dir=$(mktemp -d)/yato.iconset
mkdir -p "$set_dir"
for s in 16 32 128 256 512; do
  rsvg-convert -w $s -h $s icon-macos.svg -o "$set_dir/icon_${s}x${s}.png"
  rsvg-convert -w $((s * 2)) -h $((s * 2)) icon-macos.svg -o "$set_dir/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$set_dir" -o icon.icns
echo "icon.icns rebuilt"
