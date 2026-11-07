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
# A cursive "h" with a strike through its stem (like an h-bar). Same curves as logo.dart.
$M "$T/tile.png" -fill none -stroke white -strokewidth 62 \
  -draw "stroke-linecap round stroke-linejoin round path 'M 340,320 C 380,240 450,210 470,260 C 490,320 410,600 372,790'" \
  -draw "stroke-linecap round stroke-linejoin round path 'M 410,598 C 445,505 590,420 620,560 C 640,660 590,740 610,780 C 625,810 680,780 720,730'" \
  -strokewidth 40 -draw "stroke-linecap round path 'M 372,415 L 560,415'" \
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
