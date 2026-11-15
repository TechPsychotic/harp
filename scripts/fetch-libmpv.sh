#!/bin/bash
# Fetches the audio engine (libmpv) Harp builds with into app/libmpv/, which pubspec.yaml points
# mpv_audio_kit at (hooks › user_defines › mpv_audio_kit › libmpv).
#
# Android arm64 is Harp's own build: libmpv-r23 with the dsp_hook patch, which lets Harp's 3D
# audio run inside the engine (TechPsychotic/libmpv-scripts, branch harp/r23). Every other
# target is mpv_audio_kit's stock r23 release, the one mpv_audio_kit 0.6.4 expects. Each file is
# checked against its SHA-256; files already there and correct are kept.
set -euo pipefail
cd "$(dirname "$0")/../app"
mkdir -p libmpv

STOCK=https://github.com/ales-drnz/mpv_audio_kit/releases/download/libmpv-r23
HARP=https://github.com/TechPsychotic/libmpv-scripts/releases/download/harp-libmpv-r23.1

sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

fetch() { # base name sha256
  local f="libmpv/$2"
  if [ -f "$f" ] && [ "$(sha "$f")" = "$3" ]; then return 0; fi
  for try in 1 2 3; do
    if curl -fsSL --retry 2 -o "$f.part" "$1/$2" && [ "$(sha "$f.part")" = "$3" ]; then
      mv "$f.part" "$f"
      return 0
    fi
    sleep $((try * 5))
  done
  rm -f "$f.part"
  echo "fetch-libmpv: couldn't get $2 with the expected SHA-256" >&2
  return 1
}

fetch "$HARP"  libmpv_android-arm64-v8a.so   417abc5cdb08a39f034481695ab37895500ab6f16f95e61dd6f32de7ee9b153d
fetch "$STOCK" libmpv_android-armeabi-v7a.so 0415456779015ad2d62ce7b1549c0adb54821986e88c2b0ac82053e3af31cccb
fetch "$STOCK" libmpv_android-x86_64.so      0f0bbc952feed353e5bfd4bc0c1af2d4944631574caf953883cfde7755e0a219
fetch "$STOCK" libmpv_ios.xcframework.zip    cd5c8700c19b12c7e802410a07e4fd93fb906a245a391330a985d34abdf73cea
fetch "$STOCK" libmpv_linux-aarch64.so       3bc9b3277705686e00303f2a2cebeef6dc9ad22a5b1f71f41de78557d5904789
fetch "$STOCK" libmpv_linux-x86_64.so        13d59f9026d831d8c69fc897b7f120e1d96ed67a8fb0e10e206b1adc44786239
fetch "$STOCK" libmpv_macos.xcframework.zip  55c720db17eaaa20f8928cc53587a06dfcb92c154db8a0a5fe20f2552730898e
fetch "$STOCK" libmpv_windows-arm64.dll      d72b331061cdbfc61f85595a2879aac06fbea9c4ab966be819ebf870ff94c42a
fetch "$STOCK" libmpv_windows-x86_64.dll     36928dcaf795385f476c0484077757c91b946436fc0c4954c0f344d093bdd7b1
echo "fetch-libmpv: app/libmpv/ ready"
