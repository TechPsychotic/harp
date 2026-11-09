# Harp: roadmap

**Harp** is a standalone music app for Linux, Windows, macOS and Android: search and stream
through YouTube Music (via yt-dlp and YouTube Music's public web API), a library that lives on
the device, a recommender that learns your taste **on the device**, and bringing your playlists
over from other services. No accounts of ours, no servers of ours.

This file is the single place that tracks **everything you asked for and reported**, what is
done, what is not, what is impossible (and why), and the **known bugs and weak spots** in both
the idea and the code. `UPDATES.md` is the dated log of changes; this file is the current state.

> Streaming from YouTube with yt-dlp goes against YouTube's terms of service. It is widely
> done (NewPipe, ViMusic, InnerTune, …) but rules out Google Play: F-Droid and our own site.

*Last reviewed: 2026-11-09.*

**Status legend**

| Mark | Meaning |
|---|---|
| ✅ | Done and checked (by tests, on the laptop, or against the live service) |
| 🟡 | Done in code, **not yet confirmed on your phone / laptop** |
| 🔧 | In progress (part of it is built) |
| ⬜ | Not started |
| ⛔ | Not possible as asked (the reason is given) |

## Principles

- **No accounts, no servers of ours.** Searches and streams go from the device; the library
  and the taste model live on the device. Signing in to *your* Spotify account to bring your
  playlists over is optional and goes straight from your device to Spotify.
- **Sources are plugins.** YouTube Music first; local files and other yt-dlp sites later.
- **One core on every platform.** Rust core (search, resolving, cache, library, queue,
  recommender), Flutter UI.
- **Honest status.** Nothing is called done until it has been checked, and what could not be
  checked is said.

---

## 1. Tracker: everything you reported or asked for

### 1a. Problems you reported (23)

| # | Problem | Status | What was done / what is left |
|---|---|---|---|
| B1 | Artists shown as "Unknown" | ✅ | Search moved to YouTube Music's own API (artist, album, length, cover). Checked live. |
| B2 | Laptop crashes while building | 🟡 | Heavy builds moved to GitHub Actions; local builds go through `scripts/safe-build-apk.sh` (waits for free RAM, 6 GB cap, freezes on memory pressure; one build ran with ≥5.6 GB free). Root cause not fixed: a kernel fault on 6.17.1, most likely with several builds at once. |
| B3 | App feels laggy (reported 3×) | 🟡 | Per-tick rebuilds removed, blur limited, background throttled, recommender batched, covers moved off the Rust thread pool, lighter background by default. **Never measured on your devices.** Settings › Performance › *Show frame times* gives numbers. |
| B4 | Not recognised as a music app on Linux | 🟡 | Desktop entry with music categories, audio types, icons, window class ✅; MPRIS media controls added, not confirmed in GNOME. |
| B5 | Settings / downloads / playlists not visible | ✅ | Settings is a main tab; Library has Liked / Playlists / Downloads / History. |
| B6 | Inconsistent UI (reported 3×) | 🟡 | One design system (buttons, dialogs, menus, titles, chips, empty states). Not reviewed on a phone. The full redesign (F26) is still to do. |
| B7 | Not shown as a music service on Android | 🟡 | Media service, media-button receiver, APP_MUSIC category; all confirmed in the APK's manifest. Not confirmed on the phone. |
| B8 | Any settings change stalls playback | 🟡 | Audio filters are no longer swapped mid-song: the song reopens at the same position. Not confirmed. |
| B9 | No play next / queue / playlist on long-press of home cards | ✅ | Long-press sheet on every song card and row. |
| B10 | Spatial audio silent on Linux, hangs Android | 🟡 | Android: turned off (its libmpv stalls with filters). Linux: the filter chains run in libmpv (checked), audibility not confirmed. |
| B11 | Some songs fail with a yt-dlp error (502 / tunnel) | 🟡 | Proxy now tries every address (IPv4 first) and network errors retry. Not confirmed on the phone. |
| B12 | Stops playing when the screen is off for long | 🟡 | Foreground media service, wake lock and Wi-Fi lock; next songs prefetched. Not confirmed. |
| B13 | Late response; skipped songs load before / over the current one | 🟡 | Playback logic rebuilt: numbered loads, stale ones dropped, rapid skips coalesced, false end-of-file ignored. Logic tested; feel not confirmed. |
| B14 | Imports stop at 100 songs | 🔧 | YouTube / YouTube Music: ✅ fixed (paging; a 150+ song playlist read in full, live). Spotify **link**: Spotify's public page itself stops at 100 (⛔ without signing in). Spotify **account** connection like Exportify: 🔧 core built and tested, screen not yet wired. |
| B15 | The app crashed on the phone | 🟡 | Likely cause found and fixed: the media-notification icon was stripped from the APK by the resource shrinker (confirmed absent, now confirmed present). Crash reporter added (Settings › About › Last crash report). Not confirmed that it was the whole cause. |
| B16 | No Spotify-style controls in the notification | 🟡 | Same cause as B15. Not confirmed. |
| B17 | Two settings entries (Home header + tab) | ✅ | The Home gear is gone. |
| B19 | Search history saved every keystroke | ✅ | History is saved only when you press search / enter or open a result (song, album, artist, podcast), like Spotify. Typing still searches as you go, but saves nothing. |
| B20 | The mini player disappears inside a playlist (or any opened page) | 🟡 | Each tab now has its own page stack, so playlists, albums and lists open inside the tab with the mini player and tab bar still showing. Back closes the tab's pages first; tapping the open tab goes back to its start. Analyzer clean; not yet tried on the phone. |
| B21 | Doesn't pause when the output changes (earphones out, Bluetooth off) | 🟡 | Android: pauses when the output would switch to the speaker, pauses for calls and resumes after, lowers the volume while navigation speaks. Needs the phone to confirm. Desktop systems don't report this, so not on Linux yet. |
| B22 | Notification sometimes has no progress bar | 🟡 | Cause: songs from search often come without a length, and the notification only draws progress when it knows the length; it was never told the real one. Now the player's length is passed on as soon as it is known, and seeks are reported too. Needs the phone. |
| B23 | Playlists: no picture, wasteful layout, no pinning | 🟡 | Covers (first song, or a 2×2 mosaic from four songs) on Library and Home; Library lists playlists as compact rows; pin to top from the … menu or a long press (pinned first, saved in the database). 🧪 database tests. |
| B18 | Android builds failing on GitHub | 🟡 | GitHub's release downloads returned 504 for hours. Day-to-day CI now runs on the laptop (`scripts/ci-local.sh`, ✅); GitHub Actions only for releases or by hand, with caches and retries. |

**Problems (23): 5 fixed and checked, 17 fixed but waiting for your confirmation, 1 partly fixed.**

### 1b. Features you asked for (49)

| # | Feature | Status | Notes |
|---|---|---|---|
| F1 | An implementation roadmap | ✅ | `IMPLEMENTATION_PLAN.md`, and this file. |
| F2 | Name "Harp", private GitHub repo | ✅ | `TechPsychotic/harp`. |
| F3 | yt-dlp on Android the way zevro does it | 🟡 | Static Python + QuickJS as native libraries, yt-dlp zipapp, loopback proxy, CA bundle. Bundled in the APK (checked); streaming on the phone has failed before (B11). |
| F4 | Glassmorphic look | ✅ | Frosted surfaces, aurora background, accent from the song. |
| F5 | music-recommender's features without sign-in or API keys | ✅ | Graded skip/finish scoring, 4× superlike, skip reasons, genre injection, reason line, resume, history. |
| F6 | Install on the laptop | ✅ | `~/.local/opt/harp` with a launcher and menu entry. |
| F7 | Python and QuickJS inside the APK | ✅ | Checked in the APK. |
| F8 | ytmusicapi-style detailed results | ✅ | Same public calls, in Rust (no Python). |
| F9 | GitHub Actions for heavy work | ✅ | Tests, Linux, Android; Windows/macOS by hand or on tags. |
| F10 | Dark and light themes | ✅ | System / Dark / Light. |
| F11 | Download and offline mode | 🟡 | Built and tested in code; not tried on the phone. |
| F12 | Import playlists from other services | 🔧 | Spotify (link ≤100, data export, Exportify CSV), Apple Music exported text, M3U, YouTube / YouTube Music links (full), pasted lists. Missing: Apple Music **links** (feasible, see F32), Spotify account (F31). |
| F13 | Logo: small cursive "h" with a strike, red gradient | ✅ | App, adaptive (with themed layer) and notification icons. |
| F14 | Background that reacts live to the music | 🟡 | Linux: real loudness from mpv (checked in libmpv). Android: a beat-style imitation. Not a real waveform or spectrum yet. |
| F15 | Less rounded corners; dock at the bottom | ✅ | |
| F16 | Streaming and download quality | ✅ | Low / Medium / High. |
| F17 | Skip songs within the queue | ✅ | Tap a queued song to jump to it. |
| F18 | Better Home: recommended, similar to recent, moods, playlists, precached covers | ✅ | |
| F19 | 3D audio and Dolby Atmos | 🟡 / ⛔ | Virtual 3D (wide, crossfeed, immersive) on desktop. **Atmos: not possible**: it needs a licensed decoder and Atmos-encoded streams; YouTube's music streams are stereo. Android has no spatial mode yet. |
| F20 | Recent search history | ✅ | |
| F21 | Cache songs ahead instead of only streaming | 🟡 | Prefetches the next songs (and the current one) into a size-limited cache. |
| F22 | Playlists and podcasts in search, not just songs | ✅ | Songs, Artists, Albums, Playlists, Podcasts (checked live). Podcast extras (backlog) not done. |
| F23 | Snappier app | 🟡 | See B3. |
| F24 | Recommendations that use your searches | ✅ | "Because you searched for …". |
| F25 | Synced lyrics (SongSync / BetterLyrics style) | 🔧 | Line-synced lyrics from LRCLIB, tap to seek, cached. **Word-by-word (BetterLyrics-style) not done.** |
| F26 | UI like Apple Music, with Apple Music / Spotify features | 🟡 | **Redesigned** (2026-11-09): artwork-coloured theme, plain list rows, big titles, floating mini player over a tab bar (sidebar on wide screens), artwork-backdrop Now Playing with big lyrics, album/artist/playlist pages with headers and Play/Shuffle, Library as categories. Not yet seen on your devices; more Apple/Spotify features remain (see backlog). |
| F27 | Android: edge-to-edge, predictive back, adaptive icons, Live Updates / Now Bar | 🔧 | Edge-to-edge 🟡, predictive back 🟡, adaptive icon ✅. Live Updates / Now Bar: nothing built beyond the standard media notification (which Samsung's Now Bar uses); not confirmed. |
| F28 | Search for artists | ✅ | Artist page with all songs, albums, playlists, similar artists. Checked live. |
| F29 | Keep `UPDATES.md` | ✅ | Updated with every change. |
| F30 | Build the APK on the laptop | ✅ | Memory-guarded script. |
| F31 | Exportify-like tools inside the app | 🟡 | **In the app now** (Library › Bring Your Music Over › From your Spotify account; Settings › Spotify; Export on playlists and Liked Songs). **Core tested:** Spotify sign-in (PKCE, your own Client ID, no secret), all playlists and Liked Songs with no 100-song cap (tested reading 250 songs and waiting out a rate limit), and saving any playlist as Exportify-style CSV, M3U or JSON (round-trip tested). The real Spotify sign-in has not been tried with a real account yet. |
| F32 | The same for Apple Music | 🟡 / ⛔ | Reading *your Apple Music account* needs a MusicKit developer token, which needs a paid Apple Developer membership (⛔ without one). **Public Apple Music playlist links are readable** with no key: the page carries the track list (checked: 50 songs in its structured data, more in its page data). **Link import is now built** (tested on a saved copy of a real page). File exports (text, M3U) already import. *(Earlier I said Apple links could not be read; that was wrong.)* |
| F34 | Search: "All" by default, ranked by relevance | ✅ | All results in YouTube Music's own relevance order: top result, then songs, artists, albums, podcasts (videos, playlists and profiles left out). Tested on real responses. |
| F35 | Remove the Playlists search filter | ✅ | Filters: All, Songs, Albums, Artists, Podcasts. |
| F36 | Hindi, Urdu, Punjabi… lyrics in English letters (Hinglish) | 🟡 | Built into Harp's core (no service, works offline): Devanagari, Gurmukhi, Bengali, Gujarati, Odia, Tamil, Telugu, Kannada, Malayalam, and Urdu approximately. Drops the silent "a" the way people type ("tum hi ho", "aankhein", "dil"). The lyrics view has a switch: original / both / English letters. 🧪 Urdu is rougher than the rest, because Urdu script leaves most vowels out. |
| F37 | Downloads grouped by playlist | 🟡 | Downloaded songs that belong to playlists show under each playlist ("12 of 40 songs downloaded", with Play and Shuffle); the rest under Songs. |
| F38 | Recommendations learn from saved playlists | ✅ | Songs in your playlists now seed picks ("Because … is in your playlist …"). 🧪 |
| F39 | Moods as pictures | ✅ | Each mood tile has its own icon art, rings and a two-colour gradient. |
| F40 | Artists and albums on the home page | 🟡 | "Your Artists" (your most played, with pictures) and "Albums for You" (by them), looked up side by side and kept with the home picks. 🧪 ranking. Live lookup not yet seen on the phone. |
| F41 | Refresh button only on the laptop | ✅ | Phones pull down to refresh; desktop keeps the button. |
| F42 | "Harp" with an animated logo instead of "Listen Now" | ✅ | The logo writes itself in, and its strike line quivers like a plucked string with the music (still when paused or with effects off). |
| F43 | Share a song | 🟡 | Share sheet on Android, link copied on desktop (a YouTube Music link anyone can open). |
| F44 | The star button in the player was pointless | ✅ | It is now **Add to playlist**. Superlike moved to a long press on the heart (the heart turns into a sparkle). |
| F45 | A better music-reactive player background | 🟡 | **Back to the original soft waves** (you liked them better than both newer tries), now rising with the real loudness and lightly shaped by the spectrum. Before that: spectrum-driven waves (you preferred waves to the colour clouds): three layers over the blurred cover, bass in the middle, treble at the edges, cover colours. Earlier attempt: | New moving background made from the cover's own colours: soft clouds of light that drift, swell with loudness and move faster with energy; beats send a ring of light out from behind the artwork. Still when paused or with effects reduced. 🧪 renders and stops ticking when paused. Feel on the phone not confirmed (Android's loudness is still simulated until M2). |
| F46 | New songs go on top of a playlist | ✅ | Added songs appear first (a batch keeps its order). 🧪 |
| F47 | Playlists you downloaded keep downloading new songs | 🟡 | "Download All" now also turns on auto-download for that playlist (shown under its name; "Stop downloading new songs" in its menu). Songs added later download by themselves. |
| F48 | Search's top result can't be queued or added to a playlist | ✅ | It has a ⋮ menu and a long press, like every other song. |
| B24 | The reactive background crashed the phone | 🟡 | Cause: it drew full-screen "screen"-blended clouds and blurred rings 60 times a second, and on Android's graphics engine each such draw can force a full-screen copy. Now it's drawn at 54 px wide with normal blending and scaled up (same look), at 30 fps. Needs the phone to confirm. |
| B25 | Lyrics don't open at the current line | 🟡 | The list only builds lines near the top, so a far line had no position and the scroll gave up. Now it jumps near the line by estimate, then lands exactly. |
| F49 | Restoring a backup should add, not overwrite | ✅ | Restoring adds the backup to the current library, right away (no restart). 🧪 |
| B26 | Skipping left the next song paused | ✅ | The new engine needs `play: true` when opening a song. Reproduced and fixed on the laptop's engine. |
| B27 | The player is always dark, even in light mode | 🟡 | It follows the app's theme now (pale cover wash, dark text in light mode). |
| B28 | A pill bar on the player that did nothing | ✅ | Removed. |
| F33 | The same for Amazon Music | ⛔ | No public API for listeners (Amazon's web API is invite-only), and shared playlist pages are a JavaScript app with no songs in the page (checked). Workarounds: paste the list, or a CSV from a converter such as TuneMyMusic (already imports). |

**Features (48): 29 done and checked, 15 done but waiting for your confirmation, 3 in progress,
1 not possible (Amazon Music); plus parts that are not possible (Dolby Atmos, Apple Music
account access without a paid developer key).**

---

## 2. Milestone map

Each milestone has a goal, its tasks (☐ to do, ☑ done), what "done" means, and what it depends
on. They are in order; later ones can overlap once their dependencies are met.

### M1 · Rock-solid on your phone *(now)*
Goal: what exists works every day on Android and Linux, with no guessing.
- ☐ You confirm: no crash, notification controls, screen-off playback, yt-dlp streaming, the
  redesign, search history, Spotify connection, Apple Music links (B7, B11, B12, B15, B16, F26, F31, F32).
- ☑ Crash reports (Settings › About), frame-time readout (Settings › Performance).
- ☑ Fallback yt-dlp inside the APK; queue and position survive restarts.
- ☐ Fix whatever the crash report and frame times show.
- **Done when:** a week of daily use on the phone without a crash, a stall, or a missing control.

### M2 · A new audio engine with real DSP on every platform
Goal: equalizer, spatial audio, a real spectrum and gapless playback **on Android too**.

Why: Android's libmpv (from `media_kit`) was built without most audio filters. Checked in the
APK: it has `lavfi` and `equalizer`, but no `stereowiden`, `crossfeed`, `sofalizer` or `astats`.
That is why spatial audio hung the Android player, and why Android's background only imitates
the music.

- ◐ **Spike: `mpv_audio_kit` 0.6.4** (in progress). **Laptop: ✅** equalizer, stereo widen,
  crossfeed, `dynaudnorm` and even `loudnorm` all play without stalling; the live FFT spectrum works;
  **`sofalizer` is not in its FFmpeg build** (so true HRTF needs another route). Its native library
  downloads from the package's GitHub releases, checked against pinned SHA-256s. It ships
  `libmpv_audio_kit.so` (no clash with media_kit's `libmpv.so`) and declares its own Media3 service
  (inactive unless its session is turned on). **Phone:** Settings › About › Test the new audio engine.
  Original notes: (BSD-3 package, bundles LGPL libmpv with FFmpeg 8.1). Its docs list
  86 FFmpeg effects on Android (arm64, armv7, x86_64) and desktop, including an 18-band graphic
  EQ, compressor, loudness normalisation, bass/treble, **stereo width, headphone crossfeed**, raw
  lavfi filters, **gapless** playback (playlist prefetch), a **live FFT spectrum**, raw PCM and
  media-session controls. Check: whether `sofalizer` is in its FFmpeg build, that it coexists
  with our `audio_service` session (or replaces it), APK size (+7.7–8.3 MB per ABI vs about 6 MB
  today), Android 7+ (ours is 9+).
- ☑ Spike passed on the phone (all effects and the live spectrum; no `sofalizer`).
- ◐ **Replaced `media_kit` with `mpv_audio_kit`** behind `PlayerController` (the rest of the app is
  unchanged): equalizer, spatial, even volume (now true EBU R128 `loudnorm`) on every platform,
  effects change live without reopening the song, the background follows the real spectrum. 🧪 all
  40 effect combinations play without a stall on the laptop. The engine's own media service is
  removed (audio_service keeps the notification). **Needs the phone:** playback, notification,
  lock screen, headphones, screen-off.
- ☐ **Plan B** if the spike fails: build our own libmpv for Android with FFmpeg's audio filters
  enabled (zevro's script already cross-builds FFmpeg with the NDK), or use Android's
  `DynamicsProcessing` effect (API 28+: multi-band EQ, compressor, limiter; what the open-source
  Equalizer314 uses on the global session) for the equalizer only.
- **Done when:** the same equalizer and spatial settings work on Android and desktop, the reactive
  background follows a real spectrum on both, and songs change without a gap.

### M3 · Sound
Goal: control over how it sounds. *(Needs M2 for Android.)*
- ◐ Equalizer: presets (Flat, Bass, Vocal, Treble, Loudness) **in the app on desktop** (Settings ›
  Playback; each chain played in libmpv together with spatial and the loudness reader ✅). Android
  waits for M2. Then a 10-band custom EQ, then per-headphone correction (AutoEQ profiles).
- ☐ Spatial audio: Wide stage and Headphone crossfeed on Android (M2); a true **HRTF binaural** mode
  with a bundled HRTF data set (FFmpeg `sofalizer`, or Steam Audio / OpenAL Soft, both of which
  support HRTF on Android).
- ◐ **Even volume** on desktop (a slow leveller, `dynaudnorm`; FFmpeg's `loudnorm` stalls
  libmpv, checked). Android waits for M2. Crossfade and gapless: M2.
- ☑ Playback speed, 0.75–2× with the pitch kept (Now Playing and Settings); resume position for
  long tracks and episodes (10 min or more, each remembered on its own).
- ☑ Sleep timer.
- **Done when:** every sound option works the same on phone and laptop.
- ⛔ Dolby Atmos stays impossible (stereo sources, licensed decoder).

### M4 · Lyrics
Goal: lyrics as good as Apple Music's: word-by-word where they exist, line-synced otherwise.
- ☑ Line-synced lyrics from LRCLIB, tap a line to seek, cached for offline.
- ◐ A lyrics-source layer in Rust that tries several sources in order and caches the best.
  **Done:** LRCLIB synced → **NetEase** synced (keyless; strong for Indian and Asian songs) →
  LRCLIB plain → **YouTube Music's own lyrics** (LyricFind, plain, often in the original script,
  which the Hinglish switch then spells out). Checked live on a Hindi song; 🧪 on saved real
  responses. Songs remembered as "no lyrics" are looked up again once. Still to do:
  - **NetEase** (keyless): word-by-word "YRC" format, strong for Asian catalogues.
  - **LyricsPlus** (the open YouLy+ backend, `ibratabian17/lyricsplus`): word-synced lyrics
    gathered from Apple Music, Musixmatch and others.
  - **Musixmatch** word sync ("richsync"), as the SyncLyrics library does: needs a token, so last.
  - LRCLIB stays the open default.
  - **Better Lyrics** (GPL-3, like Harp; github.com/better-lyrics): its extension merges bLyrics
    (its own word-synced TTML catalogue), Unison (community lyrics, `unison.betterlyrics.org`),
    Musixmatch richsync, LRCLIB and YouTube Music's own lyrics. Its backend (Go, open source)
    gets most lyrics from **lrc.red**, a shared catalogue keyed by ISRC, and documents its API
    at docs.betterlyrics.org (with rate limits). The extension passes a Turnstile check before
    asking for lyrics, so a third-party app may not be welcome: ask the authors or use lrc.red
    and Unison directly. Its TTML handling is a good reference for our parser.
- ☐ Parsers for TTML (word timings, background vocals, duet sides) and YRC, with tests on saved
  real responses.
- ◐ **Find lyrics by name** and a **timing nudge** (±0.5 s, saved per song) are in. Still to do:
  word-by-word highlighting (Apple-style sweep), duet alignment, bigger full-screen lyrics.
- ☐ Later: translation / romanisation lines where sources provide them.
- **Done when:** popular songs show word-by-word lyrics; others fall back to lines; nothing breaks
  offline. Check each source's terms before shipping it.

### M5 · Library and discovery
- ☑ Your most played artists and their albums on Listen Now; **follow artists** (button on their
  page) and **save albums, playlists and podcasts** (Library › Artists, Albums & Podcasts);
  **New Releases** on Listen Now from the artists you follow (this year and last, newest first,
  checked live; 🧪 on a saved artist page).
- ◐ Sort: Liked and Downloaded (recent, title, artist), Library playlists (recent, name; pinned
  stay on top), remembered. Playlist covers are automatic mosaics. Still to do: filter, folders,
  custom covers.
- ☑ Backup and restore of the whole library to one file (Settings › Backup; 🧪). Restoring
  **adds** the backup to what's here (likes, playlists by name, history, follows); nothing is lost,
  and restoring twice adds nothing more.
- ☑ **Your own music files** (Library › On This Device): pick folders; tags, covers and lengths are
  read; they play offline and work with likes, playlists and the queue. 🧪 on real files. Android:
  asks for the music permission (needs the phone to confirm).
- ☑ Browse (Search tab): **Charts** for your country (from the device's region) and every
  YouTube Music **mood and genre** as a tile, each opening rows of playlists. Checked live; 🧪.
- ◐ A smarter taste model: **time decay** (an artist's score halves every 60 days) and
  **time of day** (artists you play around this hour get a nudge) are in, 🧪. Still to do:
  song-level signals, wider starter genres, recommendations from downloads when offline.
- **Done when:** the Library tab can replace another music app's library for you.

### M6 · Platform integration
- ☐ Android: Android Auto browse tree (the media service is there already), home-screen widget,
  Now Bar / lock-screen check, Wear OS controls.
- ☐ Desktop: tray icon, global media keys, a mini-player window, Discord rich presence.
- ☐ Casting (Chromecast / DLNA).
- ☐ Windows and macOS: run them for real (built, never run).

### M7 · Together and sync
- ☐ Peer-to-peer library sync between your own devices on the same network (no servers).
- ☐ Share a song or playlist as a link / card / QR code.
- ☐ Listening rooms with a shared queue.

### M8 · Release
- ☐ A real signing key for APKs; versioning and an in-app update check.
- ☐ F-Droid metadata, GitHub Releases (Actions only here), Flathub / AppImage, Windows installer,
  macOS DMG.
- ☐ Accessibility pass (screen readers, text size, contrast) and translations.
- ☐ iOS: decide how yt-dlp could run there (see open questions).

### Research notes (sources)
- mpv_audio_kit (Flutter, BSD-3, libmpv with FFmpeg audio effects on Android and desktop):
  https://pub.dev/packages/mpv_audio_kit
- Android `DynamicsProcessing` (API 28+): https://developer.android.com/reference/android/media/audiofx/DynamicsProcessing
  · Equalizer314 (GPL-3 app using it): https://f-droid.org/en/packages/com.bearinmind.equalizer314/
- Android spatial audio and head tracking (Android 13+, system-level):
  https://source.android.com/docs/core/audio/spatial
- Steam Audio (HRTF, SOFA files, Android libraries): https://valvesoftware.github.io/steam-audio/doc/capi/getting-started.html
  · OpenAL Soft fork with Android HRTF: https://github.com/6779660/OpenAL-MOB
- Better Lyrics: https://github.com/better-lyrics (extension, API, docs.betterlyrics.org, lrc.red)
- Word-synced lyrics: YouLy+ / LyricsPlus (https://github.com/ibratabian17/youlyplus),
  SyncLyrics (Musixmatch, LRCLIB, NetEase: https://synclyrics.js.org/),
  NetEase YRC format (https://docs.lyricify.app/en/lyrics/guide/),
  TTML tools (https://awesome.ecosyste.ms/projects/github.com%2Famll-dev%2Famll-ttml-tool).

---

## 3. Known bugs and weak spots in the implementation

**Reliability**
- ~~Android depends on GitHub at first launch~~ fixed: a verified yt-dlp ships inside the APK as
  a fallback; the first download also has a time limit now.
- **Only SHA-256 checks yt-dlp downloads.** The checksum file's GPG signature is not verified.
- **No database migrations.** Tables are created with `IF NOT EXISTS`; the first schema change
  will need a versioned migration step.
- **Rust calls block worker threads.** Long yt-dlp calls (2–5 s) share the bridge's thread pool
  with everything else and cannot be cancelled; heavy use can still delay taps.
- ~~The queue is not saved across restarts~~ fixed: the whole queue and the position come back.
- ~~The cover cache has no size limit~~ fixed: capped at 300 MB.
- ~~Prefetching ignores mobile data~~ fixed: off on metered networks unless allowed in Settings.
- **Undocumented YouTube Music calls.** Search filters (e.g. podcasts = `JQ`) were found by
  experiment and can change without notice; yt-dlp is a fallback only for songs.
- **Spotify link imports are capped at 100** by Spotify; the account connection (F31) is the fix.
- **Import matching makes one search per song** (four at a time); very large imports may hit
  rate limits, and ISRCs are not used for exact matching.

**Platforms**
- **Android: arm64 only, Android 9+.** x86_64 and 32-bit devices cannot stream (musl Python
  limits); the APK carries 23 MB of Python.
- **Android: no spatial audio, no real loudness signal** (libmpv filters stall there).
- **Windows and macOS: built, never run.** macOS is unsigned with the sandbox off.
- **Release APKs are signed with the debug key**: fine for testing, not for distribution.
- **Linux: libmpv comes from a locally extracted package** on this laptop; a clean install needs
  the system package (`mpv-libs`).

**Testing gaps**
- No widget tests of whole screens with a fake backend; no Android emulator test in CI.
- Nothing has been run on your phone by me; every 🟡 above depends on your report.
- Performance has never been measured on either device.

**Security and privacy**
- Spotify tokens (once wired) will sit in the app's local database unencrypted (device-only).
- The loopback proxy's credentials appear in yt-dlp's command line (visible to other processes
  of the same user).

---

## 4. Weak spots in the idea itself

- **Built on YouTube without permission.** Streaming through yt-dlp breaks YouTube's terms;
  the app cannot go on Google Play, and YouTube changes can break it at any time. Harp needs fast
  yt-dlp updates and a plan for outages.
- **"No servers" versus convenience.** Without accounts there is no cross-device sync or backup
  today; peer-to-peer sync (originally planned) and file export/import should cover it.
- **Other services guard their libraries.** Spotify allows reading only with a developer app
  (and limits those to a few users), Apple needs a paid developer token, Amazon has no public
  access. "Bring your library over" can never be one tap for every service.
- **A taste model on one device learns slowly** and only from what you play there; it scores
  artists, not songs or moods, has no time decay, and its starter genres lean Western and Indian.
- **Python on Android is heavy** (size, start-up, arm64 only). A native extractor (talking to
  YouTube's player API directly, like NewPipe) would remove it, but is a large project.
- ~~**Lyrics come from one free source** (LRCLIB), which will miss many regional songs.~~ Now three (LRCLIB, NetEase, YouTube Music).

---

## 5. Ideas worth adding (backlog)

**Listening:** ~~sleep timer~~ (done) · equalizer (desktop first) · playback speed (podcasts) · crossfade /
gapless · loudness normalisation · "radio" from any song, artist or playlist · smart downloads
(auto-download liked songs on Wi-Fi) · offline recommendations from downloads.

**Library:** sort and filter · album and artist sections in the library · follow artists and
see new releases · playlist folders and covers · local files as a source · backup and restore of
the whole library to a file · LAN peer-to-peer sync between your devices.

**Discovery:** time-of-day and mood-aware picks · "your year" stats computed on the device ·
charts by country · related-artist exploration from the artist page.

**Platforms:** Android home-screen widget · Android Auto (the media browser service exists; a
browse tree is needed) · casting (Chromecast / DLNA) · desktop tray icon, global media keys,
mini-player window · Discord rich presence (desktop) · Wear OS controls.

**Sharing and together:** share a song or playlist as a link / card / QR code · listening rooms
with a shared queue (milestone 4 below).

---

## 6. Original milestones

| Milestone | Status |
|---|---|
| 1. Player | 🔧 Search, resolve, cache, queue rules, shuffle/repeat, background playback and media controls are built (🟡 on Android). Gapless not done. |
| 2. Library | 🔧 Playlists, likes, history, downloads, lyrics, radio, file export (core) done; albums/artists in the library not done. |
| 3. Bring your library over | 🔧 Matching with a review screen; Spotify / YouTube / files done; Spotify account and Apple links in progress; Amazon not possible. |
| 4. Together (listening rooms) | ⬜ Not started. |
| 5. Release | ⬜ CI builds exist; no signed releases, store listings or installers yet. |

## Open questions

- iOS: yt-dlp needs Python; App Store rules forbid downloading code. Bundle a frozen yt-dlp, or
  resolve on a paired device?
- Whether to replace Python on Android with a native extractor.
- Whether to offer an optional Apple Music connection for people who have a developer token.
