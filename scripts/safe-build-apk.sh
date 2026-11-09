#!/bin/bash
# Builds Harp's Android APK on this laptop without overheating it.
#
# This laptop has frozen under heavy builds (CPU package near 97 °C, a kernel fault
# logged), usually when more than one build was running. So this script:
#   1. waits until the machine is quiet (load average under 3) and the CPU has cooled;
#   2. runs the build at low priority in its own cgroup with a memory cap;
#   3. FREEZES the whole build if the CPU reaches 90 °C (or memory runs low) and
#      resumes it once it is back under 78 °C, instead of letting the machine crash.
# Usage: scripts/safe-build-apk.sh [max-wait-minutes]   (default 30)
set -u
cd "$(dirname "$0")/../app"
export PATH="$HOME/development/flutter/bin:$PATH" ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
UNIT=harp-build-$$
MAXWAIT=${1:-30}
HOT=90 COOL=78 START_BELOW=72
LOG=${HARP_BUILD_LOG:-$HOME/harp-local-build.log}

temp() { local m=0 t; for z in /sys/class/thermal/thermal_zone*/temp; do t=$(( $(cat "$z" 2>/dev/null || echo 0) / 1000 )); [ "$t" -gt "$m" ] && m=$t; done; echo "$m"; }
avail_mb() { awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo; }
# Idle Gradle daemons linger for hours, so "quiet" means low load, not "no Gradle process".
load1() { awk '{printf "%d", $1 * 10}' /proc/loadavg; }   # load average x10

echo "waiting for a quiet, cool machine (up to ${MAXWAIT} min)..." | tee "$LOG"
deadline=$(( $(date +%s) + MAXWAIT*60 ))
while :; do
  l=$(load1); t=$(temp); a=$(avail_mb)
  if [ "$l" -lt 30 ] && [ "$t" -lt "$START_BELOW" ] && [ "$a" -gt 4000 ]; then break; fi
  [ "$(date +%s)" -gt "$deadline" ] && { echo "gave up waiting: load $((l/10)).$((l%10)), ${t}°C, ${a} MB free" | tee -a "$LOG"; exit 3; }
  sleep 10
done
echo "starting at $(temp)°C, $(avail_mb) MB free" | tee -a "$LOG"

systemd-run --user --scope --quiet --unit="$UNIT" -p MemoryMax=9G -p MemorySwapMax=2G \
  nice -n 15 ionice -c3 flutter build apk --release --target-platform android-arm64 >>"$LOG" 2>&1 &
BUILD=$!
sleep 3
frozen=0
while kill -0 "$BUILD" 2>/dev/null; do
  t=$(temp); a=$(avail_mb)
  if [ "$frozen" -eq 0 ] && { [ "$t" -ge "$HOT" ] || [ "$a" -lt 900 ]; }; then
    systemctl --user freeze "$UNIT.scope" 2>/dev/null && frozen=1 && echo "[$(date +%T)] FROZEN the build: ${t}°C, ${a} MB free" | tee -a "$LOG"
  elif [ "$frozen" -eq 1 ] && [ "$t" -le "$COOL" ] && [ "$a" -gt 2000 ]; then
    systemctl --user thaw "$UNIT.scope" 2>/dev/null && frozen=0 && echo "[$(date +%T)] resumed at ${t}°C" | tee -a "$LOG"
  fi
  sleep 3
done
wait "$BUILD"; rc=$?
echo "build exited with $rc (peak check ended at $(temp)°C)" | tee -a "$LOG"
exit "$rc"
