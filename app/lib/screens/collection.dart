import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
import 'playlist.dart' show AuroraScaffold;

/// An album, a public playlist or a podcast show: its songs or episodes.
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
    try {
      final d = await api.openCollection(id: widget.info.id);
      ThumbCache.precache(d.songs.map((s) => s.thumbnail));
      if (mounted) setState(() => _detail = d);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final d = _detail;
    final info = d?.info ?? widget.info;
    final cover = info.thumbnail.isNotEmpty ? info.thumbnail : widget.info.thumbnail;
    final kindLabel = switch (widget.info.kind) { 'album' => 'Album', 'podcast' => 'Podcast', 'artist' => 'Artist', _ => 'Playlist' };
    final isArtist = widget.info.kind == 'artist';
    return AuroraScaffold(
      player: p,
      child: Scaffold(
        appBar: AppBar(backgroundColor: Colors.transparent),
        body: _error != null
            ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text(_error!, style: const TextStyle(color: Color(0xFFFF8A8A)))))
            : ListView(padding: const EdgeInsets.fromLTRB(16, 0, 16, 16), children: [
                Center(child: Cover(url: cover, seed: widget.info.id, size: 200, radius: Radii.tile, round: isArtist)),
                const SizedBox(height: 14),
                Text(info.title.isEmpty ? widget.info.title : info.title, textAlign: TextAlign.center, style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w700)),
                const SizedBox(height: 2),
                Text(isArtist ? kindLabel : '$kindLabel${info.subtitle.isEmpty ? '' : ' · ${info.subtitle}'}', textAlign: TextAlign.center, style: TextStyle(color: tok.textDim)),
                const SizedBox(height: 14),
                if (d == null)
                  const Padding(padding: EdgeInsets.all(32), child: Center(child: CircularProgressIndicator()))
                else ...[
                  Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                    FilledButton.icon(onPressed: d.songs.isEmpty ? null : () => p.playList(d.songs), icon: const Icon(Icons.play_arrow_rounded), label: const Text('Play')),
                    const SizedBox(width: 8),
                    OutlinedButton.icon(onPressed: d.songs.isEmpty ? null : () => p.playList(d.songs, shuffle: true), icon: const Icon(Icons.shuffle_rounded), label: const Text('Shuffle')),
                    const SizedBox(width: 8),
                    IconButton.outlined(
                      tooltip: 'Save as a playlist',
                      icon: const Icon(Icons.playlist_add_rounded),
                      onPressed: d.songs.isEmpty
                          ? null
                          : () async {
                              final id = await api.playlistCreate(name: info.title);
                              final n = await api.playlistAdd(id: id, songs: d.songs);
                              p.libraryRev.value++;
                              if (context.mounted) toast(context, 'Saved $n songs to “${info.title}”');
                            },
                    ),
                  ]),
                  const SizedBox(height: 12),
                  if (d.songs.isEmpty && d.related.isEmpty) const EmptyState(icon: Icons.music_off_rounded, text: 'Nothing to show here.'),
                  if (isArtist && d.songs.isNotEmpty) const Padding(padding: EdgeInsets.fromLTRB(0, 8, 0, 8), child: Text('Songs', style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700))),
                  for (var i = 0; i < d.songs.length; i++)
                    SongTile(song: d.songs[i], player: p, onPlay: () => p.playList(d.songs.sublist(i))),
                  if (d.songs.length >= 100) Padding(padding: const EdgeInsets.all(8), child: Text('${d.songs.length} songs', textAlign: TextAlign.center, style: TextStyle(color: tok.textFaint))),
                  for (final (kind, title) in const [('album', 'Albums & singles'), ('playlist', 'Playlists'), ('artist', 'Similar artists')])
                    if (d.related.any((r) => r.kind == kind)) ...[
                      Padding(padding: const EdgeInsets.fromLTRB(0, 16, 0, 10), child: Text(title, style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w700))),
                      SizedBox(
                        height: 176,
                        child: ListView(
                          scrollDirection: Axis.horizontal,
                          children: [
                            for (final r in d.related.where((r) => r.kind == kind))
                              InkWell(
                                borderRadius: BorderRadius.circular(Radii.tile),
                                onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => CollectionScreen(player: p, info: r))),
                                child: SizedBox(
                                  width: 136,
                                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                                    Cover(url: r.thumbnail, seed: r.id, size: 120, radius: Radii.tile, round: r.kind == 'artist'),
                                    const SizedBox(height: 6),
                                    Text(r.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 13.5)),
                                    Text(r.subtitle, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 12.5)),
                                  ]),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ],
                ],
              ]),
      ),
    );
  }
}
