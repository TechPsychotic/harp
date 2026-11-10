# Harp: updates

A running log of what changed, newest first. Each entry says what was added or
fixed, what has been **tested** (and how), and what is **not verified**. The
bottom of this file lists what is still open.

Legend: ✅ checked here · 🧪 automated test · 📱 needs a real phone · 🖥️ needs your laptop

---

## 2026-11-10 (night, latest): roadmap kept off GitHub

- `ROADMAP.md` is no longer in the repository; it stays on the maintainer's machine (and is ignored by
  git). README updated.

---

## 2026-11-10 (night, later): ready for a public repository

- Checked every tracked file and the whole git history for secrets: no keys, passwords or tokens; the
  release signing key stays outside the repository. `.gitignore` now also refuses keystores,
  `key.properties` and `.env` files.
- GitHub Actions only runs on version tags or by hand, so pull requests from forks can't reach the
  signing secrets.
- README links the website and the latest release; new `CONTRIBUTING.md` and `SECURITY.md`;
  wording made fit for any reader.

---

## 2026-11-10 (evening, later): website; HRTF plan

- **Website** in `docs/` (live at techpsychotic.github.io/harp): dark theme whose accent changes with each
  section as you scroll (headline words, labels, icons, buttons, the sky's glow and the spheres all
  follow it); custom line icons on accent-tinted tiles; glossy lit spheres with bloom, a coloured rim,
  light from below and depth of field, shaped per section (cluster, pile, equalizer, lyric wave, two
  groups in step, orbit, edges, flying away); the nav underlines the section you're in. Fixed: the live
  site showed the new page with the old cached stylesheet (unstyled); the CSS and JS links now carry a
  version so updates always load together. ✅ checked in Chrome on desktop and narrow widths.
- **Website, smoother:** the spheres follow the scroll exactly: each shape holds while its section is
  in view and turns into the next over the stretch between sections, with time-based easing. Scrolling
  is the browser's own again (a smooth-scroll library fought the page's settings and made scrolling
  jumpy; removed). The shapes are now real 3D arrangements seen in perspective, turning slowly and
  leaning toward your pointer: a ball (hero), a double helix (taste), a dome on the floor with a soft
  shadow (offline), equalizer bars, a lyric wave, two balls that swell on the same beat with music
  travelling between them (together), a planet with a ring passing behind it (rhythm), and a small ball
  beside "Everything else". Each is measured and centred in its area. ✅ checked in Chrome, desktop and
  narrow, with real wheel scrolling.
- **Website, headings over the spheres:** the Discover, Offline and Rhythm headings run larger and
  wider so they cross their shapes (like the hero's), with the shapes pulled in beneath them; a soft
  dark halo behind all headings keeps every letter readable over bright spheres. On tablets and phones
  the headings wrap normally. ✅ checked in Chrome.
- **Website, one section at a time:** with a mouse wheel, trackpad or the keyboard (arrows, Page Up/Down,
  space), each gesture glides to the next or previous section, centred; a trackpad's glide counts as
  one gesture. The download area at the end scrolls freely. Touch screens keep their own scrolling and
  settle gently on sections. (Plain CSS snapping was tried first: small wheel turns snapped straight
  back.) ✅ checked in Chrome with real wheel turns, down, up and through the download area.
- **Website, fixes:** the Lyrics section now sings by itself while it's on screen (each line fills in
  from left to right, then the next) instead of following the scroll, which snapping had made
  invisible; sections taller than the screen (08) line up by their top, so the heading isn't under the
  menu; from Privacy (09) on, scrolling is free. ✅ checked in Chrome.
- **HRTF (true 3D):** checked the engine's FFmpeg: it has no `sofalizer`, `headphone` or `afir`, so
  it needs our own engine build with libmysofa. Planned as the milestone after v0.9.0.

---

## 2026-11-10 (evening): next song stalls, bot check, fetching five songs ahead

**Fixes**
- **A playlist's next song didn't start, and play didn't help** until you went back a song and skipped
  again. Cause: near the end of a song Harp lined up the next one *in the engine* as a streaming link;
  when that link failed, the engine stopped on it while the screen had already moved on, and play only
  resumed the stopped engine. Now only songs **on the device** (downloaded or fetched ahead) are lined
  up in the engine; others load the ordinary way when the song ends. If a song hasn't really started
  5 s after the engine moved on, it is loaded again, and play on a stalled song loads it properly. 📱
- **"Confirm you're not a bot" again.** Each song cost 4–5 requests to YouTube (its web page, its
  player code, three player clients), with yt-dlp's cache switched off so the player code was fetched
  and solved again every time (slow with QuickJS on the phone). Now Harp asks with YouTube's lightest
  client first: one request, plain audio links, no player code. If that has nothing, yt-dlp's usual
  way runs, now with its cache kept between songs. 🧪 the fallback order; ✅ live on the laptop: the
  stream resolves and a song downloads (3.4 MB) through the light client.

**Fetching ahead** (Settings › Play cache)
- The **next 5 songs** (fewer if the cache limit is small) are fetched onto the device once a song has
  played 10 s, one at a time a few seconds apart, so they start instantly and play through a weak
  connection. A song's cached file is **deleted once the song after it has finished** (so going back one
  song is still instant). **Downloaded songs are never fetched or deleted.** 🧪 the cache tidy-up.

---

## 2026-11-10 (late): playback fixes, Your Rhythm, Harp Encore

**Fixes**
- **Wrong song / sudden pauses at the end of a song.** The next song used to be lined up in the engine
  long before the end; if the queue changed meanwhile (or that early link expired) the engine could play
  the song *after* the one shown, or stop. Now the next song is lined up only in the last 30 s (never in
  the last 3 s), the player checks every couple of seconds that what the engine plays is what it shows
  (and corrects it), and a watchdog moves on if playback stops at the end without you pausing. 📱
- **"Sign in to confirm you're not a bot".** Harp was asking YouTube for too much at once (three songs
  ahead, plus crossfade retrying every tick). Now: only the next song is fetched, after 20 s of play;
  crossfade tries once per song; and when YouTube pushes back, background fetching pauses for 2 minutes,
  with a clear message and no skipping through the queue. 📱
- **Library play/pause** on a playlist row no longer overlaps the text.
- **Queue scrolling** is smooth: one scrolling list, and dragging to reorder starts from the handle (or a
  long press), so a scroll no longer turns into a drag.
- **Storage sizes** (cache, downloads) refresh as soon as something is downloaded or removed; a download
  of a song already in the cache reuses it instead of fetching again.
- **Moods** now lean on artists you like half the time ("because you like …"), so they fit your taste
  better. 🧪
- **Windows and macOS builds:** fixed the two build errors (a missing macOS system framework for the
  device-sync networking, and a Windows compiler setting for the media-controls plugin).

**New: updates install themselves** (Settings › About)
- Harp still looks for a new release once a day. On **Android** a tap on "Update to …" downloads it,
  checks it against the release's SHA256SUMS, and hands it to Android's installer: you confirm once (the
  first time Android also asks you to allow Harp to install updates). On the **Linux** install
  (`~/.local/opt/harp`) it downloads, checks, unpacks next to the install, swaps it in and restarts.
  Windows and macOS still open the release page. A bad checksum or a failed unpack leaves your Harp as
  it was. 🧪 checksum reading; 📱🖥️ the install itself needs the first published release, and the repo to
  be public (GitHub hides releases of private repos from the app).

**New: Your Rhythm** (Home › Your Rhythm)
- This week / month / year: minutes listened (with the change from the last one), a chart by day or
  month, songs, artists, longest day streak, time of day, your top artists and songs, and what kind of
  listener you are (Night Owl, Early Riser… Explorer). All worked out on the device. 🧪

**New: Harp Encore** (your year in music)
- From the second week of December (the 8th) to 7 January, a banner on Home opens a story: minutes, the
  first song of your year, artist and song of the year, top fives, your listener kind, your hour and
  streak, and at the end **save your top 50 as a playlist** or play them. 🧪 dates and numbers.

---

## 2026-11-10 (night): M3, M5, M6, M7, M8: crossfade, headphone correction, folders, widget, device sync, releases

**Fixes first**
- **The playlist ✓ is on the player** (like Spotify's: + when the song isn't in a playlist, a filled ✓
  when it is; tap to see which), not on song rows.
- **Download icons:** downloaded songs show a filled badge; download buttons are an outline when not
  downloaded, filled when they are, and the "downloading" icon while it works.
- **The player's blurred background is back** (the stand-in spoiled the look). What made opening the
  player stutter on the laptop was the blur being recalculated on every frame of the slide-in; it's now
  worked out once per song into a picture. 🧪 the result matches the full blur (average difference
  under 2/255).
- **Listen together stays in step.** Guests used to correct only when 1.5 s off, by jumping. Now a gap
  over 0.4 s is closed with one exact seek, and smaller drift by playing up to 8 % faster or slower
  (pitch kept) until back on the host's beat; the host sends its real position every 2 s. ✅ on the
  laptop's engine: 800 ms apart → one seek → 16 ms apart after 12 s. A **sync offset** (Listen
  Together, guests) makes up for Bluetooth delay, which no app can measure.

**M3 · Sound**
- **Crossfade** (Settings › Playback: off/3/6/9/12 s): the next song starts on a second player and the
  volumes cross over; off means gapless. ✅ two players overlapping and handing over, on the engine.
- **Headphone correction:** pick your headphones or earbuds from ~9,000 measured models (AutoEq); their
  correction applies before your equalizer. ✅ live (Sony WH-1000XM4: 10 filters) and played through the
  engine with every other effect on.

**M5 · Library**
- **Playlist folders** (… › Move to folder; folders sit at the top of the Library, renameable) and
  **custom covers** (… › Change cover). Folders carry over in backups and device sync. 🧪

**M6 · Platform**
- **Android home-screen widget:** cover, title, artist, previous / play-pause / next; tap to open. 📱
- Windows and macOS builds run on GitHub Actions with each release.

**M7 · Together and sync**
- **Pair your devices once; they keep in sync by themselves** (when Harp opens and every 10 minutes
  while both are on). Only paired devices are let in. 🧪 pairing, automatic sync, a stranger refused.
- **Shared playlists keep their link and stay up to date:** friends who saved one get your changes. 🧪
- **Switches** (Settings › Backup, and the Sync page): "Keep my devices in sync" and "Keep saved
  playlists up to date". Off means only when you ask (a device with it off also refuses automatic syncs).

**M8 · Releases**
- Android releases are **signed with Harp's release key**; the release workflow refuses to publish an
  APK signed any other way, adds SHA-256 checksums and publishes (not a draft). Version **0.9.0**.
- **Update check** in Settings › About (looks at the latest GitHub release once a day).
- **One-time step for you:** run `scripts/release-secrets.sh` to put the signing key into the repo's
  GitHub secrets (I wasn't allowed to write secrets myself). And keep a copy of
  `~/.local/share/harp-signing/`: without it, future updates can't install over this version.
- **Your phone:** this build is signed with the new key, so it can't update the old one in place:
  back up (Settings › Backup), uninstall, install, restore (restore adds, so nothing is lost). Once.

**Docs:** README and the GPL-3.0 LICENSE ready for a public repository; the roadmap shows today's
status without the impossible items, plus an **iOS plan** (a Rust extractor running YouTube's player
code in JavaScriptCore with tokens from a hidden web view, since yt-dlp can't stay current there).

## 2026-11-10 (evening): playlists like Spotify, sharing, the ✓, smarter menus

**Playlists**
- **A new action row** (same look as before otherwise): on the left, **download** (a filled
  circle-arrow; accent when everything is downloaded; the "downloading" icon while it works) and
  **share**; on the right, **shuffle** and **Smart Shuffle** toggles (accent with a dot under them
  when on, remembered per playlist), and a big round **Play / Pause**. No more "Resume": it's Play
  or Pause, like Spotify.
- **Smart Shuffle** mixes songs like the playlist's into it as it plays (one after every three of its
  songs), picked from a few of its songs. Fetched after the music starts, so nothing waits. 🧪 mixing.
- **Recommended** at the bottom of every playlist: songs like its own, each with a + to add it.
- **Share a playlist**, the same way as Sync: Share shows a link; friends open it in Harp, see the
  name and song count, and save a copy. Works while sharing is on and Harp is open on your device.
  Links open Harp (Android: tap, or share the message to Harp; laptop: click), or paste them in
  Library › 🔗. 🧪 several friends fetch the same shared playlist.

**Songs**
- **A ✓ on songs that are in one of your playlists.** Tap or hold it to see which playlists, tick
  others to add it there, untick to take it out, or make a new playlist with it. "Add to playlist"
  everywhere opens the same sheet. 🧪
- **The song that's playing no longer offers "Play next" or "Add to queue"** in its menu: it can't
  come after itself.

## 2026-11-10 (later): gapless, custom equalizer, device sync, offline picks, links that open Harp

**Fixed**
- **A playlist could stop after its first song** (my bug, from the queue rework). When deciding
  whether anything came next, Harp looked at your own queue and autoplay but not the new "Next from"
  list, so it fetched a Smart Shuffle pick instead, which also cleared the playlist. It now plays
  the list.
- **Old queued songs that never went away.** A queue saved by the previous version kept a playlist's
  songs in "Next in Queue" (where your own songs live), so they survived everything you started
  afterwards. Old saved queues are now converted on load: those songs become the list part, which
  the next thing you play replaces. 🧪
- **Opening the player on the laptop was slow.** The background blurred the full-size cover across
  the whole window, recalculated on every frame of the slide-in, which is heavy on a big desktop
  window. It's now a tiny copy of the cover stretched smooth: the same look for almost nothing.
- **The playlist that's playing shows Pause** (Resume when paused) instead of Play, on its page and
  as a button on its Library row. Same for albums, Liked Songs, Downloaded and On This Device.

**Skips (your rule)**
- The 30-day "don't pick a song you skipped" rule is gone. **A skip now counts for nothing**, neither
  against the song nor its artist, **until you've skipped that same song more than 5 times in 30
  days**. Only then is it set aside (and finishing it once takes that back). The "Why did you skip?"
  buttons still count, since you choose those. 🧪

**Gapless playback (M2)**
- The next song is lined up in the engine while the current one plays, so songs follow each other
  with no gap. Any change to the queue lines up the right one again. ✅ checked on the laptop's
  engine: the next track starts the moment the first ends, without stopping. (A first try that
  resolved the next song at the last moment didn't work: the engine stopped; it's resolved ahead now.)
- **Crossfade is not done:** overlapping two songs needs a second player running beside the first;
  that's still to come.

**Custom equalizer (M3)**
- Settings › Playback › Equalizer › **Custom…**: ten sliders (31 Hz to 16 kHz, ±12 dB), applied live
  as you drag; the volume makes room for your biggest boost so nothing distorts.

**Sync your devices (M7)**
- Settings › Backup › **Sync with another device**. One device shows a link, the other pastes it, and
  both end up with each other's likes, playlists, history and follows. Nothing is replaced or lost.
  Peer to peer over iroh. 🧪 two devices swap libraries (a 300 KB one arrives whole); waiting can be
  cancelled; a party link pasted by mistake gets a clear message.

**Offline and starter picks (M5)**
- Smart Shuffle (and autoplay) **offline** now rank your downloads and your own files by your
  taste, instead of shuffling them at random. 🧪
- More of the world's music to start from and to ask for: Bollywood, Punjabi, South Indian, Sufi &
  Ghazal, K-Pop, J-Pop & Anime, Afrobeats, Arabic, Metal, Classical, Gospel.

**harp:// links open Harp**
- **Android:** tapping a party or sync link opens Harp on Listen Together (asking "Join the party?")
  or Sync with the link filled in. Many chat apps don't make harp:// tappable, so Harp also appears
  in **Share**: share the message to Harp and it finds the link. 📱
- **Laptop:** the link opens Harp; if Harp is already open, the open window takes it and comes forward
  (Harp is now one app, not a new window each time).

## 2026-11-10: queue that makes sense, "playing from", stronger sound, listening together, Android Auto

A big batch, built once at the end as you asked.

**The queue**
- **Starting something new replaces what was playing.** The queue now has three parts, like
  Spotify: **Next in Queue** (songs you added yourself), **Next from: <playlist / album / liked
  songs…>**, and **Autoplay**. Playing a mood, Smart Shuffle, a song or another playlist replaces
  the "Next from" part; your own queued songs stay. (Before, a playlist's songs sat in the same list
  as your own and never went away.) 🧪
- **Gestures in the queue:** swipe right to play a song next (nothing else moves), swipe left to
  remove it (the − button is still there), hold and drag to move it. Works in all three parts. 🧪

**Where it's playing from**
- The player says **PLAYING FROM** at the top ("Playlist · Road trip", "Mood · Chill", "Liked Songs",
  "Smart Shuffle"…).
- **Listen Now** shows a "Playing from" card; tap it to open that playlist, album or list.
- **Library** highlights what's playing: the playlist, Liked Songs, Downloaded or On This Device row
  turns accent-coloured with a "Playing" tag.

**Sound you can hear** (you said the enhancements didn't do much, and the measurements agreed)
- Every preset was rebuilt and **measured on the engine with pink noise**:
  - **Bass** lows **+6.3 dB** (the old one: **+0.4 dB**, practically nothing: it boosted a narrow
    peak at 60 Hz where little music lives; now it's a proper low shelf).
  - **Treble** highs +5.4 dB (old +4.2). **Vocal** cuts boom by 10.8 dB and lifts presence 5.3 dB.
    **Loudness** lifts lows and highs and gently brings quiet passages up.
  - **Wide** stereo +10.9 dB wider (old +6.3); **Immersive** +11.7 dB with a short room echo;
    **Headphone** crossfeed brings the sound out of your head (narrower, by design).

**Taste**
- Already in (since last week): an artist's score halves after 60 days, so what you like now
  counts most. **New:** a song you skip early isn't picked for you again for 30 days (unless you
  finish it in between). 🧪

**Listen together (M7)**, built like Warptron's party mode, on iroh
- Listen Now › 👥 (top right) › **Host a party**, then share the link. Friends paste it in
  **Join**. Everyone hears the same song at the same moment; anyone can play, pause, skip and seek,
  and guests can **Suggest to the party** from any song's menu. The host sees who's there.
- Peer to peer, no accounts and no servers of our own (iroh's public relays only help devices find
  each other). Each device streams the song itself; only a few hundred bytes of "what's playing,
  where" cross the connection, corrected for the trip time, repeated every few seconds.
- ✅ **Checked over the real internet**: a guest joined with only the link (through iroh's
  Asia-Pacific relay) and was in sync within 436 ms. 🧪 host/guest sync, controls, suggestions,
  leaving and ending, on a local connection (5 of 5 runs).

**Sorting (M5)**
- Playlists: Custom order (drag), Title or Artist, remembered per playlist.
- Artists and Albums & Podcasts: Recently added or Name.
- A **filter box** in Liked Songs and Downloaded.

**Platform (M6)**
- **Android Auto:** Harp shows up in the car with Smart Shuffle, Liked Songs, Playlists, Downloaded
  and Recently Played to browse and play. 📱 needs a car or the Android Auto head unit to try.
- **Mini player (laptop):** the ⧉ button on the player shrinks Harp to a small strip with the cover
  and controls; ⤢ brings it back. It asks to stay on top; GNOME on Wayland doesn't let apps do that,
  so there: right-click its title bar › Always on Top.
- Not done, and why: a **tray icon** (GNOME doesn't show tray icons without an extension, and its
  own media controls already show Harp); **Discord status** (needs an app registered on Discord's
  developer site, i.e. a key: your call).

## 2026-11-09 (late night): moods and Smart Shuffle play again

- **Fixed: "Couldn't play …: the value of User-Agent must not contain … comma".** It hit songs that
  stream straight from YouTube (moods and Smart Shuffle pick new songs; songs already cached play from
  disk, so they were fine). yt-dlp gives browser headers whose values contain commas (the
  User-Agent's "(KHTML, like Gecko)", Accept, Accept-Language), and the new engine refuses commas in
  per-song headers. Now the User-Agent goes through the player's own setting, which has no such
  limit, and the browser-only headers are left out. ✅ reproduced your exact error with a real
  YouTube stream on the laptop, then played it with the fix. 🧪

## 2026-11-09 (night, fixes): skips play again, light-mode player, the original waves

- **Fixed: skipping (and autoplay) left the next song paused.** The new engine doesn't start a
  newly opened song unless told to; the old one did. Now it's told to. ✅ reproduced and checked on
  the laptop's engine (without the fix the next song loads paused; with it, it plays).
- **The player follows light mode.** In light mode it's a pale wash of the cover's colour with dark
  text and controls; in dark mode, as before.
- **The original waves are back** (the soft translucent ones you liked better). The difference: they
  now rise with the song's real loudness, with the spectrum adding a subtle lift where the energy
  is. White in dark mode, a deep tint of the cover in light mode.
- **The little bar at the top of the player is gone.** It looked like a handle but did nothing.
  Swipe down on the player (or the ⌄ button) still closes it.

## 2026-11-09 (night): waves are back, now driven by the real music

You preferred the waves to the colour clouds, so the clouds are gone. The player background is the
blurred cover again, with **three layered waves along the bottom shaped by the live spectrum**:
bass lifts the middle and treble ripples the edges, so a kick drum swells the centre and
cymbals move the sides. They're in the cover's own colours with a soft bright crest, drift slowly,
and settle when you pause. With effects reduced they still show, holding still. Cheap to draw
(plain shapes, 30 frames a second). 🧪 they follow the bands and stop redrawing when paused; checked
by eye on a rendered frame.

## 2026-11-09 (M2 step 2): the new audio engine plays everything

Your test passed on the phone (every effect and the live spectrum; only true 3D is missing), so
**Harp now plays through the new engine (`mpv_audio_kit`) everywhere**. The old player is removed.

**What this brings to Android**
- **Equalizer presets** (Bass, Vocal, Treble, Loudness), **spatial audio** (Wide stage, Headphone
  crossfeed, Immersive) and **Even volume** in Settings › Playback now work on the phone too.
- **Even volume is now proper loudness normalisation** (EBU R128, the broadcast standard), which
  the old engine couldn't run.
- **Effects change instantly**, mid-song, without the song restarting.
- **The player background follows the real music** (its live spectrum, bass counting most) instead
  of a simulated beat, on the phone and the laptop.
- "Virtual 3D" is renamed **Immersive (wide + crossfeed)**: the engine has no true 3D (HRTF) filter,
  so the name was promising too much.

**Checked:** 🧪 all 40 combinations of equalizer × spatial × even volume play without a stall on the
laptop's engine; the APK carries the new engine and no longer the old one. The equalizer presets
first failed in that test (this engine has no `volume` filter for their headroom), so the
headroom now comes from the engine's own volume gain.

**Please test on the phone:** playing and skipping, the notification and lock screen controls,
unplugging headphones, a long screen-off, and Settings › Playback (equalizer, spatial, even volume).
The engine check is still in Settings › About if you want to re-run it.

## 2026-11-09 (M2 step 1): testing the new audio engine

The Android audio engine is under way (roadmap M2). This build adds **a self-check, nothing else
changes**: Settings › About › **Test the new audio engine**. It plays the current song (or your last
one) through `mpv_audio_kit` for about 30 seconds, tries each effect, and shows a report with a
**Copy** button. Please run it on the phone and send me the report.

- **On the laptop it already passes:** equalizer, stereo widen, headphone crossfeed, both volume
  levellers (even FFmpeg's `loudnorm`, which stalls the current player) and the live spectrum.
- **True 3D (`sofalizer`) is not included** in its FFmpeg build, so the "Virtual 3D" mode will need
  another route later; Wide stage and Crossfeed are fine.
- The APK grows by about 8 MB (the engine's library) for now; the old one goes once we switch.

## 2026-11-09 (latest): background fix, lyrics position, your own music files, restore adds

**Fixed**
- **The reactive background could bring the phone down.** It drew big full-screen clouds with
  "screen" blending and blurred rings, 60 times a second. On Android's graphics engine, each of those
  draws can force a copy of the whole screen, which can overwhelm a phone's GPU. Now the scene is
  drawn at 54 pixels wide and scaled up (the blur of scaling is the look anyway), with normal blending
  and no blur filters, at 30 frames a second: a tiny fraction of the work, the same look (checked on
  a rendered frame). 📱 please confirm.
- **Lyrics now open at the line being sung.** The list only builds the lines near the top, so when
  you opened lyrics mid-song the current line didn't exist yet and the scroll gave up. It now jumps
  near the line first, then lands on it exactly. 📱
- **Restoring a backup adds to your library instead of replacing it** (you asked). Likes, follows,
  saved albums and history are added; a playlist with the same name gets the songs it's missing (at
  the end); new playlists are created; your settings, queue and folders stay. It happens right away,
  with no restart, and restoring the same backup twice adds nothing more. 🧪

**Next milestones (M5)**
- **Your own music files:** Library › On This Device. Add folders; Harp reads titles, artists,
  albums, lengths and embedded covers (or uses "Artist - Title" from the file name), and they play
  offline like downloads, with likes, playlists, the queue and recommendations ("Because you played"
  finds the song online to suggest similar ones). Filter and sort, rescan, remove folders.
  🧪 on real tagged and untagged files. Android asks for the music permission (📱).
- **Smarter picks:** taste now fades (an artist's score halves after 60 days without plays, so what
  you like now counts more than a year ago), and artists you usually play around this hour get a
  nudge (morning music in the morning). 🧪

## 2026-11-09 (late): browse charts and genres, lyrics tools, even volume

**Browse (M5)**
- The Search tab now shows a few recent searches, then **Browse**:
  - **Charts** for your country (picked from your device's region): top songs and videos, charts
    by language, and the top artists.
  - Every YouTube Music **mood and genre** as a coloured tile: 12 moods and 38 genres, each
    opening rows of playlists.
- ✅ checked live (India: 13 rows for "Chill"; the charts with 40 top artists), and a playlist from
  a mood opens and plays. 🧪 on saved real pages.

**Lyrics (M4)**
- **Wrong lyrics, or none? Search by name.** The ⚙ button at the top of the lyrics (or the button
  under "No lyrics found") lets you type the title and artist; what it finds is kept for that song.
- **Timing nudge:** if the lines run ahead of or behind the singing, move them by half a second at
  a time. Remembered per song.

**Sound (M3), desktop**
- **Even volume** (Settings › Playback): quiet songs come up and loud ones down. It uses a slow
  volume leveller. I first tried FFmpeg's standard loudness filter, but it **stalls playback**
  in libmpv (checked), so it isn't used. ✅ the leveller plays together with the equalizer,
  spatial audio and the loudness reader. Android waits for the new audio engine (M2).

## 2026-11-09 (night, last batch): playlists, top result, following artists

**Asked for**
- **New songs go on top of a playlist** (a batch you add at once keeps its order). 🧪
- **Downloaded playlists stay downloaded.** Choosing "Download All" on a playlist also turns on
  auto-download: songs you add later download by themselves. The playlist says so under its name,
  and its menu has "Stop downloading new songs".
- **Search's top result** now has the ⋮ menu (and a long press): Play next, Add to queue, Add to
  playlist, Download, Share, without playing it first.

**Next milestone (M5): following artists**
- **Follow artists** with the button on their page, and **save albums, playlists and podcasts**
  with the + at the top of theirs. They're in Library › Artists and Library › Albums & Podcasts. 🧪
- **New Releases** on Listen Now: the newest albums and singles (this year and last) from the
  artists you follow, newest first, mixed so one artist doesn't fill the shelf. Refreshed every
  12 hours or when you pull to refresh. ✅ checked live on an artist page.

**Fixed (my mistake):** the previous update overwrote a saved test response that another test
used, so one Rust test was failing. It never reached a build; it's restored and all tests pass.

## 2026-11-09 (night, later still): a new player background, more lyrics

**Player**
- **The star button is gone.** It only liked the song, like the heart. In its place is **Add to
  playlist**. Superlike ("more like this, sooner") is now a long press on the heart, which turns
  into a sparkle.
- **A new music-reactive background** in place of the waves. It's made from the cover's own
  colours (up to four, picked from the artwork): large soft clouds of light that drift and blend,
  swell and brighten with the loudness, and move faster when the music has more energy. A beat
  or drop sends a soft ring of light out from behind the artwork. New songs fade from the old
  colours to the new. It holds still when paused or with effects reduced, so it costs nothing
  then. 🧪 renders, rings on beats, stops ticking when paused (and checked by eye on a rendered frame).
  On Android, loudness is still simulated until the new audio engine (M2), so there the motion
  follows a beat-like pulse, not the real song.

**Lyrics (M4): three sources instead of one**
- In order: LRCLIB synced → **NetEase** synced (no key needed; strong for Hindi and other Indian
  and Asian songs, often already in English letters) → LRCLIB plain → **YouTube Music's own
  lyrics** (from LyricFind, without timings, often in the original script, which the अ/A switch
  then spells out in English letters). The source is shown under the lyrics.
- Songs that had no lyrics before are looked up again once (a one-time database update).
- ✅ checked live on "Tum Hi Ho": NetEase gave synced lines, YouTube Music the Devanagari text.
  🧪 parsers tested on saved real responses (credit lines like "作曲 : …" are dropped,
  instrumentals give nothing, a wrong length or artist doesn't match).

## 2026-11-09 (night, final): a livelier home, equalizer, speed, backup

**Home**
- **"Harp" at the top with an animated logo**, instead of "Listen Now". The logo writes itself in
  when the page opens, and while music plays its strike line quivers like a plucked harp string,
  harder on louder parts. It stays still when paused or when background effects are off.
- **Refresh button only on the laptop.** On the phone, pull down to refresh.
- **Moods are pictures now:** each has its own icon art, soft rings and a two-colour gradient.
- **Your Artists and Albums for You:** your most played artists (plays count once, liked songs
  three times) with their pictures, and albums by them. They're looked up side by side and saved
  with the home picks, so they show instantly next time. 🧪 ranking

**From the milestone map**
- **Equalizer presets (M3), desktop:** Flat, Bass boost, Vocal, Treble boost, Loudness (Settings ›
  Playback). Boosting presets lower the volume a little first so nothing clips. ✅ every preset
  played in libmpv together with spatial audio and the loudness reader, without stalling. Android
  waits for the new audio engine (M2).
- **Playback speed (M3):** 0.75× to 2× with the pitch kept, from the new speed button in Now
  Playing or in Settings. The notification knows the speed, so its bar keeps up.
- **Long tracks remember where you stopped (M3):** podcast episodes, mixes and anything 10 minutes
  or longer start where you left them, each one separately. Finished ones forget.
- **Sorting (M5):** Liked Songs and Downloaded by recently added, title or artist; Library
  playlists by recent or name (pinned stay on top). Your choice is remembered. 🧪
- **Backup and restore (M5):** Settings › Backup saves your whole library (likes, playlists,
  history, taste, settings, saved lyrics) as one `.harp` file. Restoring checks the file, shows
  what's in it, and switches over the next time Harp starts, keeping the current library aside
  as a safety copy. Downloads from another device that aren't on this one are dropped from the list. 🧪
- **Share a song:** "Share link" in every song menu opens the share sheet on Android, and copies
  the link on the laptop.

**Tested:** ✅ 64 core tests, Flutter analyzer clean, 19 widget/unit tests, EQ chains in libmpv.
**Not verified:** the phone (share sheet, speed in the notification, the home artist lookups on
mobile data).

## 2026-11-09 (late night): mini player everywhere, pause on unplug, Hinglish lyrics, playlist covers

**Fixed**
- **The mini player no longer disappears** when you open a playlist, album, artist or list. Each
  tab keeps its own pages, so the player and tab bar stay on screen. Back closes pages in the tab
  first; tapping the tab you're on goes back to its start. Song menus open above the tab bar.
- **Pauses when headphones are unplugged or Bluetooth disconnects** (Android), instead of
  playing out of the speaker. Pauses for calls and resumes after; ducks for navigation prompts. 📱
- **Missing progress bar in the notification, explained and fixed.** Songs found through search
  often arrive without a length, and Android only draws the bar when it knows the length; Harp
  never told it the real one once playback started. Now it does, as soon as the player knows,
  and also tells it about seeks so the bar doesn't drift. That's why it happened only sometimes
  (songs that came with a length were fine). 📱
- **Playlists have pictures:** the first song's cover, or a 2×2 mosaic once there are four songs,
  in Library and on Listen Now.
- **Library playlists are compact rows** (cover, name, count) instead of the oversized grid.

**Added**
- **Pin playlists** to the top of the Library: … menu or long press. Saved in the database (the
  database now upgrades itself in place; 🧪 an old database upgrades without losing anything).
- **Lyrics in English letters (Hinglish).** Hindi, Urdu, Punjabi, Bengali, Gujarati, Tamil, Telugu,
  Kannada, Malayalam and Odia lyrics can show as typed in English letters: "तुम ही हो" → "tum hi
  ho". A switch at the top of the lyrics (अ / अ+A / A) picks original, both, or English letters only;
  it is remembered. Done on the device, no service needed. 🧪 Urdu is approximate (its script leaves
  out most vowels), and spellings may differ from how a song is usually written.
- **Downloads grouped by playlist:** each playlist with downloaded songs shows as its own group
  ("12 of 40 songs downloaded", Play, Shuffle); songs downloaded on their own are listed under Songs.
- **Recommendations learn from your saved playlists:** songs in them seed picks, with the reason
  shown. 🧪

**Tested:** ✅ 62 core tests, Flutter analyzer clean, 16 widget tests. **Not verified:** everything
marked 📱 needs your phone; output-change pausing is Android-only for now (Linux doesn't report it).

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

- Planned the work in `ROADMAP.md`. Name decided: **Harp**. GitHub repo created (`TechPsychotic/harp`).
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
