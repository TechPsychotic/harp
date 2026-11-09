#!/bin/bash
# Builds Harp (the Android APK, or the Linux app) on this laptop without running it out
# of memory.
#
# This laptop has frozen under heavy builds, usually with several running at once
# (a Gradle daemon alone holds 3-4 GB). So this script:
#   1. waits until enough RAM is free for the build (default 6 GB available);
#   2. runs the build at low priority in its own cgroup with a hard memory cap, so if
#      anything goes wrong the kernel stops *this build* instead of freezing the system;
#   3. FREEZES the whole build when the machine starts to thrash (memory pressure high
#      or free memory low) and resumes it when things settle.
# CPU temperature is deliberately ignored.
# Usage: scripts/safe-build-apk.sh [max-wait-minutes] [apk|linux]   (defaults: 30, apk)
set -u
cd "$(dirname "$0")/../app"
export PATH="$HOME/development/flutter/bin:$PATH" ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
# libmpv for the Linux build, if it was extracted locally (no root needed).
if [ -d "$HOME/.local/mpv/usr/lib64/pkgconfig" ]; then
  export PKG_CONFIG_PATH="$HOME/.local/mpv/usr/lib64/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  export LD_LIBRARY_PATH="$HOME/.local/mpv/usr/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
UNIT=harp-build-$$
MAXWAIT=${1:-30}
TARGET=${2:-apk}
NEED_MB=${HARP_NEED_MB:-6000}      # available memory needed to start
CAP_GB=${HARP_CAP_GB:-6}           # the build may not use more than this (RAM)
FREEZE_BELOW_MB=1500               # freeze if available memory falls under this...
THAW_ABOVE_MB=2800                 # ...and resume above this
PSI_FREEZE=25                      # or if tasks spend this % of the last 10 s stalled on memory
LOG=${HARP_BUILD_LOG:-$HOME/harp-local-build.log}

avail_mb() { awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo; }
# "some" memory pressure, avg10, as a whole number: share of time tasks waited for memory
psi() { awk '/^some/ {split($2,a,"="); printf "%d", a[2]}' /proc/pressure/memory 2>/dev/null || echo 0; }

echo "waiting for ${NEED_MB} MB of free memory (up to ${MAXWAIT} min)..." | tee "$LOG"
deadline=$(( $(date +%s) + MAXWAIT*60 ))
while :; do
  a=$(avail_mb); p=$(psi)
  if [ "$a" -ge "$NEED_MB" ] && [ "$p" -lt 5 ]; then break; fi
  [ "$(date +%s)" -gt "$deadline" ] && { echo "gave up waiting: ${a} MB available, memory pressure ${p}%" | tee -a "$LOG"; exit 3; }
  sleep 10
done
echo "starting with $(avail_mb) MB available" | tee -a "$LOG"

systemd-run --user --scope --quiet --unit="$UNIT" \
  -p MemoryHigh=$((CAP_GB-1))G -p MemoryMax=${CAP_GB}G -p MemorySwapMax=1G \
  nice -n 15 ionice -c3 bash -c 'if [ "$0" = linux ]; then flutter build linux --release; else flutter build apk --release --target-platform android-arm64; fi' "$TARGET" >>"$LOG" 2>&1 &
BUILD=$!
sleep 3
frozen=0 low=999999
while kill -0 "$BUILD" 2>/dev/null; do
  a=$(avail_mb); p=$(psi); [ "$a" -lt "$low" ] && low=$a
  if [ "$frozen" -eq 0 ] && { [ "$a" -lt "$FREEZE_BELOW_MB" ] || [ "$p" -ge "$PSI_FREEZE" ]; }; then
    systemctl --user freeze "$UNIT.scope" 2>/dev/null && frozen=1 && echo "[$(date +%T)] FROZEN the build: ${a} MB available, memory pressure ${p}%" | tee -a "$LOG"
  elif [ "$frozen" -eq 1 ] && [ "$a" -ge "$THAW_ABOVE_MB" ] && [ "$p" -lt 5 ]; then
    systemctl --user thaw "$UNIT.scope" 2>/dev/null && frozen=0 && echo "[$(date +%T)] resumed: ${a} MB available" | tee -a "$LOG"
  fi
  sleep 2
done
wait "$BUILD"; rc=$?
echo "build exited with $rc (lowest available memory during the build: ${low} MB)" | tee -a "$LOG"
# Gradle leaves a daemon holding a few GB after an APK build; the next build's
# memory check then waits for memory that never frees. Stop it now.
if [ "$TARGET" = apk ]; then (cd android && timeout 60 ./gradlew --stop >/dev/null 2>&1) || true; fi
exit "$rc"
