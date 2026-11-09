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

class _LibraryScreenState extends State<LibraryScreen> {
  int _tab = 0;
  List<api.SongView> _liked = [], _history = [];
  List<api.PlaylistView> _playlists = [];
  List<api.DownloadView> _downloads = [];

  PlayerController get p => widget.player;

  static const _tabs = ['Liked', 'Playlists', 'Downloads', 'History'];

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
    super.dispose();
  }

  String _mb(int bytes) => '${(bytes / 1048576).toStringAsFixed(1)} MB';

  void _openImport() => Navigator.of(context).push(MaterialPageRoute(builder: (_) => ImportScreen(player: p)));

  Widget _songs(List<api.SongView> list, IconData icon, String empty) => list.isEmpty
      ? EmptyState(icon: icon, text: empty)
      : ListView.builder(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
          itemCount: list.length,
          itemBuilder: (_, i) => SongTile(song: list[i], player: p),
        );

  Widget _playlistsTab(HarpTokens tok) => ListView(
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
          const SizedBox(height: 6),
          GlassCard(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
            onTap: _openImport,
            child: const Row(children: [
              Icon(Icons.drive_folder_upload_rounded),
              SizedBox(width: 12),
              Expanded(child: Text('Import a playlist (Spotify, YouTube, Apple Music, Amazon…)', style: TextStyle(fontWeight: FontWeight.w600))),
            ]),
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
      );

  Widget _downloadsTab(HarpTokens tok) {
    if (_downloads.isEmpty) {
      return const EmptyState(icon: Icons.download_for_offline_rounded, text: 'Use ⋮ › Download on any song to keep it for offline listening');
    }
    final totalBytes = _downloads.fold<int>(0, (a, d) => a + d.bytes.toInt());
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
      children: [
        Row(children: [
          Expanded(child: Text('${_downloads.length} songs · ${_mb(totalBytes)}', style: TextStyle(color: tok.textDim))),
          TextButton.icon(icon: const Icon(Icons.shuffle_rounded, size: 18), label: const Text('Play all'), onPressed: () => p.playDownloads()),
        ]),
        for (final d in _downloads) SongTile(song: d.song, player: p, subtitle: '${d.quality} · ${_mb(d.bytes.toInt())}'),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      PageTitle('Library', actions: [IconButton(tooltip: 'Import a playlist', icon: const Icon(Icons.drive_folder_upload_rounded), onPressed: _openImport)]),
      SizedBox(
        height: 40,
        child: ListView(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 16),
          children: [
            for (var i = 0; i < _tabs.length; i++)
              Padding(padding: const EdgeInsets.only(right: 8), child: GlassChip(label: _tabs[i], selected: _tab == i, onTap: () => setState(() => _tab = i))),
          ],
        ),
      ),
      const SizedBox(height: 4),
      Expanded(
        child: switch (_tab) {
          0 => _songs(_liked, Icons.favorite_border_rounded, 'Heart a song to keep it here'),
          1 => _playlistsTab(tok),
          2 => _downloadsTab(tok),
          _ => _songs(_history, Icons.history_rounded, 'Nothing played yet'),
        },
      ),
    ]);
  }
}
