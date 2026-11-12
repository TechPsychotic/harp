/// Harp's own error codes. Every playback problem shows one (with a plain
/// explanation); ERRORS.md in the repository lists them all, with what to do.
library;

class HarpError {
  const HarpError(this.code, this.message);
  final String code;
  final String message;
}

const sourceBusy = HarpError('H-101', 'The music source asked Harp to slow down. Harp has paused fetching in the background; try this song again in a minute or two.');
const network = HarpError('H-102', 'The connection dropped. Check your internet and try again.');
const unavailable = HarpError('H-103', 'This song isn’t available: it was removed, made private, or isn’t offered in your region.');
const offlineNotDownloaded = HarpError('H-104', 'You’re offline and this song isn’t downloaded.');
const playbackFailed = HarpError('H-105', 'The song’s audio couldn’t be opened. Try it again.');
const fileMissing = HarpError('H-106', 'This file is no longer on the device. Rescan your folders in Library › On This Device.');
const timedOut = HarpError('H-107', 'The music source took too long to answer. Try again.');
const restricted = HarpError('H-108', 'This song needs a signed-in account (age-restricted or members-only), so Harp can’t play it.');
const unknown = HarpError('H-199', 'Something went wrong. Tap Details to see what, and include it if you report it.');

/// Which Harp error a raw message (from the fetcher or the audio engine) is.
HarpError classify(String raw) {
  final l = raw.toLowerCase();
  bool any(List<String> words) => words.any(l.contains);
  // Order matters: "confirm your age" must not read as a rate limit.
  if (any(['confirm your age', 'age-restricted', 'age restricted', 'members-only', 'join this channel', 'premium'])) return restricted;
  if (any(['not a bot', 'confirm you', 'http error 429', 'too many requests'])) return sourceBusy;
  if (any(['offline mode'])) return offlineNotDownloaded;
  if (any(['no longer on the device'])) return fileMissing;
  if (any(['timed out', 'timeout'])) return timedOut;
  if (any(['video unavailable', 'not available', 'unavailable', 'private video', 'has been removed', 'removed by', 'copyright', 'blocked'])) return unavailable;
  if (any(['502', '503', 'tunnel', 'urlopen', 'network', 'connection', 'resolve host', 'name resolution', 'unreachable'])) return network;
  return unknown;
}

/// The raw message, with the names of the services Harp fetches from replaced
/// by "source" (for showing and copying).
String neutral(String raw) => raw
    .replaceAll(RegExp(r'https?://(www\.|music\.|m\.)?youtube\.com\S*', caseSensitive: false), '<source link>')
    .replaceAll(RegExp(r'youtube\s*music', caseSensitive: false), 'source')
    .replaceAll(RegExp(r'youtube', caseSensitive: false), 'source')
    .replaceAll(RegExp(r'googlevideo\.com', caseSensitive: false), 'source');
