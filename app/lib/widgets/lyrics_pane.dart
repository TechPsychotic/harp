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

  /// Lyrics timing nudge for this song, ms. Positive: show each line later.
  int _offset = 0;

  /// Each line in English letters ("" where the line is already in them).
  List<String> _roman = [];
  bool get _hasRoman => _roman.any((r) => r.isNotEmpty);

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

  void _setLyrics(api.LyricsView? l) {
    _lyrics = l;
    _keys = List.generate(l?.lines.length ?? 0, (_) => GlobalKey());
    _roman = l == null ? [] : api.romanizeLines(lines: [for (final x in l.lines) x.text]);
    _active = -1;
  }

  /// Look the lyrics up under a title and artist you type (for messy names,
  /// or when the wrong song's lyrics showed).
  Future<void> _search() async {
    final t = TextEditingController(text: widget.song.title), a = TextEditingController(text: widget.song.artist);
    final go = await showDialog<bool>(
      context: context,
      useRootNavigator: true,
      builder: (ctx) => AlertDialog(
        title: const Text('Find lyrics'),
        content: Column(mainAxisSize: MainAxisSize.min, children: [
          TextField(controller: t, decoration: const InputDecoration(labelText: 'Song title')),
          TextField(controller: a, decoration: const InputDecoration(labelText: 'Artist')),
        ]),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
          FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Search')),
        ],
      ),
    );
    if (go != true || !mounted) return;
    setState(() { _loading = true; _error = null; });
    try {
      final l = await api.lyricsFor(s: widget.song, allowNetwork: false); // what we had, in case nothing better turns up
      final found = await api.lyricsFind(s: widget.song, title: t.text.trim(), artist: a.text.trim());
      if (!mounted) return;
      setState(() { _setLyrics(found ?? l); _loading = false; });
      if (found == null) ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('No lyrics found under that name')));
    } catch (e) {
      if (mounted) setState(() { _error = 'Lyrics are unavailable right now.'; _loading = false; });
    }
  }

  void _nudge(int ms) {
    setState(() { _offset = ms == 0 ? 0 : _offset + ms; _active = -1; });
    api.settingSet(key: 'lyr_off:${widget.song.id}', value: _offset == 0 ? '' : '$_offset');
  }

  Future<void> _load() async {
    setState(() { _loading = true; _error = null; });
    final id = widget.song.id;
    _offset = int.tryParse(await api.settingGet(key: 'lyr_off:$id') ?? '') ?? 0;
    try {
      final l = await api.lyricsFor(s: widget.song, allowNetwork: !widget.player.settings.offline);
      if (!mounted || id != widget.song.id) return;
      setState(() {
        _setLyrics(l);
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
    final first = _active == -1; // just opened, or the song changed
    _active = idx;
    if (idx < 0 || idx >= _keys.length) return;
    WidgetsBinding.instance.addPostFrameCallback((_) => _scrollTo(idx, animate: !first, tries: 4));
  }

  /// Brings line [idx] to the reading position. The list only builds the
  /// lines near the screen, so a far line (opening the lyrics mid-song) has
  /// no position yet: jump near it by estimate first, then land exactly.
  void _scrollTo(int idx, {required bool animate, required int tries}) {
    if (!mounted || idx != _active) return;
    final ctx = _keys[idx].currentContext;
    if (ctx != null) {
      Scrollable.ensureVisible(
        ctx,
        alignment: 0.38,
        duration: animate ? const Duration(milliseconds: 350) : Duration.zero,
        curve: Curves.easeOutCubic,
      );
      return;
    }
    if (tries == 0 || !_scroll.hasClients) return;
    // Estimate from the lines already laid out (about 50 px each otherwise).
    final pos = _scroll.position;
    final perLine = pos.maxScrollExtent > 0 && _keys.isNotEmpty ? (pos.maxScrollExtent + pos.viewportDimension) / (_keys.length + 1) : 50.0;
    _scroll.jumpTo((60 + idx * perLine - pos.viewportDimension * 0.38).clamp(0.0, pos.maxScrollExtent));
    WidgetsBinding.instance.addPostFrameCallback((_) => _scrollTo(idx, animate: false, tries: tries - 1));
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
      return Center(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Text('No lyrics found for this song.', style: TextStyle(color: tok.textDim)),
          const SizedBox(height: 8),
          OutlinedButton.icon(onPressed: _search, icon: const Icon(Icons.search_rounded), label: const Text('Search by name')),
        ]),
      );
    }
    final settings = widget.player.settings;
    final mode = _hasRoman ? settings.lyricsScript : 'original';
    final list = ValueListenableBuilder<Duration>(
      valueListenable: widget.player.position,
      builder: (context, pos, _) {
        final idx = l.synced ? _indexAt(pos.inMilliseconds - _offset) : -1;
        if (l.synced) _follow(idx);
        return ListView.builder(
          controller: _scroll,
          padding: const EdgeInsets.symmetric(vertical: 60, horizontal: 8),
          itemCount: l.lines.length + 1,
          itemBuilder: (_, i) {
            if (i == l.lines.length) {
              final note = mode == 'original' ? '' : '\nEnglish letters are spelled out by Harp, so they may differ from the usual spelling.';
              return Padding(padding: const EdgeInsets.only(top: 24), child: Text('Lyrics from ${l.source}$note', textAlign: TextAlign.center, style: TextStyle(color: tok.textFaint, fontSize: 12)));
            }
            final line = l.lines[i];
            final on = i == idx;
            final past = l.synced && i < idx;
            return InkWell(
              key: _keys[i],
              borderRadius: BorderRadius.circular(Radii.control),
              onTap: l.synced ? () => widget.player.seekTo(Duration(milliseconds: (line.atMs + _offset).clamp(0, 1 << 31))) : null,
              child: AnimatedDefaultTextStyle(
                duration: const Duration(milliseconds: 250),
                style: TextStyle(
                  fontSize: on ? 28 : 24,
                  height: 1.3,
                  letterSpacing: -0.3,
                  fontWeight: FontWeight.w800,
                  color: l.synced ? (on ? tok.text : tok.text.withValues(alpha: past ? 0.30 : 0.42)) : tok.text.withValues(alpha: 0.85),
                ),
                child: Padding(padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 8), child: _line(line.text, i < _roman.length ? _roman[i] : '', mode)),
              ),
            );
          },
        );
      },
    );
    final tools = Positioned(
      top: 4,
      left: 4,
      child: Row(children: [
        PopupMenuButton<int>(
          tooltip: 'Lyrics options',
          icon: Icon(Icons.tune_rounded, color: tok.textDim),
          onSelected: (v) => v == 1 ? _search() : _nudge(v == 2 ? -500 : (v == 3 ? 500 : 0)),
          itemBuilder: (_) => [
            const PopupMenuItem(value: 1, child: Text('Wrong lyrics? Search by name')),
            if (l.synced) ...[
              const PopupMenuItem(value: 2, child: Text('Lines come too late (−0.5 s)')),
              const PopupMenuItem(value: 3, child: Text('Lines come too early (+0.5 s)')),
              if (_offset != 0) const PopupMenuItem(value: 0, child: Text('Reset timing')),
            ],
          ],
        ),
        if (_offset != 0) Text('${_offset > 0 ? '+' : '−'}${(_offset.abs() / 1000).toStringAsFixed(1)} s', style: TextStyle(color: tok.textDim, fontSize: 12)),
      ]),
    );
    if (!_hasRoman) return Stack(children: [list, tools]);
    return Stack(children: [
      list,
      tools,
      Positioned(
        top: 4,
        right: 8,
        child: ListenableBuilder(
          listenable: settings,
          builder: (_, _) => SegmentedButton<String>(
            showSelectedIcon: false,
            style: const ButtonStyle(visualDensity: VisualDensity.compact, tapTargetSize: MaterialTapTargetSize.shrinkWrap),
            segments: const [
              ButtonSegment(value: 'original', label: Text('अ'), tooltip: 'Original script'),
              ButtonSegment(value: 'both', label: Text('अ+A'), tooltip: 'Both'),
              ButtonSegment(value: 'roman', label: Text('A'), tooltip: 'English letters'),
            ],
            selected: {settings.lyricsScript},
            onSelectionChanged: (v) {
              settings.setLyricsScript(v.first);
              setState(() {});
            },
          ),
        ),
      ),
    ]);
  }

  /// A line as the chosen script setting asks: original, English letters, or
  /// the original with English letters under it.
  Widget _line(String text, String roman, String mode) {
    if (text.isEmpty) return const Text('♪');
    if (roman.isEmpty || mode == 'original') return Text(text);
    if (mode == 'roman') return Text(roman);
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(text),
      Text(roman, style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600, height: 1.25)),
    ]);
  }
}
