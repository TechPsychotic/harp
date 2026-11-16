import 'dart:async';
import 'dart:io';

import '../native.dart';
import '../src/rust/api/engine.dart' as api;
import 'controller.dart';

/// Live lyrics for the "Harp now playing" home screen widget (Android), when it's set to show
/// them: follows the playing song's synced lyrics and hands the widget each new line. Does
/// nothing (no lookups) while no widget wants lyrics.
class WidgetLyrics {
  WidgetLyrics(this.c);
  final PlayerController c;

  String? _songId;
  api.LyricsView? _lyrics;
  int _offset = 0;
  int _idx = -2;
  bool _wanted = false;
  Timer? _check;

  void start() {
    if (!Platform.isAndroid) return;
    c.position.addListener(_tick);
    // A widget may be set to show lyrics at any time (its settings live outside the app).
    _check = Timer.periodic(const Duration(seconds: 15), (_) => _refreshWanted());
    unawaited(_refreshWanted());
  }

  void dispose() {
    _check?.cancel();
    c.position.removeListener(_tick);
  }

  Future<void> _refreshWanted() async {
    final was = _wanted;
    _wanted = await Native.widgetLyricsWanted();
    if (_wanted && !was) {
      _songId = null; // look the lyrics up now
      _tick();
    }
  }

  void _tick() {
    if (!_wanted) return;
    final s = c.current;
    if (s == null) return;
    if (s.id != _songId) {
      _songId = s.id;
      _lyrics = null;
      _idx = -2;
      unawaited(Native.widgetLyric(line: '', next: ''));
      unawaited(_load(s));
      return;
    }
    final l = _lyrics;
    if (l == null || !l.synced || l.lines.isEmpty) return;
    final i = _indexAt(l, c.position.value.inMilliseconds - _offset);
    if (i == _idx) return;
    _idx = i;
    String text(int k) => k < 0 || k >= l.lines.length ? '' : (l.lines[k].text.trim().isEmpty ? '♪' : l.lines[k].text.trim());
    // Before the first line: the first line as what's next.
    unawaited(Native.widgetLyric(line: i < 0 ? '♪' : text(i), next: text(i + 1)));
  }

  Future<void> _load(api.SongView s) async {
    try {
      final l = await api.lyricsFor(s: s, allowNetwork: !c.settings.offline);
      final off = int.tryParse(await api.settingGet(key: 'lyr_off:${s.id}') ?? '') ?? 0;
      if (_songId != s.id) return;
      _lyrics = l;
      _offset = off;
      _tick();
    } catch (_) {}
  }

  static int _indexAt(api.LyricsView l, int posMs) {
    var lo = 0, hi = l.lines.length;
    while (lo < hi) {
      final mid = (lo + hi) >> 1;
      if (l.lines[mid].atMs <= posMs) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    return lo - 1;
  }
}
