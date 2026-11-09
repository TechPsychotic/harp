#!/bin/bash
# Harp's CI, run on this laptop instead of GitHub Actions.
#
#   1. Rust tests, Flutter analysis and Flutter tests (light);
#   2. the Linux app and the Android APK, each through the memory-guarded builder
#      (scripts/safe-build-apk.sh), one after the other, never at the same time;
#   3. checks on the APK: the notification icon survived, Python / QuickJS / the
#      Rust core are inside, the media service is in the manifest;
#   4. optionally installs the Linux build and copies the APK to ~/Downloads/Harp.
#
# Usage: scripts/ci-local.sh [--no-apk] [--no-linux] [--install]
# Exits non-zero at the first failure. A summary is printed at the end.
set -u
cd "$(dirname "$0")/.."
ROOT=$(pwd)
export PATH="$HOME/development/flutter/bin:$PATH" ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
APK=1 LINUX=1 INSTALL=0
for a in "$@"; do
  case "$a" in
    --no-apk) APK=0 ;;
    --no-linux) LINUX=0 ;;
    --install) INSTALL=1 ;;
    *) echo "unknown option $a"; exit 2 ;;
  esac
done
SUMMARY=()
step() { echo; echo "=== $1"; }
ok() { SUMMARY+=("ok    $1"); }
fail() { SUMMARY+=("FAIL  $1"); printf '%s\n' "${SUMMARY[@]}"; exit 1; }

step "Rust tests"
nice -n 10 cargo test -q --workspace >/tmp/harp-ci-rust.log 2>&1 && ok "Rust tests ($(grep -o '[0-9]* passed' /tmp/harp-ci-rust.log | awk '{s+=$1} END {print s}') passed)" || { tail -30 /tmp/harp-ci-rust.log; fail "Rust tests"; }

step "Flutter analyze and tests"
(cd app && nice -n 10 flutter analyze --no-fatal-infos >/tmp/harp-ci-analyze.log 2>&1) && ok "flutter analyze" || { tail -30 /tmp/harp-ci-analyze.log; fail "flutter analyze"; }
(cd app && nice -n 10 flutter test >/tmp/harp-ci-test.log 2>&1) && ok "flutter test ($(grep -o '+[0-9]*: All tests passed' /tmp/harp-ci-test.log | tr -dc 0-9) passed)" || { tail -30 /tmp/harp-ci-test.log; fail "flutter test"; }

if [ "$LINUX" = 1 ]; then
  step "Linux build (memory-guarded)"
  HARP_BUILD_LOG=/tmp/harp-ci-linux.log scripts/safe-build-apk.sh 30 linux && ok "Linux build" || { tail -30 /tmp/harp-ci-linux.log; fail "Linux build"; }
  if [ "$INSTALL" = 1 ]; then
    tar -C app/build/linux/x64/release/bundle -czf /tmp/harp-linux-x64.tar.gz . \
      && scripts/install-linux.sh /tmp/harp-linux-x64.tar.gz >/dev/null \
      && { [ -d "$HOME/.local/mpv/usr/lib64" ] && cp -a "$HOME/.local/mpv/usr/lib64"/lib*.so* "$HOME/.local/opt/harp/lib/" || true; } \
      && ok "Linux app installed in ~/.local/opt/harp" || fail "Linux install"
  fi
fi

if [ "$APK" = 1 ]; then
  step "Android build (memory-guarded)"
  HARP_BUILD_LOG=/tmp/harp-ci-apk.log scripts/safe-build-apk.sh 30 apk && ok "APK build" || { tail -30 /tmp/harp-ci-apk.log; fail "APK build"; }
  F=app/build/app/outputs/flutter-apk/app-release.apk
  AAPT2=$(ls -d "$ANDROID_HOME"/build-tools/*/aapt2 | sort -V | tail -1)
  "$AAPT2" dump resources "$F" | grep -q "drawable/ic_stat_harp" && ok "APK: notification icon present" || fail "APK: notification icon missing (the media notification would crash)"
  unzip -l "$F" | grep -q "lib/arm64-v8a/libpython.so" && unzip -l "$F" | grep -q "libqjs.so" && unzip -l "$F" | grep -q "libharp_ffi.so" && ok "APK: Python, QuickJS and the Rust core bundled" || fail "APK: missing bundled libraries"
  "$AAPT2" dump xmltree --file AndroidManifest.xml "$F" | grep -q "com.ryanheise.audioservice.AudioService" && ok "APK: media service in the manifest" || fail "APK: media service missing"
  if [ "$INSTALL" = 1 ]; then
    mkdir -p "$HOME/Downloads/Harp" && cp "$F" "$HOME/Downloads/Harp/app-release.apk" && ok "APK copied to ~/Downloads/Harp/app-release.apk"
  fi
fi

echo; echo "=== Summary"
printf '%s\n' "${SUMMARY[@]}"
