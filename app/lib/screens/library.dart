import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/playlist_art.dart';
import '../widgets/shelf.dart';
import '../widgets/song_actions.dart';
import '../widgets/play_button.dart';
import '../widgets/song_tile.dart';
import '../links.dart';
import 'collection.dart';
import 'import_screen.dart';
import 'local_screen.dart';
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
  int _liked = 0, _downloads = 0, _artists = 0, _albums = 0;

  /// Playlists: "recent" or "name". Pinned ones stay on top either way.
  String _plSort = 'recent';

  List<api.PlaylistView> get _shownPlaylists {
    if (_plSort != 'name') return _playlists;
    return [..._playlists]..sort((a, b) {
        if (a.pinned != b.pinned) return a.pinned ? -1 : 1;
        return a.name.toLowerCase().compareTo(b.name.toLowerCase());
      });
  }

  PlayerController get p => widget.player;

  @override
  void initState() {
    super.initState();
    _load();
    p.libraryRev.addListener(_load);
    p.savedRev.addListener(_load);
  }

  @override
  void dispose() {
    p.libraryRev.removeListener(_load);
    p.savedRev.removeListener(_load);
    super.dispose();
  }

  Future<void> _load() async {
    final ar = await api.savedCollections(kind: 'artist');
    final al = await api.savedCollections(kind: 'other');
    if (mounted) setState(() { _artists = ar.length; _albums = al.length; });
    final pl = await api.playlists();
    final l = await api.likedSongs();
    final d = await api.downloads();
    final sort = await api.settingGet(key: 'sort:playlists') ?? 'recent';
    if (mounted) setState(() { _playlists = pl; _liked = l.length; _downloads = d.length; _plSort = sort; });
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
  Widget build(BuildContext context) => ListenableBuilder(listenable: p, builder: (context, _) => _build(context));

  /// The library row for what's playing now is highlighted ("Playing" + accent).
  bool _playing(String kind, [String id = '']) {
    final s = p.queue.source;
    return p.current != null && s != null && s.kind == kind && (id.isEmpty || s.id == id);
  }

  Widget _build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    Widget row(IconData icon, String label, String? count, VoidCallback onTap, {bool on = false}) => Column(children: [
          ListTile(
            leading: Icon(on ? Icons.graphic_eq_rounded : icon, color: accent),
            title: Text(label, style: TextStyle(fontSize: 18, color: on ? accent : null, fontWeight: on ? FontWeight.w700 : null)),
            subtitle: on ? Text('Playing', style: TextStyle(color: accent, fontSize: 12.5)) : null,
            trailing: Row(mainAxisSize: MainAxisSize.min, children: [
              if (count != null) Text(count, style: TextStyle(color: tok.textDim)),
              Icon(Icons.chevron_right_rounded, color: tok.textFaint),
            ]),
            onTap: onTap,
          ),
          const Divider(indent: 56),
        ]);
    return ListView(padding: const EdgeInsets.only(bottom: 24), children: [
      PageTitle('Library', actions: [
        IconButton(
          tooltip: 'Open a shared link (playlist, party, sync)',
          icon: Icon(Icons.add_link_rounded, color: accent),
          onPressed: () async {
            final link = await promptText(context, title: 'Paste a shared link', hint: 'harp://…');
            if (link != null && link.trim().startsWith('harp://')) Links.open(link.trim());
          },
        ),
        IconButton(tooltip: 'New playlist', icon: Icon(Icons.add_rounded, color: accent), onPressed: _newPlaylist),
      ]),
      row(Icons.favorite_rounded, 'Liked Songs', '$_liked', () => _push(SongListScreen(player: p, kind: 'liked')), on: _playing('liked')),
      row(Icons.arrow_circle_down_rounded, 'Downloaded', '$_downloads', () => _push(SongListScreen(player: p, kind: 'downloads')), on: _playing('downloads')),
      row(Icons.folder_special_rounded, 'On This Device', null, () => _push(LocalScreen(player: p)), on: _playing('local')),
      row(Icons.mic_external_on_rounded, 'Artists', '$_artists', () => _push(SavedScreen(player: p, kind: 'artist'))),
      row(Icons.album_rounded, 'Albums & Podcasts', '$_albums', () => _push(SavedScreen(player: p, kind: 'other'))),
      row(Icons.history_rounded, 'Recently Played', null, () => _push(SongListScreen(player: p, kind: 'history')), on: _playing('history')),
      row(Icons.swap_horiz_rounded, 'Bring Your Music Over', null, () => _push(ImportScreen(player: p))),
      SectionHeader(
        'Playlists',
        action: _plSort == 'name' ? 'Sort: Name' : 'Sort: Recent',
        onAction: () {
          final v = _plSort == 'name' ? 'recent' : 'name';
          setState(() => _plSort = v);
          api.settingSet(key: 'sort:playlists', value: v);
        },
      ),
      if (_playlists.isEmpty)
        Padding(padding: const EdgeInsets.symmetric(horizontal: 16), child: Text('Make a playlist, or bring one over from another app.', style: TextStyle(color: tok.textDim)))
      else
        ...() {
          // Folders first (each opens its playlists), then playlists in no folder.
          final shown = _shownPlaylists;
          final folders = <String, List<api.PlaylistView>>{};
          for (final pl in shown) {
            if (pl.folder.isNotEmpty) folders.putIfAbsent(pl.folder, () => []).add(pl);
          }
          final names = folders.keys.toList()..sort((a, b) => a.toLowerCase().compareTo(b.toLowerCase()));
          return [
            for (final f in names)
              ListTile(
                contentPadding: const EdgeInsets.fromLTRB(16, 2, 12, 2),
                leading: Container(
                  width: 56,
                  height: 56,
                  decoration: BoxDecoration(color: tok.surface, borderRadius: BorderRadius.circular(Radii.cover)),
                  child: Icon(folders[f]!.any((pl) => _playing('playlist', '${pl.id}')) ? Icons.graphic_eq_rounded : Icons.folder_rounded, color: accent, size: 28),
                ),
                title: Text(f, style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w500)),
                subtitle: Text('Folder · ${folders[f]!.length} ${folders[f]!.length == 1 ? 'playlist' : 'playlists'}', style: TextStyle(color: tok.textDim, fontSize: 13)),
                trailing: Icon(Icons.chevron_right_rounded, color: tok.textFaint),
                onTap: () => _push(FolderScreen(player: p, folder: f)),
              ),
            for (final pl in shown.where((x) => x.folder.isEmpty))
              _PlaylistRow(pl: pl, playing: _playing('playlist', '${pl.id}'), paused: !p.playing, onToggle: p.togglePlay, onTap: () => _push(PlaylistScreen(player: p, playlist: pl)), onPin: () => _pin(pl)),
          ];
        }(),
    ]);
  }
}

/// A folder of playlists.
class FolderScreen extends StatefulWidget {
  const FolderScreen({super.key, required this.player, required this.folder});
  final PlayerController player;
  final String folder;

  @override
  State<FolderScreen> createState() => _FolderScreenState();
}

class _FolderScreenState extends State<FolderScreen> {
  late String _name = widget.folder;
  List<api.PlaylistView> _lists = [];

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
    final all = await api.playlists();
    if (mounted) setState(() => _lists = all.where((x) => x.folder == _name).toList());
  }

  bool _playing(api.PlaylistView pl) {
    final s = p.queue.source;
    return p.current != null && s != null && s.kind == 'playlist' && s.id == '${pl.id}';
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(_name),
        actions: [
          IconButton(
            tooltip: 'Rename folder',
            icon: const Icon(Icons.edit_rounded),
            onPressed: () async {
              final n = await promptText(context, title: 'Rename folder', initial: _name);
              if (n == null || n.trim().isEmpty) return;
              await api.folderRename(old: _name, new_: n.trim());
              setState(() => _name = n.trim());
              p.libraryRev.value++;
            },
          ),
        ],
      ),
      body: ListenableBuilder(
        listenable: p,
        builder: (context, _) => _lists.isEmpty
            ? const EmptyState(icon: Icons.folder_open_rounded, text: 'Empty. Move playlists here from their … menu.')
            : ListView(children: [
                for (final pl in _lists)
                  _PlaylistRow(
                    pl: pl,
                    playing: _playing(pl),
                    paused: !p.playing,
                    onToggle: p.togglePlay,
                    onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => PlaylistScreen(player: p, playlist: pl))),
                    onPin: () async {
                      await api.playlistPin(id: pl.id, pinned: !pl.pinned);
                      p.libraryRev.value++;
                    },
                  ),
              ]),
      ),
    );
  }
}

/// Artists you follow, or albums, playlists and podcasts you saved, as a grid.
class SavedScreen extends StatefulWidget {
  const SavedScreen({super.key, required this.player, required this.kind});
  final PlayerController player;

  /// "artist" or "other"
  final String kind;

  @override
  State<SavedScreen> createState() => _SavedScreenState();
}

class _SavedScreenState extends State<SavedScreen> {
  List<api.CollectionView>? _items;

  /// "recent" or "name".
  String _sort = 'recent';

  @override
  void initState() {
    super.initState();
    _load();
    widget.player.savedRev.addListener(_load);
  }

  @override
  void dispose() {
    widget.player.savedRev.removeListener(_load);
    super.dispose();
  }

  Future<void> _load() async {
    final v = await api.savedCollections(kind: widget.kind);
    final sort = await api.settingGet(key: 'sort:saved:${widget.kind}');
    if (mounted) setState(() { _items = v; _sort = sort ?? _sort; });
  }

  @override
  Widget build(BuildContext context) {
    final artists = widget.kind == 'artist';
    final items = _items == null
        ? null
        : _sort == 'name'
            ? ([..._items!]..sort((a, b) => a.title.toLowerCase().compareTo(b.title.toLowerCase())))
            : _items;
    return Scaffold(
      appBar: AppBar(
        title: Text(artists ? 'Artists' : 'Albums & Podcasts'),
        actions: [
          PopupMenuButton<String>(
            tooltip: 'Sort',
            icon: const Icon(Icons.sort_rounded),
            initialValue: _sort,
            onSelected: (v) {
              setState(() => _sort = v);
              api.settingSet(key: 'sort:saved:${widget.kind}', value: v);
            },
            itemBuilder: (_) => const [
              PopupMenuItem(value: 'recent', child: Text('Recently added')),
              PopupMenuItem(value: 'name', child: Text('Name')),
            ],
          ),
        ],
      ),
      body: items == null
          ? const Center(child: CircularProgressIndicator())
          : items.isEmpty
              ? EmptyState(
                  icon: artists ? Icons.mic_external_on_rounded : Icons.album_rounded,
                  text: artists ? 'Follow an artist from their page to see them here' : 'Save an album, playlist or podcast (the + at the top of its page)',
                )
              : LayoutBuilder(builder: (context, c) {
                  final per = (c.maxWidth / 170).floor().clamp(2, 8);
                  final side = (c.maxWidth - 32 - (per - 1) * 14) / per;
                  return GridView.builder(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
                    gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(crossAxisCount: per, mainAxisSpacing: 16, crossAxisSpacing: 14, mainAxisExtent: side + 52),
                    itemCount: items.length,
                    itemBuilder: (_, i) => CollectionCard(
                      c: items[i],
                      size: side,
                      onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => CollectionScreen(player: widget.player, info: items[i]))),
                    ),
                  );
                }),
    );
  }
}

/// Sorts songs by "title" or "artist" (case-insensitive); "recent" keeps the
/// order they came in (newest first).
List<api.SongView> sortSongs(List<api.SongView> l, String by) {
  if (by != 'title' && by != 'artist') return l;
  String k(api.SongView s) => (by == 'title' ? s.title : '${s.artist}\u0000${s.title}').toLowerCase();
  return [...l]..sort((a, b) => k(a).compareTo(k(b)));
}

/// One playlist in the library: cover mosaic, name, count. Pinned ones sit on
/// top; pin from the menu or with a long press.
class _PlaylistRow extends StatelessWidget {
  const _PlaylistRow({required this.pl, required this.onTap, required this.onPin, this.playing = false, this.paused = false, this.onToggle});
  final api.PlaylistView pl;
  final bool playing;
  final bool paused;
  final VoidCallback? onToggle;
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
          PlaylistArt(covers: pl.covers, custom: pl.cover, seed: 'pl${pl.id}', size: 56, radius: Radii.cover),
          const SizedBox(width: 14),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(pl.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 16, fontWeight: playing ? FontWeight.w700 : FontWeight.w500, color: playing ? accent : null)),
              const SizedBox(height: 2),
              Row(children: [
                if (playing) ...[Icon(Icons.graphic_eq_rounded, size: 14, color: accent), const SizedBox(width: 4), Text('Playing · ', style: TextStyle(color: accent, fontSize: 13))],
                if (pl.pinned) ...[Icon(Icons.push_pin_rounded, size: 13, color: accent), const SizedBox(width: 4)],
                // Shrinks (…) instead of running under the play/pause button.
                Flexible(
                  child: Text('Playlist · ${pl.count} ${pl.count == 1 ? 'song' : 'songs'}', maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 13)),
                ),
              ]),
            ]),
          ),
          if (playing)
            Padding(
              padding: const EdgeInsets.only(left: 6),
              child: IconButton(
                tooltip: paused ? 'Play' : 'Pause',
                icon: Icon(paused ? Icons.play_circle_fill_rounded : Icons.pause_circle_filled_rounded, color: accent, size: 32),
                onPressed: onToggle,
              ),
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

  /// "recent", "title" or "artist" (remembered per list).
  String _sort = 'recent';

  /// Text typed in the filter box.
  String _filter = '';

  List<api.SongView> _filtered(List<api.SongView> l) {
    final q = _filter.toLowerCase();
    if (q.isEmpty) return l;
    return l.where((s) => s.title.toLowerCase().contains(q) || s.artist.toLowerCase().contains(q) || (s.album ?? '').toLowerCase().contains(q)).toList();
  }

  List<api.SongView> _sorted(List<api.SongView> l) => sortSongs(l, _sort);

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
    final sort = await api.settingGet(key: 'sort:${widget.kind}');
    if (sort != null && mounted) _sort = sort;
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

  From get _from => switch (widget.kind) {
        'liked' => (kind: 'liked', id: '', title: 'Liked Songs'),
        'downloads' => (kind: 'downloads', id: '', title: 'Downloaded'),
        _ => (kind: 'history', id: '', title: 'Recently Played'),
      };

  List<Widget> _tiles(List<api.SongView> songs) => [
        for (var i = 0; i < songs.length; i++) ...[
          SongTile(song: songs[i], player: p, subtitle: _notes[songs[i].id], onPlay: () => p.playList(songs.sublist(i), from: _from)),
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
        leading: PlaylistArt(covers: pl.covers, custom: pl.cover, seed: 'pl${pl.id}', size: 48, radius: Radii.cover),
        title: Text(pl.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600)),
        subtitle: Text(all ? 'All ${pl.count} songs downloaded' : '${got.length} of ${pl.count} songs downloaded', style: TextStyle(color: tok.textDim, fontSize: 13)),
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 6),
            child: Row(children: [
              TextButton.icon(onPressed: () => p.playList(got, from: (kind: 'downloads', id: '', title: 'Downloaded · ${pl.name}')), icon: const Icon(Icons.play_arrow_rounded), label: const Text('Play')),
              TextButton.icon(onPressed: () => p.playList(got, shuffle: true, from: (kind: 'downloads', id: '', title: 'Downloaded · ${pl.name}')), icon: const Icon(Icons.shuffle_rounded), label: const Text('Shuffle')),
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
    final songs = _songs == null ? null : _sorted(_filtered(_songs!));
    return Scaffold(
      appBar: AppBar(
        title: Text(title),
        actions: [
          if (widget.kind != 'history')
            PopupMenuButton<String>(
              tooltip: 'Sort',
              icon: const Icon(Icons.sort_rounded),
              initialValue: _sort,
              onSelected: (v) {
                setState(() => _sort = v);
                api.settingSet(key: 'sort:${widget.kind}', value: v);
              },
              itemBuilder: (_) => const [
                PopupMenuItem(value: 'recent', child: Text('Recently added')),
                PopupMenuItem(value: 'title', child: Text('Title')),
                PopupMenuItem(value: 'artist', child: Text('Artist')),
              ],
            ),
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
          : _songs!.isEmpty
              ? EmptyState(
                  icon: Icons.music_note_rounded,
                  text: switch (widget.kind) {
                    'liked' => 'Heart a song to keep it here',
                    'downloads' => 'Download songs (… › Download) to play them offline',
                    _ => 'Nothing played yet',
                  },
                )
              : ListView(padding: const EdgeInsets.only(bottom: 24), children: [
                  if (widget.kind != 'history')
                    Padding(
                      padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                      child: TextField(
                        decoration: InputDecoration(hintText: 'Filter ${_songs!.length} songs', prefixIcon: const Icon(Icons.filter_list_rounded), isDense: true),
                        onChanged: (v) => setState(() => _filter = v.trim()),
                      ),
                    ),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                    child: Row(children: [
                      Expanded(child: ListPlayButton(player: p, kind: _from.kind, id: _from.id, onPlay: () => p.playList(songs, from: _from))),
                      const SizedBox(width: 12),
                      Expanded(child: OutlinedButton.icon(onPressed: () => p.playList(songs, shuffle: true, from: _from), icon: const Icon(Icons.shuffle_rounded), label: const Text('Shuffle'))),
                    ]),
                  ),
                  if (widget.kind == 'downloads' && _groups.isNotEmpty && _filter.isEmpty) ...[
                    // Downloaded playlists first, then songs downloaded on their own.
                    for (final (pl, got) in _groups) _group(pl, _sorted(got)),
                    if (_loose.isNotEmpty) ...[
                      const SectionHeader('Songs'),
                      ..._tiles(_sorted(_loose)),
                    ],
                  ] else
                    ..._tiles(songs),
                ]),
    );
  }
}
