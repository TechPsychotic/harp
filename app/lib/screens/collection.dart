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
    final kindLabel = switch (widget.info.kind) { 'album' => 'Album', 'podcast' => 'Podcast', _ => 'Playlist' };
    return AuroraScaffold(
      player: p,
      child: Scaffold(
        appBar: AppBar(backgroundColor: Colors.transparent),
        body: _error != null
            ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text(_error!, style: const TextStyle(color: Color(0xFFFF8A8A)))))
            : ListView(padding: const EdgeInsets.fromLTRB(16, 0, 16, 16), children: [
                Center(child: Cover(url: cover, seed: widget.info.id, size: 200, radius: Radii.tile)),
                const SizedBox(height: 14),
                Text(info.title.isEmpty ? widget.info.title : info.title, textAlign: TextAlign.center, style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w700)),
                const SizedBox(height: 2),
                Text('$kindLabel${info.subtitle.isEmpty ? '' : ' · ${info.subtitle}'}', textAlign: TextAlign.center, style: TextStyle(color: tok.textDim)),
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
                  if (d.songs.isEmpty) Padding(padding: const EdgeInsets.all(24), child: Text('Nothing to show here.', textAlign: TextAlign.center, style: TextStyle(color: tok.textFaint))),
                  for (var i = 0; i < d.songs.length; i++)
                    SongTile(song: d.songs[i], player: p, onPlay: () => p.playList(d.songs.sublist(i))),
                ],
              ]),
      ),
    );
  }
}
