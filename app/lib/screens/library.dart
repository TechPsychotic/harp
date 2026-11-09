import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
import 'home.dart' show PlaylistCard;
import 'import_screen.dart';
import 'playlist.dart';

/// Your music on this device: categories first, then your playlists.
class LibraryScreen extends StatefulWidget {
  const LibraryScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<LibraryScreen> createState() => _LibraryScreenState();
}

class _LibraryScreenState extends State<LibraryScreen> {
  List<api.PlaylistView> _playlists = [];
  int _liked = 0, _downloads = 0;

  PlayerController get p => widget.player;

  @override
  void initState() {
    super.initState();
    _load();
    p.libraryRev.addListener(_load);
  }

  @override
  void dispose() {
    p.libraryRev.removeListener(_load);
    super.dispose();
  }

  Future<void> _load() async {
    final pl = await api.playlists();
    final l = await api.likedSongs();
    final d = await api.downloads();
    if (mounted) setState(() { _playlists = pl; _liked = l.length; _downloads = d.length; });
  }

  void _push(Widget w) => Navigator.of(context).push(MaterialPageRoute(builder: (_) => w));

  Future<void> _newPlaylist() async {
    final n = await promptText(context, title: 'New Playlist', hint: 'Name');
    if (n != null) {
      await api.playlistCreate(name: n);
      p.libraryRev.value++;
    }
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    Widget row(IconData icon, String label, String? count, VoidCallback onTap) => Column(children: [
          ListTile(
            leading: Icon(icon, color: accent),
            title: Text(label, style: const TextStyle(fontSize: 18)),
            trailing: Row(mainAxisSize: MainAxisSize.min, children: [
              if (count != null) Text(count, style: TextStyle(color: tok.textDim)),
              Icon(Icons.chevron_right_rounded, color: tok.textFaint),
            ]),
            onTap: onTap,
          ),
          const Divider(indent: 56),
        ]);
    return ListView(padding: const EdgeInsets.only(bottom: 24), children: [
      PageTitle('Library', actions: [IconButton(tooltip: 'New playlist', icon: Icon(Icons.add_rounded, color: accent), onPressed: _newPlaylist)]),
      row(Icons.favorite_rounded, 'Liked Songs', '$_liked', () => _push(SongListScreen(player: p, kind: 'liked'))),
      row(Icons.arrow_circle_down_rounded, 'Downloaded', '$_downloads', () => _push(SongListScreen(player: p, kind: 'downloads'))),
      row(Icons.history_rounded, 'Recently Played', null, () => _push(SongListScreen(player: p, kind: 'history'))),
      row(Icons.swap_horiz_rounded, 'Bring Your Music Over', null, () => _push(ImportScreen(player: p))),
      SectionHeader('Playlists', action: 'New', onAction: _newPlaylist),
      if (_playlists.isEmpty)
        Padding(padding: const EdgeInsets.symmetric(horizontal: 16), child: Text('Make a playlist, or bring one over from another app.', style: TextStyle(color: tok.textDim)))
      else
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: LayoutBuilder(builder: (context, c) {
            final per = (c.maxWidth / 164).floor().clamp(2, 6);
            final w = (c.maxWidth - (per - 1) * 14) / per;
            return Wrap(spacing: 0, runSpacing: 14, children: [
              for (final pl in _playlists)
                SizedBox(
                  width: w + 14,
                  child: PlaylistCard(pl: pl, onTap: () => _push(PlaylistScreen(player: p, playlist: pl))),
                ),
            ]);
          }),
        ),
    ]);
  }
}

/// Liked songs, downloads or history as one list, with play / shuffle.
class SongListScreen extends StatefulWidget {
  const SongListScreen({super.key, required this.player, required this.kind});
  final PlayerController player;

  /// "liked", "downloads" or "history"
  final String kind;

  @override
  State<SongListScreen> createState() => _SongListScreenState();
}

class _SongListScreenState extends State<SongListScreen> {
  List<api.SongView>? _songs;
  Map<String, String> _notes = {};

  PlayerController get p => widget.player;

  @override
  void initState() {
    super.initState();
    _load();
    p.libraryRev.addListener(_load);
  }

  @override
  void dispose() {
    p.libraryRev.removeListener(_load);
    super.dispose();
  }

  Future<void> _load() async {
    List<api.SongView> s;
    var notes = <String, String>{};
    switch (widget.kind) {
      case 'liked':
        s = await api.likedSongs();
      case 'downloads':
        final d = await api.downloads();
        s = d.map((x) => x.song).toList();
        notes = {for (final x in d) x.song.id: '${x.quality} · ${(x.bytes.toInt() / 1048576).toStringAsFixed(1)} MB'};
      default:
        s = await api.history(limit: 200);
    }
    if (mounted) setState(() { _songs = s; _notes = notes; });
  }

  @override
  Widget build(BuildContext context) {
    final title = switch (widget.kind) { 'liked' => 'Liked Songs', 'downloads' => 'Downloaded', _ => 'Recently Played' };
    final songs = _songs;
    return Scaffold(
      appBar: AppBar(
        title: Text(title),
        actions: [
          if (widget.kind == 'liked')
            IconButton(
              tooltip: 'Export',
              icon: const Icon(Icons.ios_share_rounded),
              onPressed: () async {
                final f = await pickExportFormat(context);
                if (f == null || !context.mounted) return;
                final file = await api.likedExport(format: f);
                if (!context.mounted) return;
                await saveExport(context, file);
              },
            ),
        ],
      ),
      body: songs == null
          ? const Center(child: CircularProgressIndicator())
          : songs.isEmpty
              ? EmptyState(
                  icon: Icons.music_note_rounded,
                  text: switch (widget.kind) {
                    'liked' => 'Heart a song to keep it here',
                    'downloads' => 'Download songs (… › Download) to play them offline',
                    _ => 'Nothing played yet',
                  },
                )
              : ListView(padding: const EdgeInsets.only(bottom: 24), children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                    child: Row(children: [
                      Expanded(child: FilledButton.icon(onPressed: () => p.playList(songs), icon: const Icon(Icons.play_arrow_rounded), label: const Text('Play'))),
                      const SizedBox(width: 12),
                      Expanded(child: OutlinedButton.icon(onPressed: () => p.playList(songs, shuffle: true), icon: const Icon(Icons.shuffle_rounded), label: const Text('Shuffle'))),
                    ]),
                  ),
                  for (var i = 0; i < songs.length; i++) ...[
                    SongTile(song: songs[i], player: p, subtitle: _notes[songs[i].id], onPlay: () => p.playList(songs.sublist(i))),
                    const Divider(indent: 78),
                  ],
                ]),
    );
  }
}
