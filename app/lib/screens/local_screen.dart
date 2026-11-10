import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../native.dart';
import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
import 'library.dart' show sortSongs;

/// Your own music files: pick folders, Harp reads their tags and plays them
/// like any other song (offline too). Likes, playlists and the queue work the same.
class LocalScreen extends StatefulWidget {
  const LocalScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<LocalScreen> createState() => _LocalScreenState();
}

class _LocalScreenState extends State<LocalScreen> {
  List<api.SongView>? _songs;
  List<String> _folders = [];
  bool _scanning = false;
  String _sort = 'artist';
  String _filter = '';

  PlayerController get p => widget.player;
  static const From _from = (kind: 'local', id: '', title: 'On This Device');

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final f = await api.localFolders();
    final s = await api.localSongs();
    if (mounted) setState(() { _folders = f; _songs = s; });
  }

  Future<void> _scan(Future<int> Function() job) async {
    setState(() => _scanning = true);
    try {
      final n = await job();
      if (mounted) toast(context, 'Found $n songs');
    } catch (e) {
      if (mounted) toast(context, "Couldn't read the folder: $e");
    } finally {
      if (mounted) setState(() => _scanning = false);
      await _load();
    }
  }

  Future<void> _addFolder() async {
    if (!await Native.requestAudioAccess()) {
      if (mounted) toast(context, 'Harp needs permission to read your music files');
      return;
    }
    final dir = await FilePicker.getDirectoryPath(dialogTitle: 'Choose a music folder');
    if (dir == null) return;
    if (!Directory(dir).existsSync()) {
      if (mounted) toast(context, "Can't open that folder");
      return;
    }
    await _scan(() async => (await api.localFolderSet(path: dir, on_: true)).toInt());
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final all = _songs;
    final q = _filter.toLowerCase();
    final songs = all == null
        ? null
        : sortSongs(
            q.isEmpty ? all : all.where((s) => s.title.toLowerCase().contains(q) || s.artist.toLowerCase().contains(q) || (s.album ?? '').toLowerCase().contains(q)).toList(),
            _sort,
          );
    return Scaffold(
      appBar: AppBar(
        title: const Text('On This Device'),
        actions: [
          if (_folders.isNotEmpty)
            IconButton(tooltip: 'Rescan', icon: const Icon(Icons.refresh_rounded), onPressed: _scanning ? null : () => _scan(() async => (await api.localRescan()).toInt())),
          PopupMenuButton<String>(
            tooltip: 'Sort',
            icon: const Icon(Icons.sort_rounded),
            initialValue: _sort,
            onSelected: (v) => setState(() => _sort = v),
            itemBuilder: (_) => const [
              PopupMenuItem(value: 'artist', child: Text('Artist')),
              PopupMenuItem(value: 'title', child: Text('Title')),
            ],
          ),
        ],
      ),
      body: songs == null
          ? const Center(child: CircularProgressIndicator())
          : ListView(padding: const EdgeInsets.only(bottom: 24), children: [
              if (_scanning) const LinearProgressIndicator(minHeight: 2),
              // The folders, each removable, and "Add a folder".
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
                child: Wrap(spacing: 8, runSpacing: 8, children: [
                  for (final f in _folders)
                    InputChip(
                      avatar: const Icon(Icons.folder_rounded, size: 18),
                      label: Text(f.split(Platform.pathSeparator).where((x) => x.isNotEmpty).lastOrNull ?? f),
                      tooltip: f,
                      onDeleted: _scanning ? null : () => _scan(() async => (await api.localFolderSet(path: f, on_: false)).toInt()),
                    ),
                  ActionChip(avatar: const Icon(Icons.create_new_folder_rounded, size: 18), label: const Text('Add a folder'), onPressed: _scanning ? null : _addFolder),
                ]),
              ),
              if (all!.isEmpty)
                EmptyState(icon: Icons.folder_open_rounded, text: _folders.isEmpty ? 'Add a folder with your music (MP3, M4A, FLAC, OGG, OPUS, WAV…)' : 'No music files in these folders')
              else ...[
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                  child: TextField(
                    decoration: InputDecoration(hintText: 'Filter ${all.length} songs', prefixIcon: const Icon(Icons.filter_list_rounded), isDense: true),
                    onChanged: (v) => setState(() => _filter = v.trim()),
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 10, 16, 8),
                  child: Row(children: [
                    Expanded(child: FilledButton.icon(onPressed: songs.isEmpty ? null : () => p.playList(songs, from: _from), icon: const Icon(Icons.play_arrow_rounded), label: const Text('Play'))),
                    const SizedBox(width: 12),
                    Expanded(child: OutlinedButton.icon(onPressed: songs.isEmpty ? null : () => p.playList(songs, shuffle: true, from: _from), icon: const Icon(Icons.shuffle_rounded), label: const Text('Shuffle'))),
                  ]),
                ),
                for (var i = 0; i < songs.length; i++) ...[
                  SongTile(song: songs[i], player: p, subtitle: [songs[i].artist, if (songs[i].album != null) songs[i].album!].join(' · '), onPlay: () => p.playList(songs.sublist(i), from: _from)),
                  Divider(indent: 78, color: tok.separator),
                ],
              ],
            ]),
    );
  }
}
