# Android

How the application runs on Android, how its APK is built and released, and
where it differs from the desktop. A sentence with "must" states an
invariant of the code that a test or a review keeps.

| | |
| --- | --- |
| **Package** | `io.github.m_compressors.mbook`, an APK for arm64-v8a |
| **Android** | 8.0 (API 26) and later; built against API 35 |
| **Source** | [`crates/mbook-android`](https://github.com/M-Compressors/mbook/tree/main/crates/mbook-android) |
| **Platform** | [gpui-mobile](https://github.com/longbridge/gpui-mobile), on the gpui-pre engine of GPUI Kit |
| **Installer** | `mbook-vX.Y.Z-android-arm64.apk` on [GitHub Releases](https://github.com/M-Compressors/mbook/releases) |

The Android application is the same library, `mbook_ui`, as on the desktop,
started by a shell of its own. Its screens, layouts and storage are the ones
the other pages of this section describe; this page lists only what is
different.

## Building the APK

`crates/mbook-android/android.sh` builds the library and packs the APK with
the tools of the Android SDK; there is no Gradle project. It needs:

| Tool | Where it comes from |
| --- | --- |
| The Rust target `aarch64-linux-android` | `rustup target add aarch64-linux-android` |
| The Android SDK, with `platforms;android-35` and build-tools | `ANDROID_HOME` |
| The Android NDK | `ANDROID_NDK_HOME`, else `ANDROID_NDK_LATEST_HOME`, else the newest under `$ANDROID_HOME/ndk` |
| A JDK 17 or newer | `JAVA_HOME`, or `javac` on the `PATH` |
| `jq` and `zip` | the `PATH` |

```sh
crates/mbook-android/android.sh clippy          # lint mbook-ui and mbook-android for Android
crates/mbook-android/android.sh apk             # an APK of the pre-release profile
crates/mbook-android/android.sh apk --release   # an APK of the release profile
```

The APK is written to `target/android/mbook-<version>-android-arm64.apk`.
Without a keystore it is signed with a debug key made once in
`target/android/debug.keystore`. It installs with `adb install`, but it
cannot replace an APK signed with another key, nor be replaced by one:
Android then asks for the old one to be removed first, which removes its
library. `MBOOK_APK_KEYSTORE`, `MBOOK_APK_KEYSTORE_PASSWORD` and
`MBOOK_APK_KEY_ALIAS` (by default `mbook`) name the key a release is signed
with.

Continuous integration runs both commands on every push and keeps the APK
of the run as an artifact.

## What the APK holds

| Part | Made from |
| --- | --- |
| `lib/arm64-v8a/libmbook_android.so` | The library of `mbook-android`, built for `aarch64-linux-android` with the C compiler of the NDK for API 26, stripped of its symbols |
| `classes.dex` | The Java activities of the example application of gpui-mobile and `MbookActivity` of `crates/mbook-android/android/java`, compiled with `javac` and `d8` |
| `AndroidManifest.xml`, the theme | `crates/mbook-android/android/`; the version of the workspace goes into `versionName`, and `major × 1,000,000 + minor × 1,000 + patch` into `versionCode` |
| The launcher icon | `crates/mbook-ui/assets/icon@2x.png` |
| `assets/fonts/NotoColorEmoji.ttf` | The emoji font of the example application of gpui-mobile, with its licence beside it |

- The Java activities and the font are taken from the checkout of gpui-mobile
  that Cargo built against, found with `cargo metadata`. A later commit of
  gpui-mobile that moves them makes the script stop with a message.
- The activities that need AndroidX are left out: the splash screen,
  biometric sign-in, notifications and the media session. mbook uses none of
  them.
- The emoji font is bundled because gpui-mobile draws colour emoji from
  bitmap glyphs only, and the emoji font of Android 13 and later has vector
  glyphs. It adds about 10 MB.
- The APK is aligned for pages of 16 KiB and signed with the APK Signature
  Schemes v2 and v3.

## The shell

`mbook-android` is a library of type `cdylib`. The activity of the APK,
`MbookActivity`, is the `GpuiInputActivity` of gpui-mobile, a
`NativeActivity` that loads it and gives the keyboard of the system an
input connection, with the documents other apps hand over. `android-activity`
then calls `android_main` on a thread of its own, which:

1. sends `log` records to logcat under the tag `mbook`, and a panic too;
2. creates the platform of gpui-mobile;
3. builds the roots from the files of the application;
4. calls `mbook_ui::run` with an `Application` on that platform.

`android_main` must keep its unmangled name: `android-activity` finds it by
that name. The shell sets `mimalloc3` as its allocator, as the desktop
binary does.

The manifest names the configuration changes the activity handles itself:
rotation, size, keyboard, navigation, density and dark mode. Android
therefore keeps the activity across them, and gpui-mobile adapts its window
in place.

## Opening a document from another app

The manifest offers mbook for viewing and for sharing documents of the
types of the formats it converts, and of `application/octet-stream`, the
type an mbook file comes with ([Opening files from the system](architecture.md#opening-files-from-the-system)).

- A document comes as a `content:` URI that only the activity may read.
  `MbookActivity` copies it into `<cache>/opened/<n>/` under the name the
  provider shows, on a thread of its own, and queues the path. A name
  without an extension gets the one of the document's MIME type, from which
  the importer reads the format.
- Nothing wakes the interface for it: a task of the interface asks the
  activity for the queued paths once a second, through JNI, and adds them as
  dropped files are. The copy in the cache stays; Android may remove it when
  it needs room.
- A recreated activity does not take its old intent again.

## Where it differs from the desktop

| Part | On Android |
| --- | --- |
| Engine | gpui-pre instead of GPUI Fast; see [Crate and workspace](architecture.md#crate-and-workspace) |
| Roots | `<files>/mbook/` for the data and `<files>/cache/` for the cache, where `<files>` is the private folder of the application. Uninstalling the application removes its library. |
| Adding a book, choosing a picture | The document picker of the system. It copies each chosen document into the cache of the application and returns the copies; it has no filter, so a file of another kind is refused after it is chosen. |
| Export | Has no save dialog: the converted copy goes to the share sheet, whose targets include saving to a folder ([Export](architecture.md#export)). |
| Share | Through `MbookActivity.share`, which lends the file with `MbookFiles`, a read-only content provider for `<cache>/share`, and starts the chooser on the thread of the activity ([Share](architecture.md#share)). |
| Book menu | Has no **Show the file**: the library is in the private storage of the application, which no file manager shows, and gpui-mobile cannot reveal a path. The file leaves through **Share** and **Export**. |
| Updates | None in the application: a newer APK signed with the same key installs over the older one and keeps the library. |
| Header | A plain bar, not the title bar of the Kit: there is no window to move or to minimise. |
| System bars | The window covers the screen. Views keep clear of the bars as gpui-mobile measures them ([Layout](architecture.md#layout)). |

## Limits

- **arm64 only.** No APK is built for 32-bit ARM or for x86-64 emulators.
- **API 35 is the last target the theme works for.** Android 15 draws an
  application that targets API 35 under its bars, and gpui-mobile measures
  the bars from a content view that then covers the whole screen. The theme
  opts out of that, which an application that targets API 36 cannot do.
  Moving on needs gpui-mobile to report the insets of the system through
  GPUI.
- **A recreated activity shows a black screen.** When Android destroys the
  activity and creates it again in the same process, GPUI refuses to start
  on the new thread `android-activity` gives it. The configuration changes
  the manifest names do not recreate it; others, such as a change of the
  language of the system, do. gpui-mobile has a second entry point that
  avoids this, which needs a host activity of its own.
- **The navigation bar** of gesture navigation is not reported by every
  device: on some, the handle is drawn over the bottom of the library's bar.
- **gpui-mobile is a pinned commit** of an open pull request,
  [longbridge/gpui-mobile#56](https://github.com/longbridge/gpui-mobile/pull/56),
  fetched from the repository; see [Dependencies](design.md#dependencies).
- **Documents are polled.** The interface asks the activity once a second
  for documents it received, for as long as it runs.
- **No tests run on Android.** The code shared with the desktop is tested
  there; what only Android compiles is linted in continuous integration and
  was tried on one phone ([What is implemented](status.md#what-is-implemented)).
