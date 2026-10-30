#!/usr/bin/env bash
# Installs the APK on the phone adb sees and copies the WoW 1.12.1 `Data/` MPQs beside it, into
# the app's own external folder (no storage permission needed). The copy is skipped for MPQs the
# phone already has at the same size. Usage: install.sh /path/to/Data [apk]
set -euo pipefail

DATA="${1:?usage: install.sh /path/to/WoW/Data [apk]}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APK="${2:-$(cd "$HERE/../../.." && pwd)/azeroth-survivors.apk}"
PKG=local.azeroth.survivors
ADB="${ADB:-adb}"
DEST="/sdcard/Android/data/$PKG/files/Data"

"$ADB" install -r "$APK"
# The app must create its own external folder first (NativeActivity does, via getExternalFilesDir):
# one made by `adb shell` belongs to the shell user, and the app is denied listing it.
"$ADB" shell am start -W -n "$PKG/android.app.NativeActivity" >/dev/null
sleep 3
"$ADB" shell am force-stop "$PKG"
"$ADB" shell mkdir -p "$DEST"
for f in "$DATA"/*.MPQ "$DATA"/*.mpq; do
    [ -f "$f" ] || continue
    name="$(basename "$f")"
    have="$("$ADB" shell stat -c %s "$DEST/$name" 2>/dev/null | tr -d '\r' || true)"
    want="$(stat -f %z "$f" 2>/dev/null || stat -c %s "$f")"
    if [ "$have" = "$want" ]; then
        echo "have $name"
    else
        echo "push $name ($(du -h "$f" | cut -f1))"
        "$ADB" push "$f" "$DEST/$name" >/dev/null
    fi
done
echo "done — launch \"Azeroth Survivors\" on the phone; logs: $ADB logcat -s RustStdoutStderr:V bevy:V"
