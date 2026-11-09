# Harp: updates

A running log of what changed, newest first. Each entry says what was added or
fixed, what has been **tested** (and how), and what is **not verified**. The
bottom of this file lists what is still open.

Legend: ✅ checked here · 🧪 automated test · 📱 needs a real phone · 🖥️ needs your laptop

---

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
- **CI**: the Android build now retries, and the Python/QuickJS downloads are cached and
  retried, because GitHub's own downloads (the media player's Android libraries, then
  QuickJS) returned temporary 504s and failed two runs. The code was fine.
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
