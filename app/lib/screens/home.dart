import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/shelf.dart';
import 'import_screen.dart';
import 'playlist.dart';

/// "Listen Now": what to play next, picked on this device.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  api.HomeFeed? _feed;
  List<(String, String)> _moods = [];
  api.TasteStats? _stats;
  bool _refreshing = false;
  bool _wasOffline = false;

  PlayerController get p => widget.player;

  @override
  void initState() {
    super.initState();
    _wasOffline = p.settings.offline;
    _load();
    p.libraryRev.addListener(_reloadLocal);
    p.settings.addListener(_onSettings);
    p.addListener(_onPlayer);
  }

  @override
  void dispose() {
    p.libraryRev.removeListener(_reloadLocal);
    p.settings.removeListener(_onSettings);
    p.removeListener(_onPlayer);
    super.dispose();
  }

  void _onSettings() {
    if (p.settings.offline != _wasOffline) {
      _wasOffline = p.settings.offline;
      _load();
    }
  }

  // The Smart Shuffle card shows a spinner while it works.
  bool _lastLoading = false;
  void _onPlayer() {
    if (p.loading != _lastLoading && mounted) setState(() => _lastLoading = p.loading);
  }

  void _apply(api.HomeFeed f) {
    if (!mounted) return;
    setState(() => _feed = f);
    ThumbCache.precache([
      for (final list in [f.recommended, f.similar, f.recent, f.liked, f.downloads]) ...list.map((s) => s.thumbnail),
    ]);
  }

  /// Instant: what is saved. Then, if online, fresh picks.
  Future<void> _load() async {
    final moods = await api.moods();
    final stats = await api.tasteStats();
    if (mounted) setState(() { _moods = moods; _stats = stats; });
    _apply(await api.homeFeed(refresh: false, offline: true));
    if (!p.settings.offline) {
      if (mounted) setState(() => _refreshing = true);
      try {
        _apply(await api.homeFeed(refresh: false, offline: false));
      } catch (_) {} finally {
        if (mounted) setState(() => _refreshing = false);
      }
    }
  }

  Future<void> _reloadLocal() async {
    final f = await api.homeFeed(refresh: false, offline: true);
    if (mounted) setState(() => _feed = f);
  }

  Future<void> _forceRefresh() async {
    if (p.settings.offline) return;
    setState(() => _refreshing = true);
    try {
      _apply(await api.homeFeed(refresh: true, offline: false));
    } catch (_) {} finally {
      if (mounted) setState(() => _refreshing = false);
    }
  }

  String _greeting() {
    final h = DateTime.now().hour;
    return h < 5 ? 'Late night' : h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening';
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final f = _feed;
    final offline = p.settings.offline;
    final picks = f?.recommended ?? const <api.SongView>[];
    return RefreshIndicator(
      onRefresh: _forceRefresh,
      child: ListView(
        padding: const EdgeInsets.only(bottom: 24),
        children: [
          PageTitle(
            'Listen Now',
            subtitle: _greeting(),
            actions: [
              if (_refreshing) const Padding(padding: EdgeInsets.only(right: 8), child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2))),
              IconButton(tooltip: 'New picks', icon: const Icon(Icons.refresh_rounded), onPressed: offline ? null : _forceRefresh),
            ],
          ),
          if (offline)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 4),
              child: GlassCard(
                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                child: Row(children: [
                  Icon(Icons.cloud_off_rounded, size: 18, color: tok.textDim),
                  const SizedBox(width: 10),
                  Expanded(child: Text('Offline: only downloaded songs play.', style: TextStyle(color: tok.textDim, fontSize: 13.5))),
                  TextButton(onPressed: () => p.settings.setOffline(false), child: const Text('Go online')),
                ]),
              ),
            ),
          Padding(padding: const EdgeInsets.fromLTRB(16, 10, 16, 0), child: _heroRow(context)),
          if (p.error != null) Padding(padding: const EdgeInsets.fromLTRB(16, 10, 16, 0), child: Text(p.error!, style: const TextStyle(color: Color(0xFFFF6B6B)))),
          if (f == null)
            const Padding(padding: EdgeInsets.all(40), child: Center(child: CircularProgressIndicator()))
          else ...[
            if (picks.isEmpty && !offline)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 20, 16, 0),
                child: Text(_refreshing ? 'Finding songs you might like…' : 'Play a few songs and your picks will show up here.', style: TextStyle(color: tok.textDim)),
              ),
            SongShelf(title: 'Top Picks', subtitle: 'Chosen from your taste, on this device', songs: picks.take(6).toList(), player: p, big: true),
            if (_moods.isNotEmpty && !offline) ...[
              const SectionHeader('Moods'),
              SizedBox(
                height: 88,
                child: ListView.builder(
                  scrollDirection: Axis.horizontal,
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  itemCount: _moods.length,
                  itemBuilder: (_, i) => _MoodTile(keyName: _moods[i].$1, label: _moods[i].$2, onTap: () => p.smartShuffle(genre: _moods[i].$1)),
                ),
              ),
            ],
            SongShelf(title: 'More for You', songs: picks.skip(6).toList(), player: p),
            if (f.similarTo != null) SongShelf(title: 'Because You Played', subtitle: f.similarTo!.title, songs: f.similar, player: p),
            SongShelf(title: 'Recently Played', songs: f.recent, player: p),
            if (f.playlists.isNotEmpty) ...[
              const SectionHeader('Your Playlists'),
              SizedBox(
                height: 202,
                child: ListView.builder(
                  scrollDirection: Axis.horizontal,
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  itemCount: f.playlists.length,
                  itemBuilder: (_, i) => PlaylistCard(
                    pl: f.playlists[i],
                    onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => PlaylistScreen(player: p, playlist: f.playlists[i]))),
                  ),
                ),
              ),
            ],
            SongShelf(title: 'Downloaded', subtitle: 'Plays without a connection', songs: f.downloads, player: p),
            SongShelf(title: 'Liked Songs', songs: f.liked, player: p),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 22, 16, 0),
              child: GlassCard(
                onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => ImportScreen(player: p))),
                child: Row(children: [
                  Icon(Icons.swap_horiz_rounded, color: Theme.of(context).colorScheme.primary),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      const Text('Bring your music over', style: TextStyle(fontWeight: FontWeight.w600)),
                      Text('From Spotify, Apple Music, YouTube or a file', style: TextStyle(color: tok.textDim, fontSize: 13)),
                    ]),
                  ),
                  Icon(Icons.chevron_right_rounded, color: tok.textFaint),
                ]),
              ),
            ),
          ],
        ],
      ),
    );
  }

  Widget _heroRow(BuildContext context) {
    final accent = Theme.of(context).colorScheme.primary;
    final hsl = HSLColor.fromColor(accent);
    final hero = Material(
      borderRadius: BorderRadius.circular(Radii.card),
      clipBehavior: Clip.antiAlias,
      child: Ink(
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [accent, hsl.withHue((hsl.hue + 40) % 360).withLightness((hsl.lightness * 0.6).clamp(0.15, 0.5)).toColor()],
          ),
        ),
        child: InkWell(
          onTap: p.loading ? null : () => p.smartShuffle(),
          child: Padding(
            padding: const EdgeInsets.all(18),
            child: Row(children: [
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  const Text('Smart Shuffle', style: TextStyle(fontSize: 22, fontWeight: FontWeight.w800, color: Colors.white)),
                  const SizedBox(height: 4),
                  Text(
                    (_stats?.plays ?? 0) == 0 ? 'Starts broad, then learns what you like' : 'Learned from ${_stats!.plays} plays · ${_stats!.artists} artists',
                    style: const TextStyle(color: Colors.white70, fontSize: 13),
                  ),
                ]),
              ),
              p.loading
                  ? const SizedBox(width: 44, height: 44, child: Padding(padding: EdgeInsets.all(10), child: CircularProgressIndicator(strokeWidth: 2.5, color: Colors.white)))
                  : const CircleAvatar(radius: 24, backgroundColor: Colors.white, child: Icon(Icons.play_arrow_rounded, size: 30, color: Colors.black)),
            ]),
          ),
        ),
      ),
    );
    final resume = p.resumeOffer;
    if (resume == null) return hero;
    return Column(children: [
      hero,
      const SizedBox(height: 10),
      GlassCard(
        padding: const EdgeInsets.all(8),
        onTap: p.resume,
        child: Row(children: [
          Cover(url: resume.thumbnail, seed: resume.id, size: 44),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('Pick up where you left off', style: TextStyle(color: context.tok.textDim, fontSize: 12.5)),
              Text(resume.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600)),
            ]),
          ),
          Icon(Icons.play_circle_fill_rounded, color: accent, size: 32),
        ]),
      ),
    ]);
  }
}

Color moodColor(String key) {
  var h = 0;
  for (final c in key.codeUnits) {
    h = (h * 37 + c) & 0x7fffffff;
  }
  return HSLColor.fromAHSL(1, (h % 360).toDouble(), 0.62, 0.48).toColor();
}

class _MoodTile extends StatelessWidget {
  const _MoodTile({required this.keyName, required this.label, required this.onTap});
  final String keyName;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = moodColor(keyName);
    return Padding(
      padding: const EdgeInsets.only(right: 10),
      child: Material(
        borderRadius: BorderRadius.circular(Radii.tile),
        clipBehavior: Clip.antiAlias,
        child: Ink(
          width: 150,
          decoration: BoxDecoration(gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [c, HSLColor.fromColor(c).withLightness(0.28).toColor()])),
          child: InkWell(
            onTap: onTap,
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Align(alignment: Alignment.bottomLeft, child: Text(label, style: const TextStyle(fontWeight: FontWeight.w800, fontSize: 16, color: Colors.white))),
            ),
          ),
        ),
      ),
    );
  }
}

/// A playlist card: a colour tile with a music icon, name and count.
class PlaylistCard extends StatelessWidget {
  const PlaylistCard({super.key, required this.pl, required this.onTap});
  final api.PlaylistView pl;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = moodColor('pl${pl.id}');
    return GestureDetector(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.only(right: 14),
        child: SizedBox(
          width: 150,
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Container(
              width: 150,
              height: 150,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(Radii.tile),
                gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [c, HSLColor.fromColor(c).withLightness(0.25).toColor()]),
              ),
              child: const Icon(Icons.music_note_rounded, size: 56, color: Colors.white70),
            ),
            const SizedBox(height: 7),
            Text(pl.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w500, fontSize: 14)),
            Text('${pl.count} songs', style: TextStyle(color: context.tok.textDim, fontSize: 13)),
          ]),
        ),
      ),
    );
  }
}
