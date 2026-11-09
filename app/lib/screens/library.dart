import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/playlist_art.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
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

  Future<void> _pin(api.PlaylistView pl) async {
    await api.playlistPin(id: pl.id, pinned: !pl.pinned);
    p.libraryRev.value++;
  }

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
        for (final pl in _playlists) _PlaylistRow(pl: pl, onTap: () => _push(PlaylistScreen(player: p, playlist: pl)), onPin: () => _pin(pl)),
    ]);
  }
}

/// One playlist in the library: cover mosaic, name, count. Pinned ones sit on
/// top; pin from the menu or with a long press.
class _PlaylistRow extends StatelessWidget {
  const _PlaylistRow({required this.pl, required this.onTap, required this.onPin});
  final api.PlaylistView pl;
  final VoidCallback onTap;
  final VoidCallback onPin;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    return InkWell(
      onTap: onTap,
      onLongPress: onPin,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 6, 4, 6),
        child: Row(children: [
          PlaylistArt(covers: pl.covers, seed: 'pl${pl.id}', size: 56, radius: Radii.cover),
          const SizedBox(width: 14),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(pl.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w500)),
              const SizedBox(height: 2),
              Row(children: [
                if (pl.pinned) ...[Icon(Icons.push_pin_rounded, size: 13, color: accent), const SizedBox(width: 4)],
                Text('Playlist · ${pl.count} ${pl.count == 1 ? 'song' : 'songs'}', style: TextStyle(color: tok.textDim, fontSize: 13)),
              ]),
            ]),
          ),
          PopupMenuButton<String>(
            icon: Icon(Icons.more_horiz_rounded, color: tok.textDim),
            onSelected: (_) => onPin(),
            itemBuilder: (_) => [
              PopupMenuItem(
                value: 'pin',
                child: ListTile(leading: Icon(pl.pinned ? Icons.push_pin_outlined : Icons.push_pin_rounded), title: Text(pl.pinned ? 'Unpin' : 'Pin to top')),
              ),
            ],
          ),
        ]),
      ),
    );
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

  /// Downloads only: playlists with downloaded songs, each with those songs.
  List<(api.PlaylistView, List<api.SongView>)> _groups = [];

  /// Downloads only: downloaded songs that aren't in any playlist.
  List<api.SongView> _loose = [];

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
        final byId = {for (final x in s) x.id: x};
        final groups = <(api.PlaylistView, List<api.SongView>)>[];
        final inSome = <String>{};
        for (final pl in await api.playlists()) {
          final got = [for (final x in await api.playlistSongs(id: pl.id)) if (byId[x.id] != null) byId[x.id]!];
          if (got.isEmpty) continue;
          groups.add((pl, got));
          inSome.addAll(got.map((x) => x.id));
        }
        if (mounted) setState(() { _groups = groups; _loose = [for (final x in s) if (!inSome.contains(x.id)) x]; });
      default:
        s = await api.history(limit: 200);
    }
    if (mounted) setState(() { _songs = s; _notes = notes; });
  }

  List<Widget> _tiles(List<api.SongView> songs) => [
        for (var i = 0; i < songs.length; i++) ...[
          SongTile(song: songs[i], player: p, subtitle: _notes[songs[i].id], onPlay: () => p.playList(songs.sublist(i))),
          const Divider(indent: 78),
        ],
      ];

  Widget _group(api.PlaylistView pl, List<api.SongView> got) {
    final tok = context.tok;
    final all = got.length >= pl.count;
    return Theme(
      data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        tilePadding: const EdgeInsets.fromLTRB(16, 0, 12, 0),
        leading: PlaylistArt(covers: pl.covers, seed: 'pl${pl.id}', size: 48, radius: Radii.cover),
        title: Text(pl.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600)),
        subtitle: Text(all ? 'All ${pl.count} songs downloaded' : '${got.length} of ${pl.count} songs downloaded', style: TextStyle(color: tok.textDim, fontSize: 13)),
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 6),
            child: Row(children: [
              TextButton.icon(onPressed: () => p.playList(got), icon: const Icon(Icons.play_arrow_rounded), label: const Text('Play')),
              TextButton.icon(onPressed: () => p.playList(got, shuffle: true), icon: const Icon(Icons.shuffle_rounded), label: const Text('Shuffle')),
            ]),
          ),
          ..._tiles(got),
        ],
      ),
    );
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
                  if (widget.kind == 'downloads' && _groups.isNotEmpty) ...[
                    // Downloaded playlists first, then songs downloaded on their own.
                    for (final (pl, got) in _groups) _group(pl, got),
                    if (_loose.isNotEmpty) ...[
                      const SectionHeader('Songs'),
                      ..._tiles(_loose),
                    ],
                  ] else
                    ..._tiles(songs),
                ]),
    );
  }
}
