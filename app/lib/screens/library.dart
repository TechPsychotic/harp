import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
import 'home.dart' show moodColor;
import 'import_screen.dart';
import 'playlist.dart';

class LibraryScreen extends StatefulWidget {
  const LibraryScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<LibraryScreen> createState() => _LibraryScreenState();
}

class _LibraryScreenState extends State<LibraryScreen> with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 4, vsync: this);
  List<api.SongView> _liked = [], _history = [];
  List<api.PlaylistView> _playlists = [];
  List<api.DownloadView> _downloads = [];

  PlayerController get p => widget.player;

  @override
  void initState() {
    super.initState();
    _load();
    p.libraryRev.addListener(_load);
  }

  Future<void> _load() async {
    final l = await api.likedSongs();
    final h = await api.history(limit: 100);
    final pl = await api.playlists();
    final d = await api.downloads();
    if (mounted) setState(() { _liked = l; _history = h; _playlists = pl; _downloads = d; });
  }

  @override
  void dispose() {
    p.libraryRev.removeListener(_load);
    _tabs.dispose();
    super.dispose();
  }

  String _mb(int bytes) => '${(bytes / 1048576).toStringAsFixed(1)} MB';

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    Widget songs(List<api.SongView> list, String empty) => list.isEmpty
        ? Center(child: Text(empty, style: TextStyle(color: tok.textFaint)))
        : ListView.builder(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
            itemCount: list.length,
            itemBuilder: (_, i) => SongTile(song: list[i], player: p),
          );
    final totalBytes = _downloads.fold<int>(0, (a, d) => a + d.bytes.toInt());
    return Column(children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 20, 16, 4),
        child: Row(children: [
          Expanded(child: Text('Library', style: Theme.of(context).textTheme.headlineSmall?.copyWith(fontWeight: FontWeight.w700))),
          TextButton.icon(
            icon: const Icon(Icons.drive_folder_upload_rounded, size: 18),
            label: const Text('Import'),
            onPressed: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => ImportScreen(player: p))),
          ),
        ]),
      ),
      TabBar(
        controller: _tabs,
        isScrollable: true,
        tabAlignment: TabAlignment.start,
        dividerColor: Colors.transparent,
        indicatorColor: Theme.of(context).colorScheme.primary,
        tabs: const [Tab(text: 'Liked'), Tab(text: 'Playlists'), Tab(text: 'Downloads'), Tab(text: 'History')],
      ),
      Expanded(
        child: TabBarView(controller: _tabs, children: [
          songs(_liked, 'Heart a song to keep it here'),
          // playlists
          ListView(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
            children: [
              GlassCard(
                padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
                onTap: () async {
                  final n = await promptText(context, title: 'New playlist', hint: 'Name');
                  if (n != null) {
                    await api.playlistCreate(name: n);
                    p.libraryRev.value++;
                  }
                },
                child: const Row(children: [Icon(Icons.add_rounded), SizedBox(width: 12), Text('New playlist', style: TextStyle(fontWeight: FontWeight.w600))]),
              ),
              for (final pl in _playlists)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: GlassCard(
                    padding: const EdgeInsets.all(10),
                    onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => PlaylistScreen(player: p, playlist: pl))),
                    child: Row(children: [
                      Container(
                        width: 44,
                        height: 44,
                        decoration: BoxDecoration(
                          borderRadius: BorderRadius.circular(Radii.cover),
                          gradient: LinearGradient(colors: [moodColor('pl${pl.id}').withValues(alpha: 0.85), moodColor('pl${pl.id}').withValues(alpha: 0.3)]),
                        ),
                        child: const Icon(Icons.queue_music_rounded, color: Colors.white),
                      ),
                      const SizedBox(width: 12),
                      Expanded(child: Text(pl.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600))),
                      Text('${pl.count}', style: TextStyle(color: tok.textDim)),
                      const Icon(Icons.chevron_right_rounded),
                    ]),
                  ),
                ),
            ],
          ),
          // downloads
          _downloads.isEmpty
              ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text('Use ⋮ › Download on any song to keep it for offline listening', textAlign: TextAlign.center, style: TextStyle(color: tok.textFaint))))
              : ListView(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
                  children: [
                    Row(children: [
                      Expanded(child: Text('${_downloads.length} songs · ${_mb(totalBytes)}', style: TextStyle(color: tok.textDim))),
                      TextButton.icon(icon: const Icon(Icons.shuffle_rounded, size: 18), label: const Text('Play all'), onPressed: () => p.playDownloads()),
                    ]),
                    for (final d in _downloads) SongTile(song: d.song, player: p, subtitle: '${d.quality} · ${_mb(d.bytes.toInt())}'),
                  ],
                ),
          songs(_history, 'Nothing played yet'),
        ]),
      ),
    ]);
  }
}
