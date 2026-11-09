import 'dart:io';

import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/shelf.dart';
import 'collection.dart';

/// The country for charts and local suggestions, from the device's language
/// setting ("en_IN" → "IN"). US when the device doesn't say.
String deviceCountry() {
  final parts = Platform.localeName.split(RegExp(r'[_\-.]'));
  final c = parts.length > 1 ? parts[1] : '';
  return RegExp(r'^[A-Za-z]{2}$').hasMatch(c) ? c.toUpperCase() : 'US';
}

/// Search's starting page below your recent searches: Charts, then every mood
/// and genre as a coloured tile (like Spotify's "Browse all").
class BrowseGrid extends StatefulWidget {
  const BrowseGrid({super.key, required this.player});
  final PlayerController player;

  @override
  State<BrowseGrid> createState() => _BrowseGridState();
}

class _BrowseGridState extends State<BrowseGrid> {
  static List<api.BrowseGroupView>? _cache; // the page barely changes; once per run is plenty
  List<api.BrowseGroupView>? _groups = _cache;
  bool _failed = false;

  @override
  void initState() {
    super.initState();
    if (_groups == null && !widget.player.settings.offline) _load();
  }

  Future<void> _load() async {
    try {
      final g = await api.browseExplore(country: deviceCountry());
      _cache = g;
      if (mounted) setState(() => _groups = g);
    } catch (_) {
      if (mounted) setState(() => _failed = true);
    }
  }

  void _open(String title, {String? params}) => Navigator.of(context).push(
        MaterialPageRoute(builder: (_) => BrowseScreen(player: widget.player, title: title, params: params)),
      );

  @override
  Widget build(BuildContext context) {
    if (widget.player.settings.offline) return const SizedBox.shrink();
    final groups = _groups;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      const SectionHeader('Browse'),
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16),
        child: _Tile(
          title: 'Charts',
          subtitle: 'Top songs, videos and artists in ${deviceCountry()}',
          color: const Color(0xFFE0313F),
          icon: Icons.trending_up_rounded,
          tall: true,
          onTap: () => _open('Charts'),
        ),
      ),
      if (groups == null && !_failed)
        const Padding(padding: EdgeInsets.all(24), child: Center(child: CircularProgressIndicator()))
      else if (_failed)
        Padding(padding: const EdgeInsets.all(16), child: TextButton(onPressed: () { setState(() => _failed = false); _load(); }, child: const Text('Couldn’t load moods and genres. Try again')))
      else
        for (final g in groups!) ...[
          SectionHeader(g.title),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: LayoutBuilder(builder: (context, c) {
              final per = (c.maxWidth / 180).floor().clamp(2, 6);
              final w = (c.maxWidth - (per - 1) * 10) / per;
              return Wrap(spacing: 10, runSpacing: 10, children: [
                for (final t in g.tiles)
                  SizedBox(width: w, child: _Tile(title: t.title, color: Color(t.color), onTap: () => _open(t.title, params: t.params))),
              ]);
            }),
          ),
        ],
    ]);
  }
}

class _Tile extends StatelessWidget {
  const _Tile({required this.title, required this.color, required this.onTap, this.subtitle, this.icon, this.tall = false});
  final String title;
  final String? subtitle;
  final Color color;
  final IconData? icon;
  final bool tall;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final hsl = HSLColor.fromColor(color.withAlpha(255));
    final a = hsl.withLightness(hsl.lightness.clamp(0.38, 0.55)).withSaturation(hsl.saturation.clamp(0.45, 0.9)).toColor();
    final b = hsl.withHue((hsl.hue + 30) % 360).withLightness(0.22).toColor();
    return Material(
      borderRadius: BorderRadius.circular(Radii.tile),
      clipBehavior: Clip.antiAlias,
      child: Ink(
        height: tall ? 96 : 64,
        decoration: BoxDecoration(gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [a, b])),
        child: InkWell(
          onTap: onTap,
          child: Stack(children: [
            if (icon != null) Positioned(right: -6, bottom: -14, child: Transform.rotate(angle: 0.3, child: Icon(icon, size: 90, color: Colors.white24))),
            Padding(
              padding: const EdgeInsets.all(12),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.end, children: [
                Text(title, maxLines: 2, overflow: TextOverflow.ellipsis, style: TextStyle(fontWeight: FontWeight.w800, fontSize: tall ? 22 : 15, color: Colors.white)),
                if (subtitle != null) Text(subtitle!, style: const TextStyle(color: Colors.white70, fontSize: 13)),
              ]),
            ),
          ]),
        ),
      ),
    );
  }
}

/// A mood, genre or the charts: rows of playlists (and artists, for charts).
class BrowseScreen extends StatefulWidget {
  const BrowseScreen({super.key, required this.player, required this.title, this.params});
  final PlayerController player;
  final String title;

  /// A mood or genre; null for the charts.
  final String? params;

  @override
  State<BrowseScreen> createState() => _BrowseScreenState();
}

class _BrowseScreenState extends State<BrowseScreen> {
  List<api.SectionView>? _sections;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final c = deviceCountry();
      final v = widget.params == null ? await api.browseCharts(country: c) : await api.browseCategory(params: widget.params!, country: c);
      ThumbCache.precache([for (final s in v) ...s.items.take(8).map((i) => i.thumbnail)]);
      if (mounted) setState(() => _sections = v);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final s = _sections;
    return Scaffold(
      appBar: AppBar(title: Text(widget.title)),
      body: _error != null
          ? EmptyState(icon: Icons.cloud_off_rounded, text: 'Couldn’t load this.\n$_error')
          : s == null
              ? const Center(child: CircularProgressIndicator())
              : s.isEmpty
                  ? const EmptyState(icon: Icons.music_off_rounded, text: 'Nothing here right now.')
                  : ListView(padding: const EdgeInsets.only(bottom: 24), children: [
                      for (final sec in s)
                        CollectionShelf(
                          title: sec.title,
                          items: sec.items,
                          size: sec.items.every((i) => i.kind == 'artist') ? 120 : 150,
                          onOpen: (c) => Navigator.of(context).push(MaterialPageRoute(builder: (_) => CollectionScreen(player: widget.player, info: c))),
                        ),
                    ]),
    );
  }
}
