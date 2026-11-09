import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';

/// Lyrics that follow the song (synced lines from LRCLIB when they exist).
/// Tap a line to jump there.
class LyricsPane extends StatefulWidget {
  const LyricsPane({super.key, required this.player, required this.song});
  final PlayerController player;
  final api.SongView song;

  @override
  State<LyricsPane> createState() => _LyricsPaneState();
}

class _LyricsPaneState extends State<LyricsPane> {
  api.LyricsView? _lyrics;
  bool _loading = true;
  String? _error;
  int _active = -1;
  final _scroll = ScrollController();
  List<GlobalKey> _keys = [];

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(LyricsPane old) {
    super.didUpdateWidget(old);
    if (old.song.id != widget.song.id) {
      _lyrics = null;
      _active = -1;
      _load();
    }
  }

  Future<void> _load() async {
    setState(() { _loading = true; _error = null; });
    final id = widget.song.id;
    try {
      final l = await api.lyricsFor(s: widget.song, allowNetwork: !widget.player.settings.offline);
      if (!mounted || id != widget.song.id) return;
      setState(() {
        _lyrics = l;
        _keys = List.generate(l?.lines.length ?? 0, (_) => GlobalKey());
        _loading = false;
      });
    } catch (e) {
      if (mounted) setState(() { _error = 'Lyrics are unavailable right now.'; _loading = false; });
    }
  }

  int _indexAt(int posMs) {
    final lines = _lyrics!.lines;
    var lo = 0, hi = lines.length;
    while (lo < hi) {
      final mid = (lo + hi) >> 1;
      if (lines[mid].atMs <= posMs) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    return lo - 1;
  }

  void _follow(int idx) {
    if (idx == _active) return;
    _active = idx;
    if (idx < 0 || idx >= _keys.length) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final ctx = _keys[idx].currentContext;
      if (ctx != null && mounted) {
        Scrollable.ensureVisible(ctx, alignment: 0.38, duration: const Duration(milliseconds: 350), curve: Curves.easeOutCubic);
      }
    });
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_error != null) return Center(child: Text(_error!, style: TextStyle(color: tok.textDim)));
    final l = _lyrics;
    if (l == null || l.lines.isEmpty) {
      return Center(child: Text('No lyrics found for this song.', style: TextStyle(color: tok.textDim)));
    }
    return ValueListenableBuilder<Duration>(
      valueListenable: widget.player.position,
      builder: (context, pos, _) {
        final idx = l.synced ? _indexAt(pos.inMilliseconds) : -1;
        if (l.synced) _follow(idx);
        return ListView.builder(
          controller: _scroll,
          padding: const EdgeInsets.symmetric(vertical: 60, horizontal: 8),
          itemCount: l.lines.length + 1,
          itemBuilder: (_, i) {
            if (i == l.lines.length) {
              return Padding(padding: const EdgeInsets.only(top: 24), child: Text('Lyrics from ${l.source}', textAlign: TextAlign.center, style: TextStyle(color: tok.textFaint, fontSize: 12)));
            }
            final line = l.lines[i];
            final on = i == idx;
            final past = l.synced && i < idx;
            return InkWell(
              key: _keys[i],
              borderRadius: BorderRadius.circular(Radii.control),
              onTap: l.synced ? () => widget.player.seekTo(Duration(milliseconds: line.atMs)) : null,
              child: AnimatedDefaultTextStyle(
                duration: const Duration(milliseconds: 250),
                style: TextStyle(
                  fontSize: on ? 24 : 20,
                  height: 1.35,
                  fontWeight: on ? FontWeight.w800 : FontWeight.w600,
                  color: l.synced ? (on ? tok.text : tok.text.withValues(alpha: past ? 0.38 : 0.55)) : tok.text.withValues(alpha: 0.85),
                ),
                child: Padding(padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 8), child: Text(line.text.isEmpty ? '♪' : line.text)),
              ),
            );
          },
        );
      },
    );
  }
}
