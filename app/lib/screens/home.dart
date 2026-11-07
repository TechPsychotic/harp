import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../theme/logo.dart';
import '../widgets/cover.dart';
import '../widgets/shelf.dart';
import 'playlist.dart';
import 'settings_screen.dart';

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

  PlayerController get p => widget.player;

  @override
  void initState() {
    super.initState();
    _load();
    p.libraryRev.addListener(_reloadLocal);
    p.settings.addListener(_load);
  }

  @override
  void dispose() {
    p.libraryRev.removeListener(_reloadLocal);
    p.settings.removeListener(_load);
    super.dispose();
  }

  void _apply(api.HomeFeed f) {
    if (!mounted) return;
    setState(() => _feed = f);
    ThumbCache.precache([
      for (final list in [f.recommended, f.similar, f.recent, f.liked, f.downloads]) ...list.map((s) => s.thumbnail),
    ]);
  }

  /// Instant: what is saved. Then, if online, fresh recommendations.
  Future<void> _load() async {
    final moods = await api.moods();
    final stats = await api.tasteStats();
    if (mounted) setState(() { _moods = moods; _stats = stats; });
    _apply(await api.homeFeed(refresh: false, offline: true));
    if (!p.settings.offline) {
      setState(() => _refreshing = true);
      try {
        _apply(await api.homeFeed(refresh: false, offline: false));
      } catch (_) {} finally {
        if (mounted) setState(() => _refreshing = false);
      }
    }
  }

  /// Local-only parts (likes, playlists, downloads) after a change.
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

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final f = _feed;
    final hour = DateTime.now().hour;
    final greeting = hour < 5 ? 'Late night' : hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
    final offline = p.settings.offline;
    return RefreshIndicator(
      onRefresh: _forceRefresh,
      child: ListView(
        padding: const EdgeInsets.only(top: 12, bottom: 24),
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Row(children: [
              const HarpLogo(size: 34),
              const SizedBox(width: 12),
              Expanded(child: Text(greeting, style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w700))),
              if (_refreshing) const Padding(padding: EdgeInsets.only(right: 12), child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2))),
              IconButton(tooltip: 'Refresh picks', icon: const Icon(Icons.refresh_rounded), onPressed: offline ? null : _forceRefresh),
              IconButton(
                tooltip: 'Settings',
                icon: const Icon(Icons.settings_rounded),
                onPressed: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => SettingsScreen(player: p))),
              ),
            ]),
          ),
          if (offline)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 10, 16, 0),
              child: GlassCard(
                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                child: Row(children: [
                  const Icon(Icons.cloud_off_rounded, size: 18),
                  const SizedBox(width: 10),
                  Expanded(child: Text('Offline mode: only downloaded songs play.', style: TextStyle(color: tok.textDim, fontSize: 13))),
                  TextButton(onPressed: () => p.settings.setOffline(false), child: const Text('Go online')),
                ]),
              ),
            ),
          const SizedBox(height: 14),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Row(children: [
              Expanded(
                flex: 3,
                child: GlassCard(
                  padding: const EdgeInsets.all(18),
                  tint: Theme.of(context).colorScheme.primary,
                  onTap: p.loading ? null : () => p.smartShuffle(),
                  child: Row(children: [
                    const Icon(Icons.auto_awesome_rounded, size: 30),
                    const SizedBox(width: 14),
                    Expanded(
                      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                        const Text('Smart shuffle', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
                        const SizedBox(height: 2),
                        Text(
                          (_stats?.plays ?? 0) == 0 ? 'Starts broad, then learns you' : '${_stats!.plays} plays · ${_stats!.artists} artists learned',
                          style: TextStyle(color: tok.textDim, fontSize: 12.5),
                        ),
                      ]),
                    ),
                    p.loading
                        ? const SizedBox(width: 22, height: 22, child: CircularProgressIndicator(strokeWidth: 2))
                        : const Icon(Icons.play_circle_fill_rounded, size: 36),
                  ]),
                ),
              ),
              if (p.resumeOffer != null) ...[
                const SizedBox(width: 10),
                Expanded(
                  flex: 2,
                  child: GlassCard(
                    padding: const EdgeInsets.all(14),
                    onTap: p.resume,
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      const Icon(Icons.history_rounded),
                      const SizedBox(height: 6),
                      const Text('Resume', style: TextStyle(fontWeight: FontWeight.w700)),
                      Text(p.resumeOffer!.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 12)),
                    ]),
                  ),
                ),
              ],
            ]),
          ),
          if (p.error != null)
            Padding(padding: const EdgeInsets.fromLTRB(16, 10, 16, 0), child: Text(p.error!, style: const TextStyle(color: Color(0xFFFF8A8A)))),
          const SizedBox(height: 18),
          if (f == null)
            const Padding(padding: EdgeInsets.all(40), child: Center(child: CircularProgressIndicator()))
          else ...[
            if (f.recommended.isEmpty && !offline)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: Text(_refreshing ? 'Finding songs you might like…' : 'Play a few songs and your picks will show up here.', style: TextStyle(color: tok.textDim)),
              ),
            SongShelf(title: 'Recommended for you', subtitle: 'Picked from your taste, on this device', songs: f.recommended, player: p),
            ShelfHeader(title: 'Change the mood', subtitle: offline ? 'Needs a connection' : null),
            SizedBox(
              height: 64,
              child: ListView.builder(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: 16),
                itemCount: _moods.length,
                itemBuilder: (_, i) => _MoodTile(
                  keyName: _moods[i].$1,
                  label: _moods[i].$2,
                  onTap: offline ? null : () => p.smartShuffle(genre: _moods[i].$1),
                ),
              ),
            ),
            const SizedBox(height: 16),
            if (f.similarTo != null)
              SongShelf(title: 'Similar to “${f.similarTo!.title}”', subtitle: 'More like what you played last', songs: f.similar, player: p),
            SongShelf(title: 'Recently played', songs: f.recent, player: p),
            if (f.playlists.isNotEmpty) ...[
              const ShelfHeader(title: 'Your playlists'),
              SizedBox(
                height: 120,
                child: ListView.builder(
                  scrollDirection: Axis.horizontal,
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  itemCount: f.playlists.length,
                  itemBuilder: (_, i) => _PlaylistCard(
                    pl: f.playlists[i],
                    onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => PlaylistScreen(player: p, playlist: f.playlists[i]))),
                  ),
                ),
              ),
              const SizedBox(height: 16),
            ],
            SongShelf(title: 'Downloaded', subtitle: 'Plays without a connection', songs: f.downloads, player: p),
            SongShelf(title: 'Liked songs', songs: f.liked, player: p),
          ],
        ],
      ),
    );
  }
}

Color moodColor(String key) {
  var h = 0;
  for (final c in key.codeUnits) {
    h = (h * 37 + c) & 0x7fffffff;
  }
  return HSLColor.fromAHSL(1, (h % 360).toDouble(), 0.65, 0.5).toColor();
}

class _MoodTile extends StatelessWidget {
  const _MoodTile({required this.keyName, required this.label, required this.onTap});
  final String keyName;
  final String label;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final c = moodColor(keyName);
    return Padding(
      padding: const EdgeInsets.only(right: 10),
      child: Opacity(
        opacity: onTap == null ? 0.5 : 1,
        child: Material(
          color: Colors.transparent,
          child: InkWell(
            borderRadius: BorderRadius.circular(Radii.tile),
            onTap: onTap,
            child: Ink(
              width: 132,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(Radii.tile),
                gradient: LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [c.withValues(alpha: 0.85), c.withValues(alpha: 0.35)]),
              ),
              child: Align(alignment: Alignment.bottomLeft, child: Text(label, style: const TextStyle(fontWeight: FontWeight.w700, color: Colors.white))),
            ),
          ),
        ),
      ),
    );
  }
}

class _PlaylistCard extends StatelessWidget {
  const _PlaylistCard({required this.pl, required this.onTap});
  final api.PlaylistView pl;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = moodColor('pl${pl.id}');
    return Padding(
      padding: const EdgeInsets.only(right: 12),
      child: InkWell(
        borderRadius: BorderRadius.circular(Radii.tile),
        onTap: onTap,
        child: SizedBox(
          width: 148,
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Container(
              height: 76,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(Radii.tile),
                gradient: LinearGradient(colors: [c.withValues(alpha: 0.8), c.withValues(alpha: 0.3)]),
              ),
              child: const Center(child: Icon(Icons.queue_music_rounded, size: 30, color: Colors.white)),
            ),
            const SizedBox(height: 6),
            Text(pl.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 13.5)),
            Text('${pl.count} songs', style: TextStyle(color: context.tok.textDim, fontSize: 12.5)),
          ]),
        ),
      ),
    );
  }
}
