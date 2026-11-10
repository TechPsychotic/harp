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
| 🟡 | Done in code, **not yet confirmed on a real phone / computer** |
| 🔧 | In progress (part of it is built) |
| ⬜ | Not started |

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

## 1. Tracker: reported problems and requested features

### 1a. Reported problems (23)

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
| B14 | Imports stop at 100 songs | 🔧 | YouTube / YouTube Music: ✅ fixed (paging; a 150+ song playlist read in full, live). Spotify: the **account connection** (like Exportify) reads every playlist in full; a public link alone shows the first 100. |
| B15 | The app crashed on the phone | 🟡 | Likely cause found and fixed: the media-notification icon was stripped from the APK by the resource shrinker (confirmed absent, now confirmed present). Crash reporter added (Settings › About › Last crash report). Not confirmed that it was the whole cause. |
| B16 | No Spotify-style controls in the notification | 🟡 | Same cause as B15. Not confirmed. |
| B17 | Two settings entries (Home header + tab) | ✅ | The Home gear is gone. |
| B19 | Search history saved every keystroke | ✅ | History is saved only when you press search / enter or open a result (song, album, artist, podcast), like Spotify. Typing still searches as you go, but saves nothing. |
| B20 | The mini player disappears inside a playlist (or any opened page) | 🟡 | Each tab now has its own page stack, so playlists, albums and lists open inside the tab with the mini player and tab bar still showing. Back closes the tab's pages first; tapping the open tab goes back to its start. Analyzer clean; not yet tried on the phone. |
| B21 | Doesn't pause when the output changes (earphones out, Bluetooth off) | 🟡 | Android: pauses when the output would switch to the speaker, pauses for calls and resumes after, lowers the volume while navigation speaks. Needs the phone to confirm. Desktop systems don't report this, so not on Linux yet. |
| B22 | Notification sometimes has no progress bar | 🟡 | Cause: songs from search often come without a length, and the notification only draws progress when it knows the length; it was never told the real one. Now the player's length is passed on as soon as it is known, and seeks are reported too. Needs the phone. |
| B23 | Playlists: no picture, wasteful layout, no pinning | 🟡 | Covers (first song, or a 2×2 mosaic from four songs) on Library and Home; Library lists playlists as compact rows; pin to top from the … menu or a long press (pinned first, saved in the database). 🧪 database tests. |
| B24 | Playlist pauses instead of playing the next song; shown song ≠ playing song | 🟡 | The next song is lined up only in the last 30 s; the player checks the engine plays what it shows and corrects it; a watchdog moves on if it stops at the end. Needs the phone. |
| B25 | "Sign in to confirm you're not a bot" again (with crossfade) | 🟡 | One song fetched ahead (after 20 s), one crossfade try per song, a 2-minute pause on push-back, no skipping. Needs use to confirm. |
| B26 | Library play/pause overlaps text; rough queue scrolling | 🟡 | Text now shrinks with an ellipsis; queue is one sliver list with handle drags. |
| B27 | Windows and macOS builds failing | 🟡 | SystemConfiguration framework (macOS); coroutine define (Windows). Re-run on Actions. |
| B28 | Cache / downloads sizes out of date | 🟡 | Refresh on every download change; downloads reuse cached songs. |
| B29 | Playlist's next song doesn't start; play doesn't help | 🟡 | Only on-device songs wait in the engine; 5 s start check; play reloads a stalled song. |
| B30 | Bot check still showing | 🟡 | Light YouTube client first (1 request), yt-dlp cache kept, fetches spaced out. |
| F64 | Fetch 5 songs ahead, delete after the next finishes, never touch downloads | 🟡 | Done; 🧪 tidy-up. |
| B18 | Android builds failing on GitHub | 🟡 | GitHub's release downloads returned 504 for hours. Day-to-day CI now runs on the laptop (`scripts/ci-local.sh`, ✅); GitHub Actions only for releases or by hand, with caches and retries. |

**Problems (23): 5 fixed and checked, 17 fixed but waiting for your confirmation, 1 partly fixed.**

### 1b. Features you asked for (53)

| # | Feature | Status | Notes |
|---|---|---|---|
| F1 | An implementation roadmap | ✅ | This file. |
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
| F19 | 3D audio | 🟡 | Wide stage, headphone crossfeed and Immersive on every platform. True 3D (HRTF) needs a custom engine build (M3). |
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
| F32 | The same for Apple Music | ✅ | **Public Apple Music playlist links import** (the page carries the track list); file exports (text, M3U) import too. |
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
| B29 | Moods / Smart Shuffle: "User-Agent must not contain comma" | ✅ | The new engine refuses commas in per-song headers; the User-Agent now goes through mpv's `user-agent` option, browser-only headers are dropped. Reproduced and fixed with a real stream. |
| B30 | Starting a mood after a playlist kept the playlist queued | ✅ | The queue now separates your own queued songs from the list being played; something new replaces the list. 🧪 |
| F50 | Show where the music is playing from (player, Home, Library) | 🟡 | "Playing from" on the player and Home (tap to open), highlighted row in the Library. |
| F51 | Queue gestures: swipe right play next, left remove, hold to drag | 🟡 | All three, in every part of the queue; the − button stays. |
| F52 | Audio enhancements make little difference | ✅ | Rebuilt and measured: Bass +6.3 dB on the lows (was +0.4), Wide +10.9 dB (was +6.3)… |
| F53 | Listen together with iroh (M7) | 🟡 | Built and checked over the internet; needs two of your devices to try for real. |
| F54 | The playing playlist should show Pause | ✅ | On its page (and albums, Liked, Downloaded, On This Device) and on its Library row. |
| F55 | A skip shouldn't count until a song is skipped more than 5 times in 30 days | ✅ | Done; the 30-day exclusion is gone. 🧪 |
| F56 | harp:// links open Harp | 🟡 | Android (tap or share to Harp) and Linux (single app, link handed to the open window). |
| B31 | Old playlist songs stuck in "Next in Queue" | 🟡 | Old saved queues are converted on load. 🧪 |
| B32 | Opening the player on the laptop is slow | 🟡 | The full-window blur is gone (tiny stretched cover instead). |
| B33 | A playlist stopped after its first song | ✅ | The next-song check ignored the list; fixed. |
| F57 | Playing song's menu shouldn't offer Play next / Add to queue | ✅ | Hidden for the current song. |
| F58 | Share playlists like Sync | 🟡 | Share link (iroh); friends open it and save a copy. 🧪 |
| F59 | Spotify-style playlist row: Play/Pause, shuffle + Smart Shuffle toggles, filled download icon | 🟡 | Done; Smart Shuffle mixes similar songs in while playing; Recommended section with +. |
| F60 | ✓ on songs in a playlist; see and change which playlists | ✅ | ✓ on rows, "Saved in" sheet with checkboxes. 🧪 |
| F61 | Listening stats like Sound Capsule → **Your Rhythm** | 🟡 | Week/month/year minutes, chart, top artists and songs, time of day, streak, listener kind. 🧪 |
| F62 | Year in review like Wrapped → **Harp Encore** (from the 2nd week of December) | 🟡 | Story pages from 8 Dec to 7 Jan; save the top 50 as a playlist. 🧪 |
| F63 | Updates install themselves | 🟡 | Android: download + checksum + system installer (you confirm). Linux: swap in and restart. Win/mac: release page. Needs a public repo and a published release. |

**Features: almost all done; the ones marked 🟡 are built and waiting for a check on your devices.**

---

## 2. Milestone map

Status as of 2026-11-10. ☑ done · ◐ partly done · ☐ to do. 📱 = built, waiting for a check on a phone.

### M1 · Rock-solid on phones
- ☑ Crash reports, frame-time readout, fallback yt-dlp inside the APK, queue and position survive restarts.
- ☐ A week of daily use on the phone without a crash, a stall or a missing control (needs you).

### M2 · One audio engine with real DSP everywhere ☑
- ☑ `mpv_audio_kit` on Android, Linux, Windows and macOS (checked on the phone with the engine test).
- ☑ Effects change live; the background follows the real spectrum; gapless playback.

### M3 · Sound ☑
- ☑ Equalizer presets (measured), a custom 10-band equalizer, **headphone correction** from AutoEq (~9,000
  measured models), even volume (EBU R128), spatial (Wide, Headphone crossfeed, Immersive), playback
  speed, sleep timer, resume for long tracks.
- ☑ **Crossfade** (0–12 s, a second player; gapless when off).
- ☐ **Next after v0.9.0:** true 3D (HRTF). The engine's FFmpeg has no `sofalizer`, `headphone` or `afir` (checked), so: a custom
  engine build with libmysofa, or a different HRTF library.

### M4 · Lyrics (low priority)
- ☑ Three sources (LRCLIB, NetEase, YouTube Music), Hinglish transliteration, find by name, timing nudge,
  opens at the current line.
- ☐ Word-by-word highlighting (TTML/YRC), duet layout, full-screen lyrics.

### M5 · Library and discovery ☑
- ☑ Playlists: covers (mosaic or your own picture), **folders**, pins, sorting and filters, Spotify-style
  page (shuffle / Smart Shuffle toggles, Play/Pause, Recommended), auto-download, ✓ "saved in".
- ☑ Liked, downloads (grouped by playlist), your own files, followed artists, saved albums and podcasts.
- ☑ Discovery: on-device taste (fades with time, time of day, skips count only when repeated), moods,
  charts and genres, new releases, offline picks, world genres.
- ☑ Backup and restore (restore adds), imports from Spotify, Apple Music links, YouTube, files.
- ☑ Your Rhythm (listening stats) and Harp Encore (the year in review, from 8 December).

### M6 · Platform integration
- ☑ Android: media session, notification and lock screen, edge-to-edge, predictive back, adaptive icon,
  Android Auto browsing, **home-screen widget**, harp:// links and share-to-Harp. Wear OS shows the
  controls through the media session.
- ☑ Linux: MPRIS (GNOME/KDE media controls, media keys), single app with link handling, mini player.
- ◐ Windows and macOS: built by GitHub Actions on every release; not yet tried on real machines.
- ☐ Optional: Discord status (needs a Discord application id; your call).

### M7 · Together and sync ☑
- ☑ Listen together (iroh): host and guests in step (seek + rate nudging, sync offset for Bluetooth).
- ☑ Your devices: pair once, then they **keep each other in sync** by themselves (switchable).
- ☑ Shared playlists with lasting links; friends' copies **follow your changes** (switchable).

### M8 · Release
- ☑ Signed Android releases on GitHub Releases (release key; CI refuses to publish an unsigned APK),
  Linux tarball, Windows and macOS zips, SHA-256 checksums; in-app update check (Settings › About).
- ☐ Make the repository public (README and license are ready); F-Droid metadata.
- ☐ macOS notarization and Windows code signing (need paid certificates).

### iOS: a plan without yt-dlp
Everything in Harp except the stream extractor already runs on iOS in principle: the Rust core, the
audio engine (`mpv_audio_kit` supports iOS), iroh and Flutter. The blocker is yt-dlp: it's Python,
and an App Store app can't download new code to keep it current, while YouTube changes break any
fixed copy within weeks.

1. **iOS build without streaming** (a few days): build the app, play your own files and downloads
   made on other devices (synced), and check the engine, the media session and background audio.
2. **A native extractor in Rust** (the main work): Harp already talks to YouTube Music's InnerTube API
   for search and pages. Add its `player` call, and the parts yt-dlp does for us today:
   - **Signature and "n" deciphering:** run YouTube's own player JavaScript in **JavaScriptCore** (built
     into iOS), as yt-dlp now does with a JS runtime. The logic arrives as data each time, not as new app code.
   - **PO tokens:** YouTube increasingly wants a token minted by its BotGuard script. The way that works
     on iOS today is a hidden WKWebView running the minter, as
     [SwiftyTube](https://github.com/686udjie/SwiftyTube) does. Note that
     [NewPipe Extractor 0.27.0](https://freedom.tech/posts/2026-11-07-newpipe-extractor-0-27-0/) dropped the
     iOS/Android InnerTube clients because their tokens can no longer be made, so the web-based clients
     with a minted token are the route ([yt-dlp's PO token notes](https://deepwiki.com/yt-dlp/yt-dlp/3.4.1-potoken-authentication-system)).
   - The same extractor can then serve Android and desktop too, as a fallback when yt-dlp breaks.
3. **Alternatives, if 2 stalls:** embed Python (BeeWare's Python-Apple-support) with a bundled yt-dlp,
   updated with app updates (fine for sideloading, fragile on the App Store); or let people choose a
   Piped/Invidious instance (other people's servers: opt-in only).
4. **Distribution:** the App Store is unlikely to accept a YouTube player; TestFlight for testing, then
   AltStore / EU alternative marketplaces, and sideloading.

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
- Every 🟡 above still needs confirming on a real device.
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
- **"No servers" versus convenience.** Sync between your devices and shared playlists are peer to
  peer: they work while Harp is open on both ends (paired devices catch up the next time both are on).
- **Other services guard their libraries.** Spotify needs your own developer app id for full reading;
  other services import from public links or exported files.
- **A taste model on one device learns from what you play there** (synced devices share it); it
  now fades with time, knows the time of day and only counts repeated skips.
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
| 3. Bring your library over | ✅ Matching with a review screen; Spotify (account or link), Apple Music links, YouTube, files. |
| 4. Together (listening rooms) | ⬜ Not started. |
| 5. Release | ⬜ CI builds exist; no signed releases, store listings or installers yet. |

## Open questions

- iOS: yt-dlp needs Python; App Store rules forbid downloading code. Bundle a frozen yt-dlp, or
  resolve on a paired device?
- Whether to replace Python on Android with a native extractor.
- Whether to offer an optional Apple Music connection for people who have a developer token.
