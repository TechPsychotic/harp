# Harp

A standalone, account-free music app for Linux, Windows, macOS and Android.
Search and stream through yt-dlp, a glassmorphic UI, and a recommender that learns your
taste **on the device** (no sign-in, no API keys, no servers of ours).

- `ROADMAP.md`: what Harp is and where it is going
- `IMPLEMENTATION_PLAN.md`: how it gets built, phase by phase
- `crates/harp-core`: songs, queue, taste store, recommender
- `crates/harp-ytdlp`: yt-dlp runner, installer, Android loopback proxy
- `app/`: the Flutter app

Streaming from YouTube with yt-dlp goes against YouTube's terms of service; Harp is for
F-Droid and direct downloads, not Google Play.
