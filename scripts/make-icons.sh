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
# The glyph alone (white on transparent) for Android's adaptive and notification icons.
G1="M 340,320 C 380,240 450,210 470,260 C 490,320 410,600 372,790"
G2="M 410,598 C 445,505 590,420 620,560 C 640,660 590,740 610,780 C 625,810 680,780 720,730"
glyph() { # scale outfile
  tx=$(python3 -c "print(512-$1*530)"); ty=$(python3 -c "print(512-$1*515)")
  $M -size ${S}x${S} xc:none -fill none -stroke white -strokewidth 62 \
    -draw "translate $tx,$ty scale $1,$1 stroke-linecap round stroke-linejoin round path '$G1'" \
    -draw "translate $tx,$ty scale $1,$1 stroke-linecap round stroke-linejoin round path '$G2'" \
    -strokewidth 40 -draw "translate $tx,$ty scale $1,$1 stroke-linecap round path 'M 372,415 L 560,415'" "$2"
}
mkdir -p app/assets dist/linux
cp "$T/icon.png" app/assets/icon.png
size() { $M "$T/icon.png" -resize "${1}x${1}" "$2"; }
# Android (legacy launcher icons)
for p in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
  size "${p#*:}" "app/android/app/src/main/res/mipmap-${p%%:*}/ic_launcher.png"
done
# Android adaptive icon: gradient background, glyph inside the safe zone, themed (monochrome) layer
R=app/android/app/src/main/res
glyph 0.74 "$T/fg.png"
glyph 0.92 "$T/stat.png"
for p in mdpi:108:24 hdpi:162:36 xhdpi:216:48 xxhdpi:324:72 xxxhdpi:432:96; do
  d=${p%%:*}; rest=${p#*:}; fg=${rest%%:*}; st=${rest#*:}
  size_from() { $M "$1" -resize "${2}x${2}" "$3"; }
  size_from "$T/fg.png" "$fg" "$R/mipmap-$d/ic_launcher_foreground.png"
  mkdir -p "$R/drawable-$d"
  size_from "$T/stat.png" "$st" "$R/drawable-$d/ic_stat_harp.png"
done
mkdir -p "$R/mipmap-anydpi-v26" "$R/drawable"
cat > "$R/drawable/ic_launcher_background.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<shape xmlns:android="http://schemas.android.com/apk/res/android" android:shape="rectangle">
    <gradient android:angle="315" android:startColor="#FFFF6B6B" android:endColor="#FFB3001B" android:type="linear"/>
</shape>
XML
cat > "$R/mipmap-anydpi-v26/ic_launcher.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background"/>
    <foreground android:drawable="@mipmap/ic_launcher_foreground"/>
    <monochrome android:drawable="@mipmap/ic_launcher_foreground"/>
</adaptive-icon>
XML
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
