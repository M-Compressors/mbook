#!/usr/bin/env bash
# Builds the Android application of mbook (mbook-ui docs, "Guide").
#
#   android.sh clippy           lints mbook-ui and mbook-android for Android
#   android.sh apk [--release]  builds, packs and signs the APK
#   android.sh version-code V   prints the versionCode of the version V
#
# The APK is packed with the tools of the Android SDK, without Gradle: the
# library of this crate, the Java activities of gpui-mobile and of
# `android/java` compiled to classes.dex, the manifest and resources in
# `android/`, and the emoji font of gpui-mobile. It is written to
# target/android/.
#
# Needs:
#   ANDROID_HOME         the Android SDK, with platforms;android-35 and
#                        build-tools (the newest installed one is used)
#   ANDROID_NDK_HOME     the NDK; else ANDROID_NDK_LATEST_HOME, else the
#                        newest under $ANDROID_HOME/ndk
#   javac and java       a JDK 17 or newer, from JAVA_HOME or the PATH
#   jq, zip              on the PATH
#   rustup target add aarch64-linux-android
#
# Signing, for `apk`:
#   MBOOK_APK_KEYSTORE           a keystore file (PKCS #12 or JKS)
#   MBOOK_APK_KEYSTORE_PASSWORD  its password, which is also the key's
#   MBOOK_APK_KEY_ALIAS          the alias of the key; `mbook` by default
# Without MBOOK_APK_KEYSTORE the APK is signed with a debug key made once in
# target/android/debug.keystore; such an APK cannot update one signed with
# the release key.
set -euo pipefail

TARGET=aarch64-linux-android
ABI=arm64-v8a
# The API level of the manifest's minSdkVersion: the C code is compiled for it.
MIN_API=26
# The platform the manifest's targetSdkVersion names; its android.jar
# compiles the Java activities and links the resources.
PLATFORM=android-35

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="$repo/target/android"

die() {
  echo "android.sh: $*" >&2
  exit 1
}

# The tools of the SDK and the NDK, and the variables Cargo and the build
# scripts of the dependencies read: the C compiler and linker of the NDK, and
# android.jar and d8 for the Java helper of robius-share.
setup() {
  [ -n "${ANDROID_HOME:-}" ] || die "set ANDROID_HOME to the Android SDK"
  android_jar="$ANDROID_HOME/platforms/$PLATFORM/android.jar"
  [ -f "$android_jar" ] || die "install $PLATFORM: sdkmanager \"platforms;$PLATFORM\""
  build_tools="$(find "$ANDROID_HOME/build-tools" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1)"
  [ -n "$build_tools" ] || die "install build-tools: sdkmanager \"build-tools;35.0.0\""

  local ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-}}"
  if [ -z "$ndk" ]; then
    ndk="$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1)"
  fi
  [ -n "$ndk" ] && [ -d "$ndk" ] || die "set ANDROID_NDK_HOME to the Android NDK"
  ndk_bin="$(find "$ndk/toolchains/llvm/prebuilt" -mindepth 2 -maxdepth 2 -type d -name bin | head -n 1)"
  [ -n "$ndk_bin" ] || die "no LLVM toolchain in $ndk"

  export ANDROID_NDK_HOME="$ndk"
  export CC_aarch64_linux_android="$ndk_bin/aarch64-linux-android$MIN_API-clang"
  export CXX_aarch64_linux_android="$ndk_bin/aarch64-linux-android$MIN_API-clang++"
  export AR_aarch64_linux_android="$ndk_bin/llvm-ar"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
  export ANDROID_JAR="$android_jar"
  export ANDROID_D8_JAR="$build_tools/lib/d8.jar"

  if [ -n "${JAVA_HOME:-}" ]; then
    export PATH="$JAVA_HOME/bin:$PATH"
  fi
  command -v javac >/dev/null || die "javac was not found: install a JDK 17 or newer"
}

clippy() {
  setup
  cargo clippy --locked --target "$TARGET" -p mbook-ui -p mbook-android -- -D warnings
  cargo clippy --locked --target "$TARGET" -p mbook-ui --no-default-features -- -D warnings
}

# The folder of the checkout of gpui-mobile that Cargo built against. Its
# example app holds the Java activities and the emoji font the APK needs.
gpui_mobile() {
  cargo metadata --locked --format-version 1 --manifest-path "$repo/Cargo.toml" \
    | jq -r '.packages[] | select(.name == "gpui-pre-mobile") | .manifest_path' \
    | xargs dirname
}

apk() {
  local profile=pre-release
  if [ "${1:-}" = --release ]; then
    profile=release
  fi
  setup
  command -v jq >/dev/null || die "jq was not found"
  command -v zip >/dev/null || die "zip was not found"

  cargo build --locked --target "$TARGET" -p mbook-android --profile "$profile"
  local library="$repo/target/$TARGET/$profile/libmbook_android.so"

  local version
  version="$(cargo metadata --locked --no-deps --format-version 1 --manifest-path "$repo/Cargo.toml" \
    | jq -r '.packages[] | select(.name == "mbook-android") | .version')"
  local code
  code="$(version_code "$version")"

  local host
  host="$(gpui_mobile)"
  local sources_root="$host/example/android/gradle/app/src/main/java"
  local java="$sources_root/dev/gpui/mobile"
  local emoji="$host/example/android/gradle/app/src/main/assets/fonts/NotoColorEmoji.ttf"
  [ -d "$java" ] || die "no Java activities in $java: has gpui-mobile moved them?"
  [ -f "$emoji" ] || die "no emoji font at $emoji: has gpui-mobile moved it?"

  local work="$out/work"
  rm -rf "$work"
  mkdir -p "$work"/{classes,dex,res,assets/fonts,apk/lib/$ABI}

  # The classes of gpui-mobile that mbook needs: the activity it extends,
  # and those its Rust code calls with mbook's features (the document
  # picker, its helper, platform views); javac compiles what they refer to
  # from the source path. They are not mbook's code, so they are compiled
  # without its warnings or notes: some use deprecated APIs of Android.
  local needed=()
  local class
  for class in GpuiInputActivity GpuiPathPicker GpuiHelper GpuiPlatformView; do
    needed+=("$java/$class.java")
  done
  javac -nowarn -XDsuppressNotes --release 17 -encoding UTF-8 \
    -classpath "$android_jar" -sourcepath "$sources_root" \
    -implicit:class -d "$work/classes" "${needed[@]}"
  # The activities and the provider of mbook, held to every warning.
  local own=()
  local file
  while IFS= read -r -d '' file; do
    own+=("$file")
  done < <(find "$here/android/java" -name '*.java' -print0)
  javac -Xlint:all -Werror --release 17 -encoding UTF-8 \
    -classpath "$android_jar:$work/classes" -d "$work/classes" "${own[@]}"
  find "$work/classes" -name '*.class' -print0 \
    | xargs -0 "$build_tools/d8" --release --min-api "$MIN_API" --lib "$android_jar" --output "$work/dex"

  cp -R "$here/android/res/." "$work/res/"
  mkdir -p "$work/res/mipmap-xxxhdpi"
  cp "$repo/crates/mbook-ui/assets/icon@2x.png" "$work/res/mipmap-xxxhdpi/ic_launcher.png"
  "$build_tools/aapt2" compile --dir "$work/res" -o "$work/res.zip"

  cp -R "$here/android/assets/." "$work/assets/"
  cp "$emoji" "$work/assets/fonts/NotoColorEmoji.ttf"

  sed -e "s/@VERSION_NAME@/$version/" -e "s/@VERSION_CODE@/$code/" \
    "$here/android/AndroidManifest.xml" > "$work/AndroidManifest.xml"
  "$build_tools/aapt2" link -o "$work/unaligned.apk" -I "$android_jar" \
    --manifest "$work/AndroidManifest.xml" -A "$work/assets" "$work/res.zip"

  cp "$work/dex/classes.dex" "$work/apk/"
  "$ndk_bin/llvm-strip" -o "$work/apk/lib/$ABI/libmbook_android.so" "$library"
  (cd "$work/apk" && zip -q -r "$work/unaligned.apk" classes.dex lib)

  # Aligned for pages of 16 KiB, which Android 15 devices may use.
  "$build_tools/zipalign" -f -P 16 4 "$work/unaligned.apk" "$work/aligned.apk"

  local keystore="${MBOOK_APK_KEYSTORE:-}"
  local password="${MBOOK_APK_KEYSTORE_PASSWORD:-}"
  local alias="${MBOOK_APK_KEY_ALIAS:-mbook}"
  if [ -z "$keystore" ]; then
    keystore="$out/debug.keystore"
    password=android
    alias=androiddebugkey
    if [ ! -f "$keystore" ]; then
      keytool -genkeypair -keystore "$keystore" -storetype PKCS12 \
        -storepass "$password" -alias "$alias" -keyalg RSA -keysize 3072 \
        -validity 10000 -dname "CN=Android Debug,O=Android,C=US" >/dev/null
    fi
    echo "android.sh: signing with the debug key $keystore" >&2
  fi
  [ -n "$password" ] || die "set MBOOK_APK_KEYSTORE_PASSWORD"

  local apk="$out/mbook-$version-android-arm64.apk"
  # The password is read from the environment, not from the command line.
  # apksigner loads the native library of its conscrypt, which a JDK 24 or
  # newer warns about unless native access is enabled (-J passes a JVM
  # option).
  MBOOK_APK_PASSWORD="$password" "$build_tools/apksigner" -J-enable-native-access=ALL-UNNAMED sign \
    --ks "$keystore" --ks-key-alias "$alias" \
    --ks-pass env:MBOOK_APK_PASSWORD --key-pass env:MBOOK_APK_PASSWORD \
    --v4-signing-enabled false \
    --out "$apk" "$work/aligned.apk"
  "$build_tools/apksigner" -J-enable-native-access=ALL-UNNAMED verify "$apk"
  rm -rf "$work"
  echo "$apk"
}

# The versionCode of the manifest for a version `major.minor.patch`: a
# number that grows with every release, as Android requires of an update.
version_code() {
  local core="${1%%[-+]*}"
  local major minor patch
  IFS=. read -r major minor patch <<< "$core"
  [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ && "$patch" =~ ^[0-9]+$ ]] \
    || die "the version $1 is not major.minor.patch"
  (( minor < 1000 && patch < 1000 )) || die "the version $1 does not fit a versionCode"
  echo $(( major * 1000000 + minor * 1000 + patch ))
}

case "${1:-}" in
  clippy) clippy ;;
  apk) shift; apk "$@" ;;
  version-code) version_code "$2" ;;
  *) die "usage: android.sh clippy | apk [--release] | version-code VERSION" ;;
esac
