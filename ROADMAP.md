# Harp: roadmap

**Harp** is a standalone music app: search and stream
songs through **yt-dlp**, queues, playlists, a library, and bringing your playlists over from
Spotify, Apple Music and Amazon Music. It is its own app, built from scratch.

Status: **in progress** (see `IMPLEMENTATION_PLAN.md`). Built from scratch; the Android
yt-dlp technique follows the sibling project zevro (static Python + QuickJS shipped as
native libraries, yt-dlp zipapp updated at runtime).

> Streaming from YouTube with yt-dlp goes against YouTube's terms of service. It is widely
> done (NewPipe, ViMusic, InnerTune, …) but rules out Google Play: F-Droid and our own site.

## Principles

- **No accounts, no servers of ours.** Searches and streams go from the device; library and
  playlists live on the device (synced between your own devices peer to peer later).
- **Sources are plugins.** yt-dlp (YouTube / YouTube Music) first; local files; later other
  yt-dlp sites (SoundCloud, Bandcamp, …). A song is *what* it is (title, artist, album,
  length, ISRC when known) with one or more *where* it plays from.
- **One core on every platform.** Rust core (search, resolving, cache, library, queue),
  Flutter UI; Android, Linux, Windows, macOS; iOS when yt-dlp can run there.

## Milestones

### 1. Player
- yt-dlp runner: computers download the official single-file build (checked against the
  release's SHA-256 sums, refreshed weekly); Android ships a static Python and QuickJS as native libraries
  and runs the same verified yt-dlp zipapp, updated without app updates.
- Search (YouTube Music first; plain YouTube's flat search gives artist / length / picture,
  tidied from "Artist - Title (Official Video)").
- Resolve to the best audio stream (cache the URL until shortly before it expires).
- Queue that behaves like people expect: tapping a search result plays **that song** (not the
  whole result list); *play next* puts it right after the current one; *add to queue* at the
  end of what the user added (before any automatic "up next"); an optional, clearly separate
  **autoplay / radio** section after the user's queue.
- Shuffle, repeat one / all, gapless where possible.
- Background playback with media controls (Android MediaSession + notification + lock
  screen, MPRIS on Linux, SMTC on Windows, Now Playing on macOS); headset buttons.

### 2. Library
- Playlists: create, rename, reorder, delete, save the queue, play / shuffle a playlist.
- Liked songs, recently played, albums and artists pages (YouTube Music metadata).
- Downloads for offline listening (size limit), lyrics (LRCLIB, synced where available),
  song radio / "more like this".
- Import / export playlists as files (M3U, JSON, CSV).

### 3. Bring your library over
Matching by title + artist + length (ISRC when given), with a review screen for weak matches.
- **Spotify:** the Web API is limited (development mode: 5 users, Premium owner) → import
  from **Spotify's data export** (Account → Privacy → download your data) and public playlist
  links; optionally the Web API with the user's own developer app.
- **Apple Music:** MusicKit needs a paid developer account → import **exported playlists**
  (Music → File → Library → Export Playlist) and public playlist links.
- **Amazon Music:** no public API → shared playlist links and copied lists, with matching.
- **YouTube Music:** your playlists by link (public / unlisted).

### 4. Together
- Listening rooms (shared queue, roles, everyone in step), peer to peer, no servers of ours.
- Share a song or playlist as a link / card.

### 5. Release
- F-Droid, GitHub Releases (signed APKs), Flathub / AppImage, Windows installer, macOS DMG.

## Open questions
- iOS: yt-dlp needs Python; App Store rules forbid downloading code → bundle a frozen
  yt-dlp (updated with the app) or resolve on a paired device.
- Whether to also offer Spotify App Remote for Premium users (exact control of the Spotify
  app, but only 5 users per developer app).
