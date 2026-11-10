import 'dart:async';

import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';

int _secs(DateTime d) => d.millisecondsSinceEpoch ~/ 1000;

/// Harp Encore (the year in review) opens from the second week of December
/// (the 8th) until the end of the first week of January.
bool encoreSeason([DateTime? now]) {
  final n = now ?? DateTime.now();
  return (n.month == 12 && n.day >= 8) || (n.month == 1 && n.day <= 7);
}

/// The year Encore is about: in January, the one that just ended.
int encoreYear([DateTime? now]) {
  final n = now ?? DateTime.now();
  return n.month == 1 ? n.year - 1 : n.year;
}

String minutesText(double m) {
  final v = m.round();
  if (v < 1000) return '$v';
  final s = '$v';
  final b = StringBuffer();
  for (var i = 0; i < s.length; i++) {
    if (i > 0 && (s.length - i) % 3 == 0) b.write(',');
    b.write(s[i]);
  }
  return b.toString();
}

String hourText(int h) => h == 0
    ? '12 am'
    : h < 12
    ? '$h am'
    : h == 12
    ? '12 pm'
    : '${h - 12} pm';

/// A one-line read of how someone listens, from when and how widely.
({String name, String line, IconData icon}) listenerKind(api.StatsView s) {
  final h = s.peakHour;
  final explorer = s.plays > 0 && s.artists / s.plays > 0.45;
  final when = h == null
      ? null
      : (h >= 22 || h < 4)
      ? ('Night Owl', 'Your music comes alive after dark.', Icons.nightlight_round)
      : h < 10
      ? ('Early Riser', 'You start the day with a soundtrack.', Icons.wb_twilight_rounded)
      : h < 17
      ? ('Daytime Drifter', 'Music keeps your days moving.', Icons.wb_sunny_rounded)
      : ('Evening Unwinder', 'Evenings are when you press play.', Icons.wb_twilight_rounded);
  if (when == null) return (name: 'Just Getting Started', line: 'Play a few songs and your rhythm shows up.', icon: Icons.music_note_rounded);
  final (n, l, i) = when;
  return (name: explorer ? '$n Explorer' : n, line: explorer ? '$l And you keep finding new artists.' : '$l You know what you love and stay with it.', icon: i);
}

/// Your Rhythm: how you listened this week, month or year. All on the device.
class RhythmScreen extends StatefulWidget {
  const RhythmScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<RhythmScreen> createState() => _RhythmScreenState();
}

class _RhythmScreenState extends State<RhythmScreen> {
  int _range = 0; // 0 week, 1 month, 2 year
  api.StatsView? _now;
  api.StatsView? _before;

  @override
  void initState() {
    super.initState();
    _load();
  }

  (DateTime, DateTime, DateTime) _bounds() {
    final n = DateTime.now();
    final today = DateTime(n.year, n.month, n.day);
    switch (_range) {
      case 0:
        final start = today.subtract(Duration(days: today.weekday - 1));
        return (start, start.add(const Duration(days: 7)), start.subtract(const Duration(days: 7)));
      case 1:
        return (DateTime(n.year, n.month), DateTime(n.year, n.month + 1), DateTime(n.year, n.month - 1));
      default:
        return (DateTime(n.year), DateTime(n.year + 1), DateTime(n.year - 1));
    }
  }

  Future<void> _load() async {
    final (start, end, prev) = _bounds();
    final now = await api.listeningStats(from: _secs(start), to: _secs(end), top: 10);
    final before = await api.listeningStats(from: _secs(prev), to: _secs(start), top: 1);
    if (!mounted) return;
    setState(() {
      _now = now;
      _before = before;
    });
  }

  List<(String, double)> _bars(api.StatsView s) {
    final (start, end, _) = _bounds();
    final byDay = {for (final d in s.byDay) d.day: d.minutes};
    String key(DateTime d) => '${d.year.toString().padLeft(4, '0')}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';
    if (_range == 2) {
      const names = ['J', 'F', 'M', 'A', 'M', 'J', 'J', 'A', 'S', 'O', 'N', 'D'];
      final months = List<double>.filled(12, 0);
      for (final d in s.byDay) {
        final m = int.tryParse(d.day.split('-').elementAtOrNull(1) ?? '') ?? 0;
        if (m >= 1 && m <= 12) months[m - 1] += d.minutes;
      }
      return [for (var i = 0; i < 12; i++) (names[i], months[i])];
    }
    const wd = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];
    final out = <(String, double)>[];
    for (var d = start; d.isBefore(end); d = DateTime(d.year, d.month, d.day + 1)) {
      final label = _range == 0 ? wd[d.weekday - 1] : (d.day == 1 || d.day % 5 == 0 ? '${d.day}' : '');
      out.add((label, byDay[key(d)] ?? 0));
    }
    return out;
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    final s = _now;
    const names = ['week', 'month', 'year'];
    return Scaffold(
      appBar: AppBar(title: const Text('Your Rhythm')),
      body: RefreshIndicator(
        onRefresh: _load,
        child: ListView(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 32),
          children: [
            SegmentedButton<int>(
              segments: const [
                ButtonSegment(value: 0, label: Text('This week')),
                ButtonSegment(value: 1, label: Text('This month')),
                ButtonSegment(value: 2, label: Text('This year')),
              ],
              selected: {_range},
              showSelectedIcon: false,
              onSelectionChanged: (v) {
                setState(() {
                  _range = v.first;
                  _now = null;
                });
                _load();
              },
            ),
            const SizedBox(height: 16),
            if (encoreSeason())
              Padding(
                padding: const EdgeInsets.only(bottom: 16),
                child: EncoreBanner(player: widget.player),
              ),
            if (s == null)
              const Padding(
                padding: EdgeInsets.all(40),
                child: Center(child: CircularProgressIndicator()),
              )
            else if (s.plays == 0)
              Padding(
                padding: const EdgeInsets.all(24),
                child: Text(
                  'Nothing played this ${names[_range]} yet. Your rhythm builds as you listen.',
                  textAlign: TextAlign.center,
                  style: TextStyle(color: tok.textDim),
                ),
              )
            else ...[
              GlassCard(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Minutes listened', style: TextStyle(color: tok.textDim, fontSize: 13)),
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.end,
                      children: [
                        Text(minutesText(s.minutes), style: const TextStyle(fontSize: 40, fontWeight: FontWeight.w800, height: 1.1)),
                        const SizedBox(width: 10),
                        if (_before != null && _before!.minutes > 0) Padding(padding: const EdgeInsets.only(bottom: 8), child: _change(context, s.minutes, _before!.minutes, names[_range])),
                      ],
                    ),
                    const SizedBox(height: 14),
                    SizedBox(
                      height: 120,
                      child: _BarChart(bars: _bars(s), color: accent),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: _Fact(label: 'Songs', value: '${s.songs}'),
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: _Fact(label: 'Artists', value: '${s.artists}'),
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: _Fact(label: 'Day streak', value: '${s.streak}'),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              _KindCard(stats: s),
              const SectionHeader('Time of day'),
              GlassCard(
                child: SizedBox(
                  height: 90,
                  child: _BarChart(bars: [for (var h = 0; h < 24; h++) (h % 6 == 0 ? hourText(h) : '', s.byHour[h])], color: accent),
                ),
              ),
              const SectionHeader('Top artists'),
              for (final (i, a) in s.topArtists.indexed)
                ListTile(
                  contentPadding: EdgeInsets.zero,
                  leading: SizedBox(
                    width: 72,
                    child: Row(
                      children: [
                        SizedBox(
                          width: 22,
                          child: Text(
                            '${i + 1}',
                            style: TextStyle(color: tok.textDim, fontWeight: FontWeight.w700),
                          ),
                        ),
                        Cover(url: a.thumbnail, seed: a.name, size: 48, round: true),
                      ],
                    ),
                  ),
                  title: Text(a.name, maxLines: 1, overflow: TextOverflow.ellipsis),
                  subtitle: Text('${minutesText(a.minutes)} min · ${a.plays} plays', style: TextStyle(color: tok.textDim)),
                ),
              const SectionHeader('Top songs'),
              for (final t in s.topSongs)
                SongTile(
                  song: t.song,
                  player: widget.player,
                  subtitle: '${t.song.artist} · ${t.plays} plays',
                  onPlay: () => widget.player.playList([for (final x in s.topSongs) x.song], from: (kind: 'rhythm', id: '', title: 'Your Rhythm')),
                ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _change(BuildContext context, double now, double before, String unit) {
    final pct = ((now - before) / before * 100).round();
    final up = pct >= 0;
    return Text(
      '${up ? '▲' : '▼'} ${pct.abs()}% vs last $unit',
      style: TextStyle(color: up ? const Color(0xFF3DDC84) : context.tok.textDim, fontSize: 13, fontWeight: FontWeight.w600),
    );
  }
}

class _Fact extends StatelessWidget {
  const _Fact({required this.label, required this.value});
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => GlassCard(
    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(value, style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w800)),
        Text(label, style: TextStyle(color: context.tok.textDim, fontSize: 12.5)),
      ],
    ),
  );
}

class _KindCard extends StatelessWidget {
  const _KindCard({required this.stats});
  final api.StatsView stats;

  @override
  Widget build(BuildContext context) {
    final k = listenerKind(stats);
    final accent = Theme.of(context).colorScheme.primary;
    final peak = stats.peakHour;
    return GlassCard(
      child: Row(
        children: [
          CircleAvatar(
            radius: 24,
            backgroundColor: accent.withValues(alpha: 0.18),
            child: Icon(k.icon, color: accent),
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(k.name, style: const TextStyle(fontWeight: FontWeight.w800, fontSize: 17)),
                Text('${k.line}${peak == null ? '' : ' Most listening around ${hourText(peak)}.'}', style: TextStyle(color: context.tok.textDim, fontSize: 13)),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _BarChart extends StatelessWidget {
  const _BarChart({required this.bars, required this.color});
  final List<(String, double)> bars;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final top = bars.fold<double>(0, (m, b) => b.$2 > m ? b.$2 : m);
    final gap = bars.length > 14 ? 2.0 : 6.0;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        for (final b in bars)
          Expanded(
            child: Padding(
              padding: EdgeInsets.symmetric(horizontal: gap / 2),
              child: Column(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  Flexible(
                    child: FractionallySizedBox(
                      heightFactor: top == 0 ? 0.02 : (b.$2 / top).clamp(0.02, 1.0),
                      child: Container(
                        decoration: BoxDecoration(color: b.$2 == top && top > 0 ? color : color.withValues(alpha: 0.45), borderRadius: BorderRadius.circular(3)),
                      ),
                    ),
                  ),
                  const SizedBox(height: 4),
                  SizedBox(
                    height: 14,
                    child: OverflowBox(
                      maxWidth: 40,
                      child: Text(b.$1, style: TextStyle(color: tok.textFaint, fontSize: 10)),
                    ),
                  ),
                ],
              ),
            ),
          ),
      ],
    );
  }
}

/// The December banner that opens Harp Encore.
class EncoreBanner extends StatelessWidget {
  const EncoreBanner({super.key, required this.player});
  final PlayerController player;

  @override
  Widget build(BuildContext context) {
    final y = encoreYear();
    return Material(
      borderRadius: BorderRadius.circular(Radii.card),
      clipBehavior: Clip.antiAlias,
      child: Ink(
        decoration: const BoxDecoration(
          gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [Color(0xFFFF5F6D), Color(0xFF7B2FF7), Color(0xFF1FA2FF)]),
        ),
        child: InkWell(
          onTap: () => Navigator.of(context).push(
            MaterialPageRoute(
              fullscreenDialog: true,
              builder: (_) => EncoreScreen(player: player, year: y),
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.all(18),
            child: Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Harp Encore $y',
                        style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w900, color: Colors.white),
                      ),
                      const SizedBox(height: 4),
                      const Text('Your year in music is ready', style: TextStyle(color: Colors.white70, fontSize: 13)),
                    ],
                  ),
                ),
                const CircleAvatar(
                  radius: 22,
                  backgroundColor: Colors.white,
                  child: Icon(Icons.auto_awesome_rounded, color: Color(0xFF7B2FF7)),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Harp Encore: your year as a story. Tap to go on, tap the left edge to go back.
class EncoreScreen extends StatefulWidget {
  const EncoreScreen({super.key, required this.player, required this.year});
  final PlayerController player;
  final int year;

  @override
  State<EncoreScreen> createState() => _EncoreScreenState();
}

class _EncoreScreenState extends State<EncoreScreen> {
  api.StatsView? _s;
  int _page = 0;
  Timer? _timer;
  bool _saved = false;
  static const _pageTime = Duration(seconds: 7);

  static const _palettes = [
    [Color(0xFF7B2FF7), Color(0xFF1FA2FF)],
    [Color(0xFFFF5F6D), Color(0xFFFFC371)],
    [Color(0xFF11998E), Color(0xFF38EF7D)],
    [Color(0xFFDA22FF), Color(0xFF9733EE)],
    [Color(0xFFF7971E), Color(0xFFFF5F6D)],
    [Color(0xFF0F2027), Color(0xFF2C5364)],
    [Color(0xFFEE0979), Color(0xFFFF6A00)],
    [Color(0xFF3A1C71), Color(0xFFD76D77)],
    [Color(0xFF1D976C), Color(0xFF2B5876)],
  ];

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    final s = await api.listeningStats(from: _secs(DateTime(widget.year)), to: _secs(DateTime(widget.year + 1)), top: 50);
    if (!mounted) return;
    setState(() => _s = s);
    _arm();
  }

  void _arm() {
    _timer?.cancel();
    if (_page < _pages().length - 1) _timer = Timer(_pageTime, () => _go(1));
  }

  void _go(int d) {
    final n = (_page + d).clamp(0, _pages().length - 1);
    if (n == _page) return;
    setState(() => _page = n);
    _arm();
  }

  Future<void> _save() async {
    final s = _s!;
    final id = await api.playlistCreate(name: 'Harp Encore ${widget.year}');
    await api.playlistAdd(id: id, songs: [for (final t in s.topSongs) t.song]);
    widget.player.libraryRev.value++;
    if (!mounted) return;
    setState(() => _saved = true);
    toast(context, 'Saved “Harp Encore ${widget.year}” to your playlists');
  }

  List<Widget Function()> _pages() {
    final s = _s;
    if (s == null) return [];
    if (s.plays == 0) {
      return [() => _big('Your ${widget.year}', 'Not much played on this device this year. Listen a little and Encore will have a story to tell.')];
    }
    final topA = s.topArtists.firstOrNull;
    final topS = s.topSongs.firstOrNull;
    final k = listenerKind(s);
    return [
      () => _big('Your ${widget.year}\nin music', 'Here’s how it sounded. Tap to go on.'),
      () => _big('${minutesText(s.minutes)}\nminutes', 'That’s ${(s.minutes / 60).toStringAsFixed(0)} hours of music across ${s.songs} songs and ${s.artists} artists.'),
      if (s.first != null) () => _art('It started with', s.first!.thumbnail, s.first!.id, s.first!.title, s.first!.artist),
      if (topA != null) () => _art('Your artist of the year', topA.thumbnail, topA.name, topA.name, '${minutesText(topA.minutes)} minutes together', round: true),
      if (s.topArtists.length > 1) () => _list('Your top artists', [for (final a in s.topArtists.take(5)) (a.name, '${minutesText(a.minutes)} min', a.thumbnail)]),
      if (topS != null) () => _art('Your song of the year', topS.song.thumbnail, topS.song.id, topS.song.title, '${topS.song.artist} · played ${topS.plays} times'),
      if (s.topSongs.length > 1) () => _list('Your top songs', [for (final t in s.topSongs.take(5)) (t.song.title, t.song.artist, t.song.thumbnail)]),
      () => _big(k.name, '${k.line}${s.peakHour == null ? '' : '\nYour hour: ${hourText(s.peakHour!)}.'}${s.streak > 1 ? '\nLongest streak: ${s.streak} days in a row.' : ''}', icon: k.icon),
      () => _outro(s),
    ];
  }

  Widget _big(String title, String line, {IconData? icon}) => Column(
    mainAxisAlignment: MainAxisAlignment.center,
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      if (icon != null)
        Padding(
          padding: const EdgeInsets.only(bottom: 16),
          child: Icon(icon, color: Colors.white, size: 56),
        ),
      Text(
        title,
        style: const TextStyle(color: Colors.white, fontSize: 46, fontWeight: FontWeight.w900, height: 1.05),
      ),
      const SizedBox(height: 18),
      Text(line, style: const TextStyle(color: Colors.white70, fontSize: 18, height: 1.4)),
    ],
  );

  Widget _art(String label, String? url, String seed, String title, String sub, {bool round = false}) => Column(
    mainAxisAlignment: MainAxisAlignment.center,
    children: [
      Text(
        label,
        style: const TextStyle(color: Colors.white70, fontSize: 18, fontWeight: FontWeight.w600),
      ),
      const SizedBox(height: 24),
      LayoutBuilder(
        builder: (_, c) => Cover(url: url, seed: seed, size: (c.maxWidth * 0.72).clamp(120, 320), radius: 18, round: round),
      ),
      const SizedBox(height: 24),
      Text(
        title,
        textAlign: TextAlign.center,
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(color: Colors.white, fontSize: 30, fontWeight: FontWeight.w900),
      ),
      const SizedBox(height: 6),
      Text(
        sub,
        textAlign: TextAlign.center,
        style: const TextStyle(color: Colors.white70, fontSize: 16),
      ),
    ],
  );

  Widget _list(String title, List<(String, String, String?)> rows) => Column(
    mainAxisAlignment: MainAxisAlignment.center,
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(
        title,
        style: const TextStyle(color: Colors.white, fontSize: 34, fontWeight: FontWeight.w900),
      ),
      const SizedBox(height: 20),
      for (final (i, r) in rows.indexed)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 7),
          child: Row(
            children: [
              SizedBox(
                width: 34,
                child: Text(
                  '${i + 1}',
                  style: const TextStyle(color: Colors.white, fontSize: 24, fontWeight: FontWeight.w900),
                ),
              ),
              Cover(url: r.$3, seed: r.$1, size: 54, radius: 8),
              const SizedBox(width: 14),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      r.$1,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(color: Colors.white, fontSize: 17, fontWeight: FontWeight.w700),
                    ),
                    Text(
                      r.$2,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(color: Colors.white70, fontSize: 14),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
    ],
  );

  Widget _outro(api.StatsView s) => Column(
    mainAxisAlignment: MainAxisAlignment.center,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      const Text(
        'That was your year.',
        style: TextStyle(color: Colors.white, fontSize: 40, fontWeight: FontWeight.w900, height: 1.05),
      ),
      const SizedBox(height: 14),
      Text('Keep your top ${s.topSongs.length} songs as a playlist, or play them now.', style: const TextStyle(color: Colors.white70, fontSize: 17)),
      const SizedBox(height: 28),
      FilledButton.icon(
        style: FilledButton.styleFrom(backgroundColor: Colors.white, foregroundColor: Colors.black, padding: const EdgeInsets.symmetric(vertical: 14)),
        onPressed: _saved ? null : _save,
        icon: Icon(_saved ? Icons.check_rounded : Icons.playlist_add_rounded),
        label: Text(_saved ? 'Saved to your playlists' : 'Save as a playlist'),
      ),
      const SizedBox(height: 10),
      OutlinedButton.icon(
        style: OutlinedButton.styleFrom(
          foregroundColor: Colors.white,
          side: const BorderSide(color: Colors.white54),
          padding: const EdgeInsets.symmetric(vertical: 14),
        ),
        onPressed: () {
          widget.player.playList([for (final t in s.topSongs) t.song], from: (kind: 'encore', id: '${widget.year}', title: 'Harp Encore ${widget.year}'));
          Navigator.of(context).pop();
        },
        icon: const Icon(Icons.play_arrow_rounded),
        label: const Text('Play my top songs'),
      ),
    ],
  );

  @override
  Widget build(BuildContext context) {
    final pages = _pages();
    final colors = _palettes[_page % _palettes.length];
    return Scaffold(
      backgroundColor: Colors.black,
      body: AnimatedContainer(
        duration: const Duration(milliseconds: 600),
        decoration: BoxDecoration(
          gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: colors),
        ),
        child: SafeArea(
          child: pages.isEmpty
              ? const Center(child: CircularProgressIndicator(color: Colors.white))
              : Column(
                  children: [
                    Padding(
                      padding: const EdgeInsets.fromLTRB(12, 10, 12, 0),
                      child: Row(
                        children: [
                          for (var i = 0; i < pages.length; i++)
                            Expanded(
                              child: Container(
                                height: 3,
                                margin: const EdgeInsets.symmetric(horizontal: 2),
                                decoration: BoxDecoration(color: i <= _page ? Colors.white : Colors.white30, borderRadius: BorderRadius.circular(2)),
                              ),
                            ),
                        ],
                      ),
                    ),
                    Row(
                      children: [
                        Padding(
                          padding: const EdgeInsets.only(left: 16),
                          child: Text(
                            'HARP ENCORE ${widget.year}',
                            style: const TextStyle(color: Colors.white70, fontWeight: FontWeight.w800, letterSpacing: 2, fontSize: 12),
                          ),
                        ),
                        const Spacer(),
                        IconButton(
                          onPressed: () => Navigator.of(context).pop(),
                          icon: const Icon(Icons.close_rounded, color: Colors.white),
                        ),
                      ],
                    ),
                    Expanded(
                      child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        // The last page has buttons; don't steal their taps.
                        onTapUp: _page == pages.length - 1 ? null : (d) => _go(d.localPosition.dx < MediaQuery.sizeOf(context).width * 0.3 ? -1 : 1),
                        onHorizontalDragEnd: (d) => _go((d.primaryVelocity ?? 0) < 0 ? 1 : -1),
                        child: Padding(
                          padding: const EdgeInsets.fromLTRB(28, 8, 28, 28),
                          child: AnimatedSwitcher(
                            duration: const Duration(milliseconds: 450),
                            transitionBuilder: (c, a) => FadeTransition(
                              opacity: a,
                              child: SlideTransition(
                                position: Tween(begin: const Offset(0, 0.06), end: Offset.zero).animate(a),
                                child: c,
                              ),
                            ),
                            child: KeyedSubtree(key: ValueKey(_page), child: pages[_page]()),
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
        ),
      ),
    );
  }
}
