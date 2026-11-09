/// Headers for a stream, the way the audio engine accepts them.
///
/// yt-dlp gives browser-like headers. The engine passes per-song headers to
/// mpv as one comma-separated list, so it refuses any value containing a
/// comma, and browser values often do: the User-Agent ("(KHTML, like Gecko)"),
/// Accept and Accept-Language. So:
/// * the User-Agent goes through mpv's own `user-agent` option instead (no
///   such limit);
/// * other comma-valued headers are left out: they describe what a browser
///   would accept and don't matter for fetching the audio.
({Map<String, String>? headers, String? userAgent}) engineHeaders(List<(String, String)> raw, {required bool local}) {
  if (local) return (headers: null, userAgent: null);
  String? ua;
  final out = <String, String>{};
  for (final (k, v) in raw) {
    if (k.toLowerCase() == 'user-agent') {
      ua = v;
    } else if (!v.contains(',') && !v.contains('\n') && !v.contains('\r') && !v.contains('\u0000') && !k.contains(':')) {
      out[k] = v;
    }
  }
  return (headers: out.isEmpty ? null : out, userAgent: ua);
}
