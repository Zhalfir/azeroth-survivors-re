#!/bin/bash
# Usage: run_batch.sh <round-name> <runs-per-class> <map> <difficulty> <class...>
# Plays auto runs per class on the tablet and pulls their combat logs into $BALANCE_DIR/<round>/.
# Every run sets WOW_GPU_MS=1, so the combat log's ticks carry gpu_ms beside cpu_ms (analyze.py gpu50).
# EXTRA_ENV adds lines to android-env.txt, e.g. EXTRA_ENV='WOW_SURVIVORS_TALENTS=17\nWOW_SURVIVORS_TIER=1'.
ROUND=$1; N=$2; MAP=$3; DIFF=$4; shift 4
ADB=/opt/homebrew/share/android-commandlinetools/platform-tools/adb
D=/sdcard/Android/data/local.azeroth.survivors/files
R=$D/benilla-config/Diagnostics/runs
OUT=${BALANCE_DIR:-balance}/$ROUND
mkdir -p "$OUT"
for CLASS in "$@"; do
  printf "WOW_SURVIVORS_AUTO=$CLASS\nWOW_SURVIVORS_MAP=$MAP\nWOW_SURVIVORS_DIFFICULTY=$DIFF\nWOW_GPU_MS=1\n${EXTRA_ENV}\n" > /tmp/env.$$; $ADB push /tmp/env.$$ $D/android-env.txt >/dev/null 2>&1
  $ADB shell "mkdir -p $R; rm -f $R/*.jsonl"
  $ADB shell am force-stop local.azeroth.survivors; sleep 1
  $ADB shell am start -n local.azeroth.survivors/android.app.NativeActivity >/dev/null
  START=$(date +%s)
  # A run is at most 20 min of play (Mythic); allow 26 min per run, then move on.
  while true; do
    ENDS=$($ADB shell "cat $R/*.jsonl 2>/dev/null" | grep -c '"type":"end"')
    [ "$ENDS" -ge "$N" ] && break
    [ $(( $(date +%s) - START )) -gt $(( N * 1560 )) ] && { echo "$CLASS: timeout after $ENDS runs"; break; }
    if ! $ADB shell pidof local.azeroth.survivors >/dev/null; then echo "$CLASS: game not running, restarting"; $ADB shell am start -n local.azeroth.survivors/android.app.NativeActivity >/dev/null; fi
    sleep 20
  done
  $ADB shell am force-stop local.azeroth.survivors
  for F in $($ADB shell ls $R | tr -d '\r'); do $ADB pull "$R/$F" "$OUT/$CLASS-$F" >/dev/null 2>&1; done
  echo "$CLASS done: $(ls "$OUT" | grep -c "^$CLASS-") files, $(( $(date +%s) - START )) s"
done
$ADB shell rm -f $D/android-env.txt
echo "ROUND $ROUND COMPLETE"
