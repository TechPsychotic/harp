# Contributing to Harp

Thanks for wanting to help. Harp is a music app with no accounts and no servers of its own, and
contributions should keep it that way: nothing that sends listening data anywhere, no analytics, no
sign-ins.

## Reporting a problem

Open an issue with:

- what you did, what you expected, and what happened instead;
- your platform and Harp version (Settings › About);
- on Android, the crash report if Harp closed on its own (Settings › About › Last crash report);
- for playback problems, the result of Settings › About › Test the audio engine.

## Building

You need Flutter 3.47+, Rust (stable), and for Android the Android SDK and NDK.

```sh
cargo test --workspace                 # the Rust core
cd app && flutter test                 # the app
flutter build linux --release          # or: windows, macos
scripts/android-tools.sh && flutter build apk --release --target-platform android-arm64
```

`scripts/ci-local.sh` runs the tests, the builds and the APK checks in one go, with a memory guard
for laptops.

After changing anything in `crates/harp-ffi/src/api/`, regenerate the Dart bindings:

```sh
cd app && flutter_rust_bridge_codegen generate
```

## Pull requests

- Keep changes focused; one topic per pull request.
- Add or update tests for behaviour in the Rust core (`cargo test --workspace` must pass) and run
  `flutter analyze` in `app/`.
- Add a short entry to `UPDATES.md` saying what changed and how it was tested.
- By contributing you agree that your work is released under the project's license (GPL-3.0 or later).

## Layout

- `crates/harp-core`: songs, queue, taste model and recommender, playlists, library, backup, lyrics.
- `crates/harp-ytdlp`: yt-dlp, YouTube Music's InnerTube API, lyrics sources, AutoEq, imports.
- `crates/harp-party`: listening together, device sync and shared playlists over iroh.
- `crates/harp-ffi`: the bridge to the Flutter app.
- `app/`: the Flutter app.
- `docs/`: the website.
