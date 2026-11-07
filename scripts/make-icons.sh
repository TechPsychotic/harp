#!/bin/sh
# Draws Harp's icon (a small harp-shaped "H" on a red gradient tile; same geometry
# as app/lib/theme/logo.dart) and writes it for every platform. Needs ImageMagick.
set -eu
cd "$(dirname "$0")/.."
M=$(command -v magick || command -v convert)
T=$(mktemp -d)
S=1024
R=246   # corner radius (24%)
$M -size ${S}x${S} -define gradient:angle=135 gradient:'#ff6b6b-#b3001b' "$T/grad.png"
$M -size ${S}x${S} xc:none -fill white -draw "roundrectangle 0,0 $((S-1)),$((S-1)) $R,$R" "$T/mask.png"
$M "$T/grad.png" "$T/mask.png" -alpha off -compose CopyOpacity -composite "$T/tile.png"
$M "$T/tile.png" -fill white -stroke none \
  -draw "roundrectangle 296,236 376,788 40,40" \
  -draw "roundrectangle 648,236 728,788 40,40" \
  -draw "roundrectangle 376,352 648,366 7,7" \
  -draw "roundrectangle 376,431 648,445 7,7" \
  -draw "roundrectangle 376,505 648,519 7,7" \
  -draw "roundrectangle 376,579 648,593 7,7" \
  -draw "roundrectangle 376,658 648,672 7,7" \
  "$T/icon.png"
mkdir -p app/assets dist/linux
cp "$T/icon.png" app/assets/icon.png
size() { $M "$T/icon.png" -resize "${1}x${1}" "$2"; }
# Android (legacy launcher icons)
for p in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
  size "${p#*:}" "app/android/app/src/main/res/mipmap-${p%%:*}/ic_launcher.png"
done
# macOS
for n in 16 32 64 128 256 512 1024; do size $n "app/macos/Runner/Assets.xcassets/AppIcon.appiconset/app_icon_$n.png"; done
# Windows
$M "$T/icon.png" -define icon:auto-resize=256,128,64,48,32,16 app/windows/runner/resources/app_icon.ico
# Linux (hicolor theme sizes)
for n in 16 24 32 48 64 128 256 512; do
  mkdir -p "dist/linux/icons/hicolor/${n}x${n}/apps"
  size $n "dist/linux/icons/hicolor/${n}x${n}/apps/dev.harp.harp.png"
done
rm -rf "$T"
echo "icons written"
