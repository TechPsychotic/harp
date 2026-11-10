import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../native.dart';
import '../player/controller.dart';
import '../src/rust/api/party.dart' as net;
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
import 'library.dart' show sortSongs;

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

  /// "custom" (your order: drag to change it), "title" or "artist". Remembered per playlist.
  String _sort = 'custom';
  late String _name = widget.playlist.name;
  late bool _autoDl = widget.playlist.autoDownload;
  bool _loaded = false;

  /// Play order toggles, remembered per playlist (like Spotify).
  bool _shuffle = false, _smart = false;
  bool _downloadingAll = false;

  /// "Recommended for this playlist" (songs like its own, not in it).
  List<api.SongView>? _recs;
  bool _recsLoading = false;

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
    _sort = await api.settingGet(key: 'sort:pl:$id') ?? _sort;
    _shuffle = await api.settingGet(key: 'pl_shuffle:$id') == '1';
    _smart = await api.settingGet(key: 'pl_smart:$id') == '1';
    ThumbCache.precache(s.map((x) => x.thumbnail));
    final me = (await api.playlists()).where((x) => x.id == id).firstOrNull;
    if (mounted) setState(() { _songs = s; _loaded = true; _autoDl = me?.autoDownload ?? _autoDl; });
  }

  bool get _isThis {
    final src = p.queue.source;
    return p.current != null && src != null && src.kind == 'playlist' && src.id == '$id';
  }

  Future<void> _play() async {
    if (_isThis) return p.togglePlay();
    await p.playPlaylist(id, shuffle: _shuffle, smart: _smart);
  }

  Future<void> _toggleShuffle() async {
    setState(() => _shuffle = !_shuffle);
    await api.settingSet(key: 'pl_shuffle:$id', value: _shuffle ? '1' : '0');
    if (_shuffle && _isThis) await p.shuffleUpcoming();
    if (mounted) toast(context, _shuffle ? 'Shuffle on' : 'Shuffle off');
  }

  Future<void> _toggleSmart() async {
    setState(() => _smart = !_smart);
    await api.settingSet(key: 'pl_smart:$id', value: _smart ? '1' : '0');
    if (_smart && _isThis) unawaited(p.mixInSmartPicks(id));
    if (mounted) toast(context, _smart ? 'Smart Shuffle on: songs like these get mixed in as it plays' : 'Smart Shuffle off');
  }

  Future<void> _downloadAll() async {
    if (_downloadingAll) return;
    final missing = _songs.where((s) => !s.downloaded).toList();
    if (missing.isEmpty) {
      return _menu(_autoDl ? 'nodl' : 'download');
    }
    await _menu('download');
  }

  Future<void> _share() async {
    String link;
    try {
      link = await net.playlistShare(id: id);
    } catch (e) {
      if (mounted) toast(context, "Couldn't share: $e");
      return;
    }
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Share “$_name”'),
        content: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          const Text('Friends open this link in Harp to get a copy of the playlist. It works while sharing is on and Harp is open here.'),
          const SizedBox(height: 10),
          SelectableText(link, style: TextStyle(fontSize: 12, color: ctx.tok.textDim)),
        ]),
        actions: [
          TextButton(
            onPressed: () {
              net.playlistShareStop(id: id);
              Navigator.pop(ctx);
              toast(context, 'Stopped sharing');
            },
            child: const Text('Stop sharing'),
          ),
          TextButton(
            onPressed: () async {
              await Clipboard.setData(ClipboardData(text: link));
              if (ctx.mounted) Navigator.pop(ctx);
              if (mounted) toast(context, 'Link copied');
            },
            child: const Text('Copy'),
          ),
          FilledButton(
            onPressed: () async {
              final text = 'Listen to my playlist “$_name” on Harp: $link';
              if (!await Native.share(text)) await Clipboard.setData(ClipboardData(text: link));
              if (ctx.mounted) Navigator.pop(ctx);
            },
            child: const Text('Share'),
          ),
        ],
      ),
    );
  }

  Future<void> _loadRecs() async {
    if (_recsLoading || _songs.isEmpty || p.settings.offline) return;
    setState(() => _recsLoading = true);
    try {
      final r = await api.playlistSmartPicks(id: id, count: 10);
      ThumbCache.precache(r.map((x) => x.thumbnail));
      if (mounted) setState(() => _recs = r);
    } catch (_) {
      if (mounted) setState(() => _recs = const []);
    } finally {
      if (mounted) setState(() => _recsLoading = false);
    }
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
      case 'nodl':
        await api.playlistAutoDownload(id: id, on_: false);
        setState(() => _autoDl = false);
        if (mounted) toast(context, 'New songs won’t be downloaded');
      case 'download':
        // From now on, songs added to this playlist are downloaded too.
        await api.playlistAutoDownload(id: id, on_: true);
        setState(() => _autoDl = true);
        var n = 0;
        setState(() => _downloadingAll = true);
        try {
          for (final s in _songs.where((s) => !s.downloaded)) {
            if (await p.download(s) == null) n++;
          }
        } finally {
          if (mounted) setState(() => _downloadingAll = false);
        }
        if (mounted) toast(context, 'Downloaded $n songs. New songs here will download too.');
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
            tooltip: 'Sort',
            icon: const Icon(Icons.sort_rounded),
            initialValue: _sort,
            onSelected: (v) {
              setState(() => _sort = v);
              api.settingSet(key: 'sort:pl:$id', value: v);
            },
            itemBuilder: (_) => const [
              PopupMenuItem(value: 'custom', child: Text('Custom order (drag to change)')),
              PopupMenuItem(value: 'title', child: Text('Title')),
              PopupMenuItem(value: 'artist', child: Text('Artist')),
            ],
          ),
          PopupMenuButton<String>(
            icon: const Icon(Icons.more_horiz_rounded),
            onSelected: _menu,
            itemBuilder: (_) => [
              const PopupMenuItem(value: 'export', child: ListTile(leading: Icon(Icons.ios_share_rounded), title: Text('Export…'), dense: true)),
              const PopupMenuItem(value: 'download', child: ListTile(leading: Icon(Icons.download_rounded), title: Text('Download All'), dense: true)),
              if (_autoDl)
                const PopupMenuItem(value: 'nodl', child: ListTile(leading: Icon(Icons.file_download_off_rounded), title: Text('Stop downloading new songs'), dense: true)),
              const PopupMenuItem(value: 'rename', child: ListTile(leading: Icon(Icons.edit_rounded), title: Text('Rename'), dense: true)),
              const PopupMenuItem(value: 'delete', child: ListTile(leading: Icon(Icons.delete_outline_rounded, color: Colors.redAccent), title: Text('Delete Playlist'), dense: true)),
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
                  Text('${_songs.length} songs${_autoDl ? ' · new songs download automatically' : ''}', style: TextStyle(color: tok.textDim, fontSize: 14)),
                  const SizedBox(height: 16),
                  if (_songs.isNotEmpty) _actions(context),
                  const SizedBox(height: 4),
                ]),
              ),
              if (_songs.isEmpty)
                const SliverFillRemaining(hasScrollBody: false, child: EmptyState(icon: Icons.playlist_add_rounded, text: 'Empty. Long-press any song › Add to Playlist.'))
              else
                if (_sort != 'custom')
                  // Sorted: a plain list (dragging only makes sense in your own order).
                  SliverList.builder(
                    itemCount: _songs.length,
                    itemBuilder: (_, i) {
                      final shown = sortSongs(_songs, _sort);
                      final s = shown[i];
                      return SongTile(
                        song: s,
                        player: p,
                        onPlay: () => p.playList(shown.sublist(i), from: (kind: 'playlist', id: '$id', title: _name)),
                        onRemoveFromList: () async {
                          await api.playlistRemove(id: id, index: _songs.indexWhere((x) => x.id == s.id));
                          p.libraryRev.value++;
                        },
                        removeLabel: 'Remove from Playlist',
                      );
                    },
                  )
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
                      onPlay: () => p.playList(_songs.sublist(i), from: (kind: 'playlist', id: '$id', title: _name)),
                      onRemoveFromList: () async {
                        await api.playlistRemove(id: id, index: i);
                        p.libraryRev.value++;
                      },
                      removeLabel: 'Remove from Playlist',
                      trailing: ReorderableDragStartListener(index: i, child: Padding(padding: const EdgeInsets.all(10), child: Icon(Icons.drag_handle_rounded, color: tok.textFaint))),
                    ),
                  ),
                ),
              if (_songs.isNotEmpty && !p.settings.offline) SliverToBoxAdapter(child: _recommended(context)),
              const SliverToBoxAdapter(child: SizedBox(height: 24)),
            ]),
    );
  }

  /// Spotify-style: download and share on the left; shuffle, smart shuffle and
  /// a big round play / pause on the right.
  Widget _actions(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    final allDown = _songs.every((s) => s.downloaded);
    Widget toggle(IconData icon, bool on, String tip, VoidCallback onTap) => Column(mainAxisSize: MainAxisSize.min, children: [
          IconButton(tooltip: tip, iconSize: 26, icon: Icon(icon, color: on ? accent : tok.textDim), onPressed: onTap),
          // A dot under it while on, as on Spotify.
          Container(width: 4, height: 4, decoration: BoxDecoration(shape: BoxShape.circle, color: on ? accent : Colors.transparent)),
        ]);
    return Padding(
      padding: const EdgeInsets.fromLTRB(8, 0, 16, 0),
      child: Row(children: [
        IconButton(
          tooltip: _downloadingAll ? 'Downloading…' : (allDown ? 'Downloaded' : 'Download'),
          iconSize: 28,
          icon: Icon(
            _downloadingAll ? Icons.downloading_rounded : Icons.download_for_offline_rounded,
            color: allDown || _downloadingAll ? accent : tok.textDim,
          ),
          onPressed: _downloadAll,
        ),
        IconButton(
          tooltip: net.playlistShareLink(id: id) != null ? 'Sharing: tap for the link' : 'Share',
          iconSize: 26,
          icon: Icon(Icons.ios_share_rounded, color: net.playlistShareLink(id: id) != null ? accent : tok.textDim),
          onPressed: _share,
        ),
        const Spacer(),
        toggle(Icons.shuffle_rounded, _shuffle, _shuffle ? 'Shuffle: on' : 'Shuffle: off', _toggleShuffle),
        toggle(Icons.auto_awesome_rounded, _smart, _smart ? 'Smart Shuffle: on' : 'Smart Shuffle: mix in songs like these', _toggleSmart),
        const SizedBox(width: 8),
        ListenableBuilder(
          listenable: p,
          builder: (context, _) {
            final pause = _isThis && p.playing;
            return SizedBox(
              width: 56,
              height: 56,
              child: FilledButton(
                style: FilledButton.styleFrom(shape: const CircleBorder(), padding: EdgeInsets.zero),
                onPressed: _play,
                child: Icon(pause ? Icons.pause_rounded : Icons.play_arrow_rounded, size: 34),
              ),
            );
          },
        ),
      ]),
    );
  }

  /// Songs like this playlist's: tap + to add one.
  Widget _recommended(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    final recs = _recs;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      SectionHeader('Recommended', subtitle: 'Based on what’s in this playlist', action: recs == null ? null : 'Refresh', onAction: () {
        setState(() => _recs = null);
        _loadRecs();
      }),
      if (recs == null)
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: _recsLoading
              ? const Center(child: Padding(padding: EdgeInsets.all(12), child: CircularProgressIndicator()))
              : OutlinedButton.icon(onPressed: _loadRecs, icon: const Icon(Icons.auto_awesome_rounded), label: const Text('Find songs for this playlist')),
        )
      else if (recs.isEmpty)
        Padding(padding: const EdgeInsets.symmetric(horizontal: 16), child: Text('Nothing found right now.', style: TextStyle(color: tok.textDim)))
      else
        for (final r in recs)
          SongTile(
            song: r,
            player: p,
            trailing: IconButton(
              tooltip: 'Add to this playlist',
              icon: Icon(Icons.add_circle_outline_rounded, color: accent),
              onPressed: () async {
                await api.playlistAdd(id: id, songs: [r]);
                setState(() => _recs = [...recs]..removeWhere((x) => x.id == r.id));
                p.libraryRev.value++;
                if (context.mounted) toast(context, 'Added “${r.title}”');
              },
            ),
          ),
    ]);
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
