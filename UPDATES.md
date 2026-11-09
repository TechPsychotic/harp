# Harp: updates

A running log of what changed, newest first. Each entry says what was added or
fixed, what has been **tested** (and how), and what is **not verified**. The
bottom of this file lists what is still open.

Legend: ✅ checked here · 🧪 automated test · 📱 needs a real phone · 🖥️ needs your laptop

---

## 2026-11-09 (night, research): milestone map; a hanging test fixed

- **Milestone map** in `ROADMAP.md` (M1–M8): reliability on the phone; a new audio engine with real
  DSP on every platform; sound (equalizer, spatial, loudness, gapless, speed); lyrics (word-by-word
  from several sources); library and discovery; platform integration; together and sync; release.
- **Why spatial audio hung on Android, found:** the Android libmpv in the APK has no
  `stereowiden`, `crossfeed`, `sofalizer` or `astats` filters (only `lavfi` and `equalizer`), so
  asking for them stalls playback. The plan (M2) is `mpv_audio_kit`, whose libmpv carries FFmpeg's
  audio effects on Android, with a custom libmpv or Android's `DynamicsProcessing` as plan B.
- Equalizer presets (Bass, Vocal, Treble, Loudness) verified in libmpv on desktop, together with
  spatial effects and the loudness reader.
- **Fixed: a test that could hang forever.** The Spotify sign-in test raced three fake browsers; if
  the right one was served first, the other two waited forever. It is now deterministic (passes
  5/5). This is what made earlier "slow" test runs time out, not compile time as I first thought.

## 2026-11-09 (night, last): a fallback yt-dlp inside the APK

- **The APK now carries yt-dlp 2026.10.26** (verified against yt-dlp's published SHA-256). The
  phone still downloads and verifies the current yt-dlp on first start; the bundled one is used
  only if that download fails and there is no earlier copy (as during GitHub's 504 outage),
  so streaming works from the first launch. Later updates happen as before.
- The yt-dlp download now has a time limit (15 s to connect, 2 min in all), so an outage can no
  longer hang the first start.
- Local CI checks the fallback is inside the APK.

## 2026-11-09 (night, later): the queue survives restarts, sleep timer, gestures

**Added**
- **The whole queue comes back after a restart** (current song, what is next, autoplay, repeat),
  and the song resumes from where it stopped when you press play. 🧪 (save/load round trip)
- **Sleep timer** on Now Playing: 15 / 30 / 45 / 60 minutes or the end of this song.
- **Gestures on Now Playing**: swipe down to close; swipe the artwork left / right for the next /
  previous song.
- **No fetching ahead on mobile data** unless Settings › Play cache › *Fetch ahead on mobile data*
  is on (Android checks whether the network is metered).
- **The cover-art cache is capped** at 300 MB (oldest covers go first).
- **Auto-update**: the Linux app is rebuilt and reinstalled after each change
  (`scripts/ci-local.sh --no-apk --install`). A git hook that does this on every commit is in
  `scripts/hooks/post-commit`; turning it on is up to you (`git config core.hooksPath scripts/hooks`).

## 2026-11-09 (evening): CI moves to the laptop

- **`scripts/ci-local.sh`** runs Harp's CI here: Rust tests, Flutter analysis and tests, then the
  Linux app and the APK one after the other through the memory-guarded builder, then checks the
  APK (notification icon, bundled Python / QuickJS / Rust core, media service). `--install`
  installs the Linux build and copies the APK to `~/Downloads/Harp/`.
- **GitHub Actions now runs only for release tags (`v*`) or when started by hand**, so ordinary
  pushes use no Actions minutes.
- The guarded builder (`scripts/safe-build-apk.sh`) also builds the Linux app now
  (`scripts/safe-build-apk.sh 30 linux`).

## 2026-11-09 (latest): redesign, search the way music apps do it, Spotify and Apple Music import

**Search**
- **History is saved only when you search on purpose**: pressing search / enter, or opening a
  result (song, album, artist, podcast). Typing still shows results as you go, but nothing is
  saved for every keystroke.
- **"All" is the default**, ranked by YouTube Music's own relevance: a top result, then songs,
  artists, albums and podcasts. Videos, playlists and profiles are left out. 🧪 on real responses.
- Filters are **All, Songs, Albums, Artists, Podcasts** (Playlists removed).

**Redesign** (to feel like a music app)
- The app takes its colour from the playing song's artwork.
- Plain list rows instead of boxed cards; big titles; section headers with "See All".
- A floating mini player above a tab bar (Listen Now, Search, Library, Settings); a sidebar on
  wide windows.
- Now Playing: the artwork's colour and a blurred copy of it as the backdrop, big artwork that
  shrinks a little when paused, bare controls, a remaining-time counter, big bold lyrics in
  place of the artwork, the queue with shuffle and repeat in its own sheet.
- Album, artist, podcast and playlist pages: centred artwork (round for artists, a 2×2 mosaic for
  your playlists), Play and Shuffle buttons, numbered tracks on albums, more shelves for artists.
- Library is a list (Liked Songs, Downloaded, Recently Played, Bring Your Music Over) with your
  playlists as a grid.
- The animated background now runs only behind Now Playing (less work for the GPU elsewhere).
- 🧪 16 Flutter tests pass (including the artwork-colour picker). 🖥️📱 not yet seen on your
  devices.

**Bringing music over**
- **Spotify account, like Exportify**, is in the app: Library › Bring Your Music Over › From your
  Spotify account (or Settings › Spotify). Every playlist and Liked Songs, no 100-song cap.
  📱🖥️ not yet tried with a real Spotify account.
- **Export** any playlist or Liked Songs as CSV (Exportify columns), M3U or JSON.
- **Apple Music links** now import (public playlists and albums). 🧪 on a saved copy of a real
  Apple Music page.
- Amazon Music: still not possible (no API, no songs in its shared pages); paste the list.

## 2026-11-09 (late night): Exportify-style tools (core), roadmap rewrite

**Added (in the Rust core, tested; not yet on any screen)**
- **Spotify account connection, like Exportify**: sign in with your own Spotify developer
  Client ID (PKCE, no secret, nothing of ours in between); reads every playlist you have and
  your Liked Songs with **no 100-song cap**. 🧪 against a fake Spotify: 250 songs read across
  pages, a rate limit waited out, both old and new API shapes, sign-in callback (right state,
  wrong state, cancel, time-out), token refresh, and the PKCE example from RFC 7636.
- **Export any playlist** as Exportify-style CSV, M3U or Spotify-export JSON; Harp can read all
  three back (round-trip tested). File names are made safe.

**Checked**
- **Apple Music**: public playlist pages *do* carry the track list (50 songs in structured
  data, more in page data) with no key, so link import is possible. I had said otherwise
  earlier; that was wrong. Reading your Apple Music *account* needs a paid Apple developer
  token, so that part is not possible without one.
- **Amazon Music**: shared playlist pages are a JavaScript app with no songs in the page, and
  there is no public API for listeners. Not possible beyond pasting or a converter's CSV.

**Changed**
- **`ROADMAP.md` rewritten** as the tracker of everything you reported (18 problems) and asked
  for (33 features), with status, plus known bugs, weak spots in the idea, and a backlog.

## 2026-11-09 (night): the crash, and why there were no notification controls

**Found and fixed**
- **The media notification icon was missing from the APK.** Android's resource shrinker
  removed `ic_stat_harp` because the media service looks it up by name at run time, so
  the shrinker thought it was unused. Without a valid icon Android cannot build the
  media notification: no lock-screen / notification controls, and the media service can
  take the app down when playback starts. This is my best explanation for both the crash
  and the missing controls on the build you have. ✅ confirmed the icon is absent from
  that APK (`aapt2`). 📱 not yet confirmed that this was the whole crash.
  Fix: a keep rule (`res/raw/keep.xml`), and CI now **fails the build** if the icon is
  missing from the APK.
- **Crash reporting inside the app.** Any crash is now written to a file before the app
  closes, and Android's own record of why it last exited is read back. Settings › About ›
  *Last crash report* shows it with Copy and Clear. Nothing is sent anywhere. If it
  crashes again, that screen gives the actual error. 📱
- **A fixed APK, built on the laptop.** GitHub's release downloads kept returning 504 and
  failed the online Android build five times, so the APK was built locally with
  `scripts/safe-build-apk.sh`: it waits for ~6 GB of free RAM, runs capped at 6 GB in its own
  memory group (so a runaway build is stopped instead of freezing the laptop), and
  freezes itself if the machine starts thrashing. Result: built in ~6 min, lowest free
  memory 5.6 GB, no freeze. ✅ checked in the APK with `aapt2`: the notification icon is
  present (it was absent before), the media service, media-button receiver, the music-app
  category, predictive back and the crash reporter are in the manifest, and Python and
  QuickJS are bundled. 📱 not yet run on a phone.
- **What probably froze the laptop.** Another heavy build (warptron's Gradle) was running at
  the same time (load 9, CPU package 97 °C, 3.5 GB held by one Java process). Several
  builds at once is the likeliest trigger. The kernel is still 6.17.1.
- **"Two settings".** The gear on the Home header was left over from the first layout.
  It is gone; Settings is only the tab. (The build on your phone predates that removal.)

## 2026-11-09 (later): artists, long playlists, speed, consistency

**Added**
- **Artists are searchable** (Search › Artists) and have a page: top songs (the full
  song list, not just five), albums & singles, playlists, similar artists.
  Tapping the artist name in Now Playing opens it. ✅ live against YouTube Music, 🧪
- **Playlists are read in full.** YouTube Music hands out 100 songs per page; the
  rest are now followed page by page (up to 5000). This is why imports and
  playlist pages stopped at 100. ✅ live (read a 150+ song playlist), 🧪
- **Cover art loads without blocking playback.** Covers are fetched in Dart now; before,
  a screen full of covers occupied the same worker threads playback and search use,
  which made taps feel late. 📱 (please report if it feels better)
- **Performance overlay** (Settings › Performance) shows frame times on screen so we can
  see what is slow instead of guessing.
- **One design system**: shared buttons, dialogs, menus, snackbars, switches, app bars
  and one big page title style on every main screen; one empty-state look; Library
  tabs use the same chips as Search. 🧪 (visual check still needed)
- **Import speed**: songs are matched 4 at a time instead of one by one.
- **YouTube / YouTube Music playlist links** now import through YouTube Music (full
  length, proper artists) and fall back to yt-dlp (up to 5000 songs).
- **`UPDATES.md`** (this file) is kept up to date with every change.
- **CI**: GitHub's release downloads (the media player's Android libraries, QuickJS, the
  Gradle distribution) returned 504s for a stretch and failed several Android builds. The
  code was fine. The workflow now caches those downloads after the first success and
  retries the build for ~9 minutes.
- Tests: 91 Rust and 13 Flutter tests pass, including new ones for playlist paging,
  the artist page, and cover-cache file names matching between Dart and Rust.

**Changed**
- The default background is now the lighter "Reduced" (still colors, no blur, no
  constant animation). "Full" (blur and live motion) is one tap away in Settings.

**Known limit (not a bug I can fix)**
- **Spotify links give at most 100 songs.** Spotify's public page itself stops at 100
  (checked on two 150+ song playlists). The app now says so and points to Spotify's
  data export or an Exportify CSV, which have no limit.

---

## 2026-11-09: reliability, background playback, lyrics, search

**Fixed**
- **yt-dlp "502 Bad Gateway / Tunnel connection failed" on Android.** My loopback proxy
  tried only the first address a site resolved to, often an IPv6 address a phone can't
  reach. It now tries every address, IPv4 first. Network errors also retry twice. 🧪
- **Songs starting out of order / late.** Loads are numbered and stale ones are dropped;
  rapid skips collapse into one jump; a stray end-of-file from the old track can't skip
  the new one. 🧪 (logic), 📱 (feel)
- **Changing a setting stalled playback.** Changing audio filters mid-song made libmpv
  rebuild its audio pipeline and stall. The song now reopens at the same position and
  resumes by itself. Spatial audio and the loudness reader are off on Android, where
  they stalled the player; the background uses a beat-style pulse there. 📱🖥️
- **Playback stopping with the screen off.** Added a foreground media service, a wake
  lock and a Wi-Fi lock on Android. 📱
- **Flaky test** (text file busy) fixed by retrying the spawn. 🧪

**Added**
- **Media session everywhere**: Android notification, lock screen and headset buttons;
  Linux MPRIS (GNOME/KDE media controls, media keys); Windows system media overlay.
  Listed as a music app on Android (APP_MUSIC). 📱🖥️
- **Play cache + prefetch**: the next songs (and the current one) are fetched ahead into
  a size-limited cache so skips are instant and playback survives a weak connection.
  Size is set in Settings (default 512 MB). 🧪
- **Lyrics** in Now Playing: synced, tap-to-seek, from LRCLIB (no key); cached on the
  device. ✅ live, 🧪
- **Search history** (recent searches, remove, clear). Searches now feed the
  recommendations ("Because you searched for…"). 🧪
- **Search Songs / Albums / Playlists / Podcasts**, and open an album, playlist or
  podcast to play, shuffle or save it as a playlist. ✅ live, 🧪
- **Long-press menu** on every song (play next, queue, playlist, like, download).
- **Android polish**: edge-to-edge, predictive back, adaptive icon (with themed
  layer), notification icon. 📱
- Downloaded the Android APK to `~/Downloads/Harp/`.

---

## 2026-11-08: features, performance, logo

**Added**
- **Settings** (a main tab): theme (system/dark/light), streaming and download quality,
  offline mode, background effects, music-reactive background, cache sizes.
- **Offline**: download songs; offline mode plays and searches only downloads. 🧪
- **Playlists**: create, rename, reorder, delete; add from any song. 🧪
- **Import playlists**: Spotify (public link, data export JSON, Exportify CSV), Apple
  Music (exported text), M3U, YouTube / YouTube Music links, pasted "Artist - Title"
  lists, with a match-review screen for weak matches. 🧪
- **Home**: Recommended for you, Change the mood, Similar to what you played, Recently
  played, playlists, downloads, liked songs; saved so it opens instantly.
- **Music-reactive background**: the real loudness from mpv's `astats`. ✅ (verified
  against libmpv with a pulsing test tone). Falls back to a beat-style pulse.
- **Spatial audio (virtual 3D)**: Wide stage, Headphone crossfeed, Virtual 3D. ✅ filter
  chains verified in libmpv. This is **not Dolby Atmos**, which is not possible here.
- **Queue**: tap a song to jump to it. 🧪
- **Logo**: a small cursive "h" with a strike through its stem, white on a red gradient;
  icons for Android, macOS, Windows and Linux.
- **Linux**: proper music-player menu entry (categories, audio types, icons, window class).
- **CI** (GitHub Actions): tests on every push; Linux and Android builds; Windows and
  macOS on release tags or by hand. Heavy builds run there, not on the laptop.

**Changed / fixed**
- Lag: the player no longer rebuilds the whole app on every playback tick; blur only on
  a few big surfaces; the background repaints ~20 fps and only while playing; the
  recommender gathers candidates once per batch instead of per song; an adaptive
  governor lowers effects on slow machines.
- Corners are less rounded; the navigation and mini player are one dock at the bottom.
- Laptop crashes while building: found to be a kernel fault, not memory (see notes).
  Local builds were capped (Gradle 3 GB, cargo 3 jobs).

---

## 2026-11-07: the start

- Read the roadmap; wrote `IMPLEMENTATION_PLAN.md`. Name decided: **Harp**. Private GitHub
  repo created (`TechPsychotic/harp`).
- **Rust core** (queue, on-device taste model and recommender, yt-dlp runner and
  installer with SHA-256 check, Android loopback proxy and CA bundle). 🧪
- **Android yt-dlp** done the way `../zevro` does it: a static Python and QuickJS shipped
  as native libraries (`scripts/android-tools.sh`), the verified yt-dlp zipapp, a
  loopback proxy for DNS and a CA bundle. Not Chaquopy.
- **Recommender** (no sign-in, no API keys): graded skip/finish scoring, 4× superlike,
  search as a signal, skip reasons, genre injection, a reason line for every pick.
- **Search via YouTube Music's public web API** (the calls `ytmusicapi` makes), because
  yt-dlp's flat results had no artists. yt-dlp is the fallback. ✅ live
- **Flutter app** with the glassmorphic look; Linux release installed on the laptop.

---

## Still open

**Asked for, not done yet**
- A full **Apple-Music-style redesign** (and Spotify-style features) beyond the
  consistency pass above.
- **BetterLyrics-style word-by-word lyrics** (only LRCLIB line-synced lyrics now).
- Android **Live Updates / Now Bar**: the normal media notification is what Samsung's Now
  Bar uses; nothing extra built or confirmed.
- **Spatial audio on Android** (needs a libmpv build whose filters don't stall, or
  Android's own spatializer).
- A proper **HRTF binaural** mode (needs a bundled SOFA data file).
- **Podcast** extras (episode progress, subscriptions).

**Not verified on a real device**
- Everything Android-specific: the media service, screen-off playback, predictive back,
  the adaptive icon, import, downloads. 📱
- The new Linux build's media controls and the effects settings. 🖥️

**Notes**
- Dolby Atmos is not possible: it needs a licensed decoder and Atmos-encoded content;
  YouTube's music streams are stereo.
- Laptop crash cause: the kernel logged `BUG: unable to handle page fault` on 6.17.1
  (a kernel-memory fault, not an out-of-memory kill). Update the kernel and run
  memtest86+ if it continues.

---

*Maintenance rule: every change to Harp gets an entry here, in the same commit.*
