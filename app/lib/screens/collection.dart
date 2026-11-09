import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/shelf.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';

/// An album, a public playlist, a podcast show or an artist.
class CollectionScreen extends StatefulWidget {
  const CollectionScreen({super.key, required this.player, required this.info});
  final PlayerController player;
  final api.CollectionView info;

  @override
  State<CollectionScreen> createState() => _CollectionScreenState();
}

class _CollectionScreenState extends State<CollectionScreen> {
  api.CollectionDetail? _detail;
  String? _error;
  bool _allSongs = false;

  PlayerController get p => widget.player;
  bool get _isArtist => widget.info.kind == 'artist';
  bool get _isAlbum => widget.info.kind == 'album';

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
    try {
      final d = await api.openCollection(id: widget.info.id);
      ThumbCache.precache([...d.songs.map((s) => s.thumbnail), ...d.related.map((r) => r.thumbnail)]);
      if (mounted) setState(() => _detail = d);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
  }

  Future<void> _saveAsPlaylist(api.CollectionDetail d, String name) async {
    final id = await api.playlistCreate(name: name);
    final n = await api.playlistAdd(id: id, songs: d.songs);
    p.libraryRev.value++;
    if (mounted) toast(context, 'Saved $n songs to “$name”');
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final d = _detail;
    final info = d?.info ?? widget.info;
    final name = info.title.isEmpty ? widget.info.title : info.title;
    final cover = info.thumbnail.isNotEmpty ? info.thumbnail : widget.info.thumbnail;
    final kindLabel = switch (widget.info.kind) { 'album' => 'Album', 'podcast' => 'Podcast', 'artist' => 'Artist', _ => 'Playlist' };
    final songs = d?.songs ?? const <api.SongView>[];
    final shown = _isArtist && !_allSongs ? songs.take(10).toList() : songs;
    return Scaffold(
      appBar: AppBar(
        actions: [
          if (d != null && d.songs.isNotEmpty)
            IconButton(tooltip: 'Save as a playlist', icon: const Icon(Icons.playlist_add_rounded), onPressed: () => _saveAsPlaylist(d, name)),
        ],
      ),
      body: _error != null
          ? EmptyState(icon: Icons.error_outline_rounded, text: 'Couldn’t open this.\n$_error')
          : ListView(padding: const EdgeInsets.only(bottom: 24), children: [
              const SizedBox(height: 4),
              Center(
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    shape: _isArtist ? BoxShape.circle : BoxShape.rectangle,
                    borderRadius: _isArtist ? null : BorderRadius.circular(Radii.tile),
                    boxShadow: const [BoxShadow(color: Colors.black38, blurRadius: 30, offset: Offset(0, 12))],
                  ),
                  child: Cover(url: cover, seed: widget.info.id, size: _isArtist ? 190 : 230, radius: Radii.tile, round: _isArtist),
                ),
              ),
              const SizedBox(height: 16),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 24),
                child: Text(name, textAlign: TextAlign.center, style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w800, letterSpacing: -0.4)),
              ),
              const SizedBox(height: 2),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 24),
                child: Text(
                  _isArtist ? kindLabel : (info.subtitle.isEmpty ? kindLabel : info.subtitle),
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 17, color: Theme.of(context).colorScheme.primary),
                ),
              ),
              if (d != null && !_isArtist)
                Padding(
                  padding: const EdgeInsets.only(top: 2),
                  child: Text('$kindLabel · ${songs.length} ${widget.info.kind == 'podcast' ? 'episodes' : 'songs'}', textAlign: TextAlign.center, style: TextStyle(color: tok.textDim, fontSize: 13)),
                ),
              const SizedBox(height: 16),
              if (d == null)
                const Padding(padding: EdgeInsets.all(40), child: Center(child: CircularProgressIndicator()))
              else ...[
                if (songs.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 16),
                    child: Row(children: [
                      Expanded(child: FilledButton.icon(onPressed: () => p.playList(songs), icon: const Icon(Icons.play_arrow_rounded), label: const Text('Play'))),
                      const SizedBox(width: 12),
                      Expanded(child: OutlinedButton.icon(onPressed: () => p.playList(songs, shuffle: true), icon: const Icon(Icons.shuffle_rounded), label: const Text('Shuffle'))),
                    ]),
                  ),
                const SizedBox(height: 8),
                if (songs.isEmpty && d.related.isEmpty) const EmptyState(icon: Icons.music_off_rounded, text: 'Nothing to show here.'),
                if (_isArtist && songs.isNotEmpty) SectionHeader('Songs', action: songs.length > 10 ? (_allSongs ? 'Fewer' : 'See All') : null, onAction: () => setState(() => _allSongs = !_allSongs)),
                for (var i = 0; i < shown.length; i++) ...[
                  SongTile(song: shown[i], player: p, index: _isAlbum ? i + 1 : null, onPlay: () => p.playList(songs.sublist(i))),
                  Divider(indent: _isAlbum ? 56 : 78),
                ],
                for (final (kind, title) in const [('album', 'Albums & Singles'), ('playlist', 'Playlists'), ('artist', 'Similar Artists')])
                  CollectionShelf(
                    title: title,
                    items: d.related.where((r) => r.kind == kind).toList(),
                    size: kind == 'artist' ? 120 : 150,
                    onOpen: (r) => Navigator.of(context).push(MaterialPageRoute(builder: (_) => CollectionScreen(player: p, info: r))),
                  ),
              ],
            ]),
    );
  }
}
