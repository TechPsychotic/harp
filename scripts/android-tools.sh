#!/bin/sh
# Fetches what Harp's Android build runs yt-dlp with, into app/android/app/src/main/jniLibs/arm64-v8a/
# and app/assets/python.zip. Same technique as the sibling project zevro:
#
#   libpython.so  python-build-standalone's static musl Python (runs the yt-dlp zipapp,
#                 which the app downloads and verifies itself)
#   libqjs.so     quickjs-ng, static, for YouTube's player challenges
#   python.zip    Python's standard library as sources (unpacked once per install)
#
# Android runs programs only from the app's library folder, hence the file names.
# Every download is checked against a pinned SHA-256 (taken from zevro's script).
# arm64 only: musl on x86_64 makes system calls Android's seccomp filter kills.
# Needs: curl, zstd, tar, python3.
set -eu
cd "$(dirname "$0")/.."
ROOT=$(pwd)
CACHE=${HARP_ANDROID_CACHE:-$ROOT/app/android/build-tools}
OUT=$ROOT/app/android/app/src/main/jniLibs/arm64-v8a
ASSETS=$ROOT/app/assets
PBS=20261026; PY=3.13.18; PYV=3.13; QJS=v0.18.0
PY_URL=https://github.com/astral-sh/python-build-standalone/releases/download/$PBS/cpython-$PY+$PBS-aarch64-unknown-linux-musl-lto+static-full.tar.zst
PY_SUM=9968a4b1fc3db172ba3b3ab36235fa76fab91c3ec83204f269adca756c87261d
# A fallback yt-dlp, shipped inside the APK: used only if the first download on the phone
# fails (as during a GitHub outage). The app still updates yt-dlp itself afterwards.
YTDLP=2026.10.26
YTDLP_URL=https://github.com/yt-dlp/yt-dlp/releases/download/$YTDLP/yt-dlp
YTDLP_SUM=e51e69e89489f199530f13d32a4342da3ddb2cf34842f364603a066db05afbf3
QJS_URL=https://github.com/quickjs-ng/quickjs/releases/download/$QJS/qjs-linux-aarch64
QJS_SUM=2c005c7171424a598d31c53c8dc4a0ae2e4958eb6fbb37908c885a307c78187c

mkdir -p "$CACHE" "$OUT" "$ASSETS"
fetch() { # file url sum
	if [ ! -f "$CACHE/$1" ] || ! echo "$3  $CACHE/$1" | sha256sum -c --status; then
		echo "downloading $2"
		curl -fsSL --retry 8 --retry-delay 4 --retry-all-errors -o "$CACHE/$1.part" "$2" && mv "$CACHE/$1.part" "$CACHE/$1"
	fi
	echo "$3  $CACHE/$1" | sha256sum -c --status || { echo "$1 does not match its pinned SHA-256" >&2; rm -f "$CACHE/$1"; exit 1; }
}
fetch python.tar.zst "$PY_URL" "$PY_SUM"
fetch qjs "$QJS_URL" "$QJS_SUM"
fetch yt-dlp.pyz "$YTDLP_URL" "$YTDLP_SUM"
cp "$CACHE/yt-dlp.pyz" "$ASSETS/yt-dlp.pyz"

zstd -dc "$CACHE/python.tar.zst" | tar -x -O "python/install/bin/python$PYV" >"$OUT/libpython.so"
cp "$CACHE/qjs" "$OUT/libqjs.so"
chmod 755 "$OUT/libpython.so" "$OUT/libqjs.so"

rm -rf "$CACHE/pylib" && mkdir -p "$CACHE/pylib"
zstd -dc "$CACHE/python.tar.zst" | tar -x -C "$CACHE/pylib" "python/install/lib/python$PYV"
LIB=$CACHE/pylib/python/install/lib/python$PYV
(cd "$LIB" && rm -rf test idlelib tkinter turtledemo ensurepip site-packages lib2to3 pydoc_data config-* lib-dynload __phello__ turtle.py)
find "$LIB" -name __pycache__ -prune -exec rm -rf {} + 2>/dev/null || true
find "$LIB" -name tests -type d -prune -exec rm -rf {} + 2>/dev/null || true
python3 -I - "$LIB" "$ASSETS/python.zip" <<'PY'
import os, sys, zipfile
lib, out = sys.argv[1:3]
with zipfile.ZipFile(out + ".tmp", "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for root, dirs, files in os.walk(lib):
        dirs.sort()
        for f in sorted(files):
            p = os.path.join(root, f)
            z.write(p, os.path.relpath(p, lib))
os.replace(out + ".tmp", out)
PY
# Strip them (python goes from 54 MB to 23 MB) if an NDK is available.
NDK=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-${ANDROID_NDK_LATEST_HOME:-}}}
[ -z "$NDK" ] && NDK=$(ls -d "${ANDROID_HOME:-$HOME/Android/Sdk}"/ndk/* 2>/dev/null | sort -V | tail -1)
STRIP=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip
if [ -x "$STRIP" ]; then "$STRIP" "$OUT/libpython.so" "$OUT/libqjs.so"; else echo "no NDK found: leaving binaries unstripped"; fi
echo "done: $OUT/libpython.so, $OUT/libqjs.so, $ASSETS/python.zip"
