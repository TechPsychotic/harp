<p align="center">
  <img src="docs/assets/icon.png" alt="Harp logo" width="128" height="128">
</p>

<h1 align="center">Harp</h1>

<p align="center">
  <a href="https://techpsychotic.github.io/harp/">Website</a> ·
  <a href="https://github.com/TechPsychotic/harp/releases/latest">Download</a> ·
  <a href="LICENSE">GPL-3.0</a>
</p>

**A music app that learns your taste on your own device.** No account, no sign-in, no API keys and
no servers of ours. For Android, iPhone, Linux, Windows and macOS.

Harp streams music from the web (through [yt-dlp](https://github.com/yt-dlp/yt-dlp)), plays your
own files, and builds its recommendations from what you listen to, right on the device.

**Website:** https://techpsychotic.github.io/harp/ · **Download:** [latest release](../../releases/latest)

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
  other public playlist links and playlist files; export as CSV, M3U or JSON; back up and restore.
- **Together:** listen in sync with friends, sync your library between your own devices, and share
  playlists that stay up to date. All peer to peer over [iroh](https://github.com/n0-computer/iroh).
- **Everywhere:** notification and lock-screen controls, Android Auto, a home-screen widget, desktop
  media keys and a mini player.

## Install

Download the latest release from **[Releases](../../releases)**:

| Platform | File | Notes |
| --- | --- | --- |
| Android 9+ (64-bit) | `harp-android-arm64.apk` | Signed with Harp's release key. Allow installing from your browser or file manager. |
| iPhone / iPad (iOS 15+) | `harp-ios-unsigned.ipa` | Unsigned: you sign it with your own Apple ID when you install it. See below. |
| Linux (x64) | `harp-linux-x64.tar.gz` | Unpack and run `harp`, or use `scripts/install-linux.sh` for a menu entry. |
| Windows (x64) | `harp-windows-x64.zip` | Unzip and run `harp.exe`. |
| macOS | `harp-macos.zip` | Not notarized: right-click › Open the first time. |

`SHA256SUMS.txt` in each release lists the files' checksums. Harp checks for new releases itself
(Settings › About).

### iPhone and iPad (no App Store)

Harp isn't on the App Store, and the `.ipa` isn't signed by a developer account. Install it with a tool
that signs it with **your own (free) Apple ID**:

- **[AltStore](https://altstore.io) or [SideStore](https://sidestore.io):** install the tool, then open
  `harp-ios-unsigned.ipa` with it. With a free Apple ID the app must be refreshed every 7 days; both
  tools do that in the background (SideStore without a computer). A free Apple ID allows 3 sideloaded
  apps at a time.
- **[Sideloadly](https://sideloadly.io)** (Windows or Mac): connect the iPhone by cable, drag the `.ipa`
  in, sign in with your Apple ID. Re-install every 7 days with a free Apple ID.
- **[TrollStore](https://github.com/opa334/TrollStore)** (only on the iOS versions it supports): installs
  the `.ipa` permanently, no re-signing.

After the first install, iOS asks you to trust your Apple ID: Settings › General › VPN & Device
Management. On iOS 16+, also turn on Developer Mode (Settings › Privacy & Security).

On iOS, Harp finds streams with its own built-in finder (iOS doesn't let apps run yt-dlp), so if the
source changes how it answers, an update may be needed sooner than on other platforms.

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
- `crates/harp-ytdlp`: yt-dlp, the music catalogue (search, artists, albums, radio), lyrics sources, AutoEq, imports.
- `crates/harp-party`: listening together, device sync and shared playlists over iroh.
- `crates/harp-ffi`: the bridge to the app ([flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge)).
- `app/`: the Flutter app; audio through [mpv_audio_kit](https://pub.dev/packages/mpv_audio_kit) (libmpv + FFmpeg).

`UPDATES.md` is the changelog.
The website lives in `docs/` (GitHub Pages).

## Contributing

Bug reports, ideas and pull requests are welcome: see [`CONTRIBUTING.md`](CONTRIBUTING.md). Error codes
(H-101 and so on) are explained in [`ERRORS.md`](ERRORS.md). Security
issues: see [`SECURITY.md`](SECURITY.md).

## Privacy

Your listening history, taste and library stay on your devices. Harp talks to its music source (search and
streams), the lyrics sources, GitHub (yt-dlp, AutoEq, update checks) and, only when you use them, iroh's
relays to reach your other devices or friends. There is no Harp account and no analytics.

## Legal

Streaming through yt-dlp may go against the terms of service of the sites it fetches from, and Harp is
not affiliated with any music service, Spotify or Apple. Harp is distributed here and for side-loading, not on Google Play
or the App Store.

## License

Harp is free software under the **GNU General Public License v3.0 or later** (see `LICENSE`).

It builds on: yt-dlp (Unlicense), Python (PSF), QuickJS (MIT), mpv_audio_kit (BSD-3-Clause) with libmpv
(LGPL-2.1+) and FFmpeg (LGPL), iroh (MIT/Apache-2.0), flutter_rust_bridge (MIT/Apache-2.0), AutoEq data
(MIT), and the Flutter and Rust ecosystems (see each package's license).

## Inspiration and thanks

Projects and work that shaped Harp, beyond the libraries it's built on:

- [yt-dlp](https://github.com/yt-dlp/yt-dlp): streaming, and its research into how the source's players work.
- [NewPipe Extractor](https://github.com/TeamNewPipe/NewPipeExtractor): how a music catalogue is read without an account.
- [AutoEq](https://github.com/jaakkopasanen/AutoEq): the measured headphone corrections behind headphone correction.
- [iroh](https://github.com/n0-computer/iroh): peer-to-peer connections for listening together, device sync and shared playlists.
- [LRCLIB](https://github.com/tranxuanthang/lrclib): free synced lyrics.
- [Better Lyrics](https://github.com/better-lyrics) and [YouLy+](https://github.com/ibratabian17/youlyplus): ideas for word-synced lyrics.
- [SwiftyTube](https://github.com/686udjie/SwiftyTube): ideas for an iOS version without yt-dlp.
- [Equalizer314](https://f-droid.org/en/packages/com.bearinmind.equalizer314/), [Steam Audio](https://github.com/ValveSoftware/steam-audio) and [OpenAL-MOB](https://github.com/6779660/OpenAL-MOB): research for the sound work and the planned 3D (HRTF) mode.
