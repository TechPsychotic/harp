import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';

/// One of your playlists: play, shuffle, reorder, export.
class PlaylistScreen extends StatefulWidget {
  const PlaylistScreen({super.key, required this.player, required this.playlist});
  final PlayerController player;
  final api.PlaylistView playlist;

  @override
  State<PlaylistScreen> createState() => _PlaylistScreenState();
}

class _PlaylistScreenState extends State<PlaylistScreen> {
  List<api.SongView> _songs = [];
  late String _name = widget.playlist.name;
  bool _loaded = false;

  PlayerController get p => widget.player;
  int get id => widget.playlist.id;

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
    final s = await api.playlistSongs(id: id);
    ThumbCache.precache(s.map((x) => x.thumbnail));
    if (mounted) setState(() { _songs = s; _loaded = true; });
  }

  Future<void> _menu(String v) async {
    switch (v) {
      case 'rename':
        final n = await promptText(context, title: 'Rename Playlist', initial: _name);
        if (n != null && n.trim().isNotEmpty) {
          await api.playlistRename(id: id, name: n);
          setState(() => _name = n.trim());
          p.libraryRev.value++;
        }
      case 'export':
        final f = await pickExportFormat(context);
        if (f == null || !mounted) return;
        final file = await api.playlistExport(id: id, format: f);
        if (!mounted) return;
        await saveExport(context, file);
      case 'download':
        var n = 0;
        for (final s in _songs.where((s) => !s.downloaded)) {
          if (await p.download(s) == null) n++;
        }
        if (mounted) toast(context, 'Downloaded $n songs');
      case 'delete':
        final ok = await showDialog<bool>(
          context: context,
          builder: (ctx) => AlertDialog(
            title: Text('Delete “$_name”?'),
            content: const Text('The songs stay in your library; only the playlist goes.'),
            actions: [
              TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
              TextButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Delete', style: TextStyle(color: Colors.redAccent))),
            ],
          ),
        );
        if (ok == true) {
          await api.playlistDelete(id: id);
          p.libraryRev.value++;
          if (mounted) Navigator.pop(context);
        }
    }
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Scaffold(
      appBar: AppBar(
        actions: [
          PopupMenuButton<String>(
            icon: const Icon(Icons.more_horiz_rounded),
            onSelected: _menu,
            itemBuilder: (_) => const [
              PopupMenuItem(value: 'export', child: ListTile(leading: Icon(Icons.ios_share_rounded), title: Text('Export…'), dense: true)),
              PopupMenuItem(value: 'download', child: ListTile(leading: Icon(Icons.download_rounded), title: Text('Download All'), dense: true)),
              PopupMenuItem(value: 'rename', child: ListTile(leading: Icon(Icons.edit_rounded), title: Text('Rename'), dense: true)),
              PopupMenuItem(value: 'delete', child: ListTile(leading: Icon(Icons.delete_outline_rounded, color: Colors.redAccent), title: Text('Delete Playlist'), dense: true)),
            ],
          ),
        ],
      ),
      body: !_loaded
          ? const Center(child: CircularProgressIndicator())
          : CustomScrollView(slivers: [
              SliverToBoxAdapter(
                child: Column(children: [
                  const SizedBox(height: 4),
                  _Mosaic(songs: _songs, size: 230),
                  const SizedBox(height: 16),
                  Text(_name, textAlign: TextAlign.center, style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w800, letterSpacing: -0.4)),
                  Text('${_songs.length} songs', style: TextStyle(color: tok.textDim, fontSize: 14)),
                  const SizedBox(height: 16),
                  if (_songs.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 16),
                      child: Row(children: [
                        Expanded(child: FilledButton.icon(onPressed: () => p.playPlaylist(id), icon: const Icon(Icons.play_arrow_rounded), label: const Text('Play'))),
                        const SizedBox(width: 12),
                        Expanded(child: OutlinedButton.icon(onPressed: () => p.playPlaylist(id, shuffle: true), icon: const Icon(Icons.shuffle_rounded), label: const Text('Shuffle'))),
                      ]),
                    ),
                  const SizedBox(height: 8),
                ]),
              ),
              if (_songs.isEmpty)
                const SliverFillRemaining(hasScrollBody: false, child: EmptyState(icon: Icons.playlist_add_rounded, text: 'Empty. Long-press any song › Add to Playlist.'))
              else
                SliverReorderableList(
                  itemCount: _songs.length,
                  onReorderItem: (from, to) async {
                    setState(() => _songs.insert(to, _songs.removeAt(from)));
                    await api.playlistMove(id: id, from: from, to: to);
                  },
                  itemBuilder: (_, i) => Material(
                    key: ValueKey('${_songs[i].id}-$i'),
                    color: Colors.transparent,
                    child: SongTile(
                      song: _songs[i],
                      player: p,
                      onPlay: () => p.playList(_songs.sublist(i)),
                      onRemoveFromList: () async {
                        await api.playlistRemove(id: id, index: i);
                        p.libraryRev.value++;
                      },
                      removeLabel: 'Remove from Playlist',
                      trailing: ReorderableDragStartListener(index: i, child: Padding(padding: const EdgeInsets.all(10), child: Icon(Icons.drag_handle_rounded, color: tok.textFaint))),
                    ),
                  ),
                ),
              const SliverToBoxAdapter(child: SizedBox(height: 24)),
            ]),
    );
  }
}

/// A playlist cover made from its first four songs' artwork.
class _Mosaic extends StatelessWidget {
  const _Mosaic({required this.songs, required this.size});
  final List<api.SongView> songs;
  final double size;

  @override
  Widget build(BuildContext context) {
    final art = songs.take(4).toList();
    Widget body;
    if (art.length < 4) {
      body = art.isEmpty
          ? Container(color: context.tok.surface, child: Icon(Icons.music_note_rounded, size: 72, color: context.tok.textFaint))
          : Cover(url: art.first.thumbnail, seed: art.first.id, size: size, radius: 0);
    } else {
      final h = size / 2;
      body = Column(children: [
        Row(children: [Cover(url: art[0].thumbnail, seed: art[0].id, size: h, radius: 0), Cover(url: art[1].thumbnail, seed: art[1].id, size: h, radius: 0)]),
        Row(children: [Cover(url: art[2].thumbnail, seed: art[2].id, size: h, radius: 0), Cover(url: art[3].thumbnail, seed: art[3].id, size: h, radius: 0)]),
      ]);
    }
    return Center(
      child: Container(
        width: size,
        height: size,
        decoration: BoxDecoration(borderRadius: BorderRadius.circular(Radii.tile), boxShadow: const [BoxShadow(color: Colors.black38, blurRadius: 30, offset: Offset(0, 12))]),
        clipBehavior: Clip.antiAlias,
        child: body,
      ),
    );
  }
}

/// Kept for screens that still wrap themselves in it; the theme now draws the background.
class AuroraScaffold extends StatelessWidget {
  const AuroraScaffold({super.key, required this.player, required this.child});
  final PlayerController player;
  final Widget child;

  @override
  Widget build(BuildContext context) => child;
}
