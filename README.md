# Harp

**A music app that learns your taste on your own device.** No account, no sign-in, no API keys and
no servers of ours. For Android, Linux, Windows and macOS.

Harp streams from YouTube Music (through [yt-dlp](https://github.com/yt-dlp/yt-dlp)), plays your
own files, and builds its recommendations from what you listen to, right on the device.

## What it does

- **Listen:** search songs, albums, artists and podcasts; Smart Shuffle and moods; charts and genres;
  new releases from artists you follow; your own music files; downloads for offline listening.
- **Sound:** gapless playback or crossfade, equalizer presets and a 10-band equalizer, headphone
  correction for ~9,000 measured models ([AutoEq](https://github.com/jaakkopasanen/AutoEq)), even
  volume (EBU R128), spatial modes, playback speed, sleep timer.
- **Lyrics:** synced lyrics from several free sources, with Hindi, Urdu, Punjabi and other Indian
  scripts shown in English letters if you like.
- **Library:** playlists with folders, pins, covers, sorting, Smart Shuffle and recommendations;
  liked songs, saved albums and podcasts, followed artists. Import from Spotify, Apple Music links,
  YouTube and playlist files; export as CSV, M3U or JSON; back up and restore.
- **Together:** listen in sync with friends, sync your library between your own devices, and share
  playlists that stay up to date. All peer to peer over [iroh](https://github.com/n0-computer/iroh).
- **Everywhere:** notification and lock-screen controls, Android Auto, a home-screen widget, desktop
  media keys and a mini player.

## Install

Download the latest release from **[Releases](../../releases)**:

| Platform | File | Notes |
| --- | --- | --- |
| Android 9+ (64-bit) | `harp-android-arm64.apk` | Signed with Harp's release key. Allow installing from your browser or file manager. |
| Linux (x64) | `harp-linux-x64.tar.gz` | Unpack and run `harp`, or use `scripts/install-linux.sh` for a menu entry. |
| Windows (x64) | `harp-windows-x64.zip` | Unzip and run `harp.exe`. |
| macOS | `harp-macos.zip` | Not notarized: right-click › Open the first time. |

`SHA256SUMS.txt` in each release lists the files' checksums. Harp checks for new releases itself
(Settings › About).

## Build from source

You need Flutter 3.47+, Rust (stable), and for Android the Android SDK and NDK.

```sh
cargo test --workspace                 # the Rust core
cd app && flutter test                 # the app
flutter build linux --release          # or: windows, macos
scripts/android-tools.sh && flutter build apk --release --target-platform android-arm64
```

`scripts/ci-local.sh` runs everything (tests, builds, APK checks) with a memory guard, for laptops.

## How it's built

- `crates/harp-core`: songs, queue, the taste model and recommender, playlists, library, backup, lyrics,
  transliteration (Rust).
- `crates/harp-ytdlp`: yt-dlp, YouTube Music's InnerTube API, lyrics sources, AutoEq, imports.
- `crates/harp-party`: listening together, device sync and shared playlists over iroh.
- `crates/harp-ffi`: the bridge to the app ([flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge)).
- `app/`: the Flutter app; audio through [mpv_audio_kit](https://pub.dev/packages/mpv_audio_kit) (libmpv + FFmpeg).

`ROADMAP.md` tracks what's done and what's next (including an iOS plan); `UPDATES.md` is the changelog.

## Privacy

Your listening history, taste and library stay on your devices. Harp talks to YouTube Music (search and
streams), the lyrics sources, GitHub (yt-dlp, AutoEq, update checks) and, only when you use them, iroh's
relays to reach your other devices or friends. There is no Harp account and no analytics.

## Legal

Streaming from YouTube with yt-dlp may go against YouTube's terms of service, and Harp is not affiliated
with YouTube, Google, Spotify or Apple. Harp is distributed here and for side-loading, not on Google Play
or the App Store.

## License

Harp is free software under the **GNU General Public License v3.0 or later** (see `LICENSE`).

It builds on: yt-dlp (Unlicense), Python (PSF), QuickJS (MIT), mpv_audio_kit (BSD-3-Clause) with libmpv
(LGPL-2.1+) and FFmpeg (LGPL), iroh (MIT/Apache-2.0), flutter_rust_bridge (MIT/Apache-2.0), AutoEq data
(MIT), and the Flutter and Rust ecosystems (see each package's license).
