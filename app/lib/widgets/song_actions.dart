import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';

void toast(BuildContext context, String message) {
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(message), behavior: SnackBarBehavior.floating, duration: const Duration(seconds: 3)));
}

/// Choose a playlist (or make one) and add the song(s).
Future<void> addToPlaylistDialog(BuildContext context, PlayerController player, List<api.SongView> songs) async {
  final lists = await api.playlists();
  if (!context.mounted) return;
  final choice = await showDialog<int>(
    context: context,
    builder: (ctx) => SimpleDialog(
      title: const Text('Add to playlist'),
      children: [
        SimpleDialogOption(
          onPressed: () => Navigator.pop(ctx, -1),
          child: const Row(children: [Icon(Icons.add_rounded), SizedBox(width: 12), Text('New playlist')]),
        ),
        for (final p in lists)
          SimpleDialogOption(
            onPressed: () => Navigator.pop(ctx, p.id),
            child: Row(children: [
              const Icon(Icons.queue_music_rounded),
              const SizedBox(width: 12),
              Expanded(child: Text(p.name, overflow: TextOverflow.ellipsis)),
              Text('${p.count}', style: TextStyle(color: ctx.tok.textDim)),
            ]),
          ),
      ],
    ),
  );
  if (choice == null || !context.mounted) return;
  var id = choice;
  if (choice == -1) {
    final name = await promptText(context, title: 'New playlist', hint: 'Name');
    if (name == null || !context.mounted) return;
    id = await api.playlistCreate(name: name);
  }
  final added = await api.playlistAdd(id: id, songs: songs);
  player.libraryRev.value++;
  if (context.mounted) toast(context, added == 0 ? 'Already in that playlist' : 'Added $added to playlist');
}

Future<String?> promptText(BuildContext context, {required String title, String hint = '', String initial = ''}) {
  final c = TextEditingController(text: initial);
  return showDialog<String>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text(title),
      content: TextField(controller: c, autofocus: true, decoration: InputDecoration(hintText: hint), onSubmitted: (v) => Navigator.pop(ctx, v)),
      actions: [
        TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Cancel')),
        FilledButton(onPressed: () => Navigator.pop(ctx, c.text), child: const Text('OK')),
      ],
    ),
  );
}

/// The "more" menu of a song row.
class SongMenu extends StatelessWidget {
  const SongMenu({super.key, required this.song, required this.player, this.onRemoveFromList, this.removeLabel = 'Remove'});
  final api.SongView song;
  final PlayerController player;
  final VoidCallback? onRemoveFromList;
  final String removeLabel;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return PopupMenuButton<String>(
      icon: Icon(Icons.more_vert_rounded, color: tok.textDim),
      color: context.isDark ? const Color(0xF21B1B2A) : const Color(0xF2FFFFFF),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Radii.tile)),
      onSelected: (v) async {
        switch (v) {
          case 'next':
            await player.playNext(song);
          case 'add':
            await player.addToQueue(song);
          case 'playlist':
            await addToPlaylistDialog(context, player, [song]);
          case 'like':
            await player.toggleLike(song);
          case 'download':
            final err = await player.download(song);
            if (context.mounted) toast(context, err ?? 'Downloaded “${song.title}”');
          case 'undownload':
            await player.removeDownload(song);
          case 'remove':
            onRemoveFromList?.call();
        }
      },
      itemBuilder: (_) => [
        _item('next', Icons.playlist_play_rounded, 'Play next'),
        _item('add', Icons.queue_music_rounded, 'Add to queue'),
        _item('playlist', Icons.playlist_add_rounded, 'Add to playlist…'),
        _item('like', song.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded, song.liked ? 'Remove from liked' : 'Like'),
        if (song.downloaded)
          _item('undownload', Icons.delete_outline_rounded, 'Remove download')
        else
          _item('download', Icons.download_rounded, 'Download'),
        if (onRemoveFromList != null) _item('remove', Icons.remove_circle_outline_rounded, removeLabel),
      ],
    );
  }

  PopupMenuItem<String> _item(String v, IconData i, String label) =>
      PopupMenuItem(value: v, child: ListTile(leading: Icon(i), title: Text(label), dense: true, contentPadding: EdgeInsets.zero));
}
