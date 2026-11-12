# Harp error codes

When a song can't play or download, Harp shows a code like **H-101** with a short explanation. Tap the
message (**Details**) to see exactly what happened, and copy it if you report a problem.

| Code | What it means | What to do |
| --- | --- | --- |
| **H-101** | The music source asked Harp to slow down (too many requests from your network). | Wait a minute or two and try again. Harp pauses fetching in the background meanwhile, and retries over your connection's other address type (IPv4/IPv6) automatically. On mobile data, switching to Wi-Fi (or the other way round) often helps. |
| **H-102** | The connection dropped while fetching the song. | Check your internet connection and try again. |
| **H-103** | The song isn't available: removed, made private, or not offered in your region. | Pick another version of the song from search. |
| **H-104** | You're in offline mode and this song isn't downloaded. | Go online, or download the song while you are. |
| **H-105** | The song was found but its audio couldn't be opened by the player. | Try again; if it keeps happening with one song, report it with the details. |
| **H-106** | A file from your own music is no longer on the device. | Library › On This Device › rescan your folders. |
| **H-107** | The music source took too long to answer. | Try again; a slow or busy connection is the usual cause. |
| **H-108** | The song needs a signed-in account (age-restricted or members-only). | Harp works without accounts, so these can't be played. Pick another version. |
| **H-199** | Something else went wrong. | Tap **Details**, copy the text and open an issue with it. |

Reporting a problem: open an issue on GitHub with the code, the copied details, your platform and Harp
version (Settings › About).
