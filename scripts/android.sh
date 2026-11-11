#!/bin/sh
# Compile et installe 555 sur un téléphone Android branché en USB (débogage USB activé).
#   sh scripts/android.sh          → APK optimisé (release), installé et lancé sur le téléphone
#   sh scripts/android.sh debug    → APK de débogage (compile plus vite, bien plus lent à l'usage)
#   sh scripts/android.sh dev      → mode développement (rechargement à chaud)
set -e
export ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
export NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/29.0.14206865}"
TC="$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64"
# OpenSSL (compilé avec l'appli) a besoin des outils d'archive du NDK.
export RANLIB_aarch64_linux_android="$TC/bin/llvm-ranlib" AR_aarch64_linux_android="$TC/bin/llvm-ar"
# QuickJS (rustypipe) : liaisons générées pour Android.
export LIBCLANG_PATH="${LIBCLANG_PATH:-/Library/Developer/CommandLineTools/usr/lib}"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$TC/sysroot"

cd "$(dirname "$0")/../app"
[ -d src-tauri/gen/android ] || npx tauri android init --ci
if [ "$1" = "dev" ]; then
  exec npx tauri android dev
fi
ADB="$ANDROID_HOME/platform-tools/adb"
OUT=src-tauri/gen/android/app/build/outputs/apk/universal
if [ "$1" = "debug" ]; then
  npx tauri android build --debug --apk --target aarch64
  APK="$OUT/debug/app-universal-debug.apk"
else
  # Clé de publication (voir le workflow « Publish a release ») si elle est sur ce Mac : l'APK
  # installé est alors identique à celui des versions publiées. Sinon, clé de test locale.
  KEY="$HOME/.tauri/555-android.jks"
  if [ -f "$KEY" ]; then
    export ANDROID_KEYSTORE_PATH="$KEY" ANDROID_KEYSTORE_PASSWORD="$(cat "$HOME/.tauri/555-android.password")"
  fi
  npx tauri android build --apk --target aarch64
  if [ -f "$KEY" ]; then
    APK="$OUT/release/app-universal-release.apk"
  else
    APK="$OUT/release/app-universal-release-signed.apk"
    BT="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -1)"
    "$BT/zipalign" -f 4 "$OUT/release/app-universal-release-unsigned.apk" "$OUT/release/aligned.apk"
    "$BT/apksigner" sign --ks "$HOME/.android/debug.keystore" --ks-pass pass:android --key-pass pass:android \
      --out "$APK" "$OUT/release/aligned.apk"
  fi
fi
ls -lh "$APK"
"$ADB" install -r "$APK"
"$ADB" shell monkey -p app.yato.player -c android.intent.category.LAUNCHER 1 >/dev/null
