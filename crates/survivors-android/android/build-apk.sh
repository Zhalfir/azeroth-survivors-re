#!/usr/bin/env bash
# Builds Warcraft Survivors for 64-bit ARM Android into `azeroth-survivors.apk`, with no Gradle:
# cargo-ndk builds the .so, aapt2 packages the manifest, zipalign + apksigner (a local debug key)
# finish it. Install and copy the game data with `install.sh`.
#
# Needs: rustup (the repo's toolchain + `aarch64-linux-android`), `cargo install cargo-ndk`, and
# an Android SDK with build-tools, a platform and the NDK at $ANDROID_HOME (or the Homebrew
# android-commandlinetools root). CARGO_TARGET_DIR is honoured: the build is ~6 GB.
#
# APK_FEATURES=dev adds `--features dev` (the GPU frame meter, `WOW_GPU_MS=1`, lives behind it);
# unset, the build is the shipping one. Balance APKs use it so the combat log carries gpu_ms.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
export ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"
BT="$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)"
JAR="$(ls -d "$ANDROID_HOME"/platforms/android-* | sort -V | tail -1)/android.jar"
PROFILE="${PROFILE:-release}"
OUT="${OUT:-$ROOT/azeroth-survivors.apk}"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cd "$ROOT"
FEATURES=()
[ -z "${APK_FEATURES:-}" ] || FEATURES=(--features "$APK_FEATURES")
cargo ndk -t arm64-v8a -P 26 -o "$STAGE/lib" build --profile "$PROFILE" -p survivors-android ${FEATURES[@]+"${FEATURES[@]}"}

# libc++_shared.so: `android_shared_stdcxx` links it, and cargo-ndk copies it only sometimes.
CXX="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so"
[ -f "$STAGE/lib/arm64-v8a/libc++_shared.so" ] || cp "$CXX" "$STAGE/lib/arm64-v8a/"

"$BT/aapt2" compile --dir "$HERE/res" -o "$STAGE/res.zip"
"$BT/aapt2" link -o "$STAGE/base.apk" -I "$JAR" --manifest "$HERE/AndroidManifest.xml" "$STAGE/res.zip"
(cd "$STAGE" && zip -q -r base.apk lib)
"$BT/zipalign" -P 16 -f 4 "$STAGE/base.apk" "$STAGE/aligned.apk"

KEY="${DEBUG_KEYSTORE:-$HOME/.android/debug.keystore}"
if [ ! -f "$KEY" ]; then
    mkdir -p "$(dirname "$KEY")"
    keytool -genkeypair -keystore "$KEY" -storepass android -keypass android -alias androiddebugkey \
        -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US" >/dev/null
fi
"$BT/apksigner" sign --ks "$KEY" --ks-pass pass:android --key-pass pass:android --out "$OUT" "$STAGE/aligned.apk"
echo "built $OUT ($(du -h "$OUT" | cut -f1))"
