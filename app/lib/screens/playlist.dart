import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';

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
    if (mounted) setState(() { _songs = s; _loaded = true; });
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return AuroraScaffold(
      player: p,
      child: Scaffold(
        appBar: AppBar(
          backgroundColor: Colors.transparent,
          title: Text(_name),
          actions: [
            PopupMenuButton<String>(
              onSelected: (v) async {
                if (v == 'rename') {
                  final n = await promptText(context, title: 'Rename playlist', initial: _name);
                  if (n != null && n.trim().isNotEmpty) {
                    await api.playlistRename(id: id, name: n);
                    setState(() => _name = n.trim());
                    p.libraryRev.value++;
                  }
                } else if (v == 'download') {
                  var n = 0;
                  for (final s in _songs.where((s) => !s.downloaded)) {
                    final err = await p.download(s);
                    if (err == null) n++;
                  }
                  if (context.mounted) toast(context, 'Downloaded $n songs');
                } else if (v == 'delete') {
                  await api.playlistDelete(id: id);
                  p.libraryRev.value++;
                  if (context.mounted) Navigator.pop(context);
                }
              },
              itemBuilder: (_) => const [
                PopupMenuItem(value: 'rename', child: Text('Rename')),
                PopupMenuItem(value: 'download', child: Text('Download all')),
                PopupMenuItem(value: 'delete', child: Text('Delete playlist')),
              ],
            ),
          ],
        ),
        body: !_loaded
            ? const Center(child: CircularProgressIndicator())
            : Column(children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
                  child: Row(children: [
                    Text('${_songs.length} songs', style: TextStyle(color: tok.textDim)),
                    const Spacer(),
                    FilledButton.icon(
                      onPressed: _songs.isEmpty ? null : () => p.playPlaylist(id),
                      icon: const Icon(Icons.play_arrow_rounded),
                      label: const Text('Play'),
                    ),
                    const SizedBox(width: 8),
                    OutlinedButton.icon(
                      onPressed: _songs.isEmpty ? null : () => p.playPlaylist(id, shuffle: true),
                      icon: const Icon(Icons.shuffle_rounded),
                      label: const Text('Shuffle'),
                    ),
                  ]),
                ),
                Expanded(
                  child: _songs.isEmpty
                      ? Center(child: Text('Empty. Add songs from the ⋮ menu.', style: TextStyle(color: tok.textFaint)))
                      : ReorderableListView.builder(
                          padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                          buildDefaultDragHandles: false,
                          itemCount: _songs.length,
                          onReorderItem: (from, to) async {
                            setState(() => _songs.insert(to, _songs.removeAt(from)));
                            await api.playlistMove(id: id, from: from, to: to);
                          },
                          itemBuilder: (_, i) => SongTile(
                            key: ValueKey('${_songs[i].id}-$i'),
                            song: _songs[i],
                            player: p,
                            onRemoveFromList: () async {
                              await api.playlistRemove(id: id, index: i);
                              p.libraryRev.value++;
                            },
                            removeLabel: 'Remove from playlist',
                            trailing: ReorderableDragStartListener(index: i, child: const Padding(padding: EdgeInsets.all(8), child: Icon(Icons.drag_handle_rounded))),
                          ),
                        ),
                ),
              ]),
      ),
    );
  }
}

/// Wraps a pushed screen in the app background so it matches the shell.
class AuroraScaffold extends StatelessWidget {
  const AuroraScaffold({super.key, required this.player, required this.child});
  final PlayerController player;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: player,
      builder: (_, _) => AuroraBackground(
        accent: HarpColors.accentFor(player.current?.id),
        active: player.playing && player.settings.fullEffects,
        flat: player.settings.flatBackground,
        level: player.settings.reactiveBackground && player.settings.fullEffects ? player.viz.level : null,
        child: child,
      ),
    );
  }
}
