import 'dart:async';
import 'dart:convert';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../native.dart';
import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import 'cover.dart';
import 'playlist_art.dart';

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
  // A playlist you chose "Download All" for keeps itself downloaded.
  final auto = (await api.playlists()).where((p) => p.id == id).firstOrNull?.autoDownload ?? false;
  final todo = auto ? songs.where((s) => !s.downloaded).toList() : const <api.SongView>[];
  if (context.mounted) {
    toast(context, added == 0 ? 'Already in that playlist' : (todo.isEmpty ? 'Added $added to playlist' : 'Added $added to playlist · downloading'));
  }
  for (final s in todo) {
    await player.download(s);
  }
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
      onSelected: (v) => runSongAction(context, player, song, v, onRemove: onRemoveFromList),
      itemBuilder: (_) => [
        // Not for the song that's playing: it can't come next.
        if (player.current?.id != song.id) ...[
          _item('next', Icons.playlist_play_rounded, 'Play next'),
          _item('add', Icons.queue_music_rounded, 'Add to queue'),
        ],
        _item('playlist', Icons.playlist_add_rounded, 'Add to playlist…'),
        _item('like', song.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded, song.liked ? 'Remove from liked' : 'Like'),
        // Your own files: nothing to download or share a link to.
        if (!song.id.startsWith('local:')) ...[
          if (song.downloaded)
            _item('undownload', Icons.delete_outline_rounded, 'Remove download')
          else
            _item('download', Icons.download_rounded, 'Download'),
          _item('share', Icons.share_rounded, 'Share link'),
        ],
        if (onRemoveFromList != null) _item('remove', Icons.remove_circle_outline_rounded, removeLabel),
      ],
    );
  }

  PopupMenuItem<String> _item(String v, IconData i, String label) =>
      PopupMenuItem(value: v, child: ListTile(leading: Icon(i), title: Text(label), dense: true, contentPadding: EdgeInsets.zero));
}

/// Runs a song action by name (shared by the ⋮ menu and the long-press sheet).
Future<void> runSongAction(BuildContext context, PlayerController player, api.SongView song, String action, {VoidCallback? onRemove}) async {
  switch (action) {
    case 'next':
      await player.playNext(song);
      if (context.mounted) toast(context, 'Playing next');
    case 'add':
      await player.addToQueue(song);
      if (context.mounted) toast(context, 'Added to queue');
    case 'playlist':
      await showSavedInSheet(context, player, song);
    case 'like':
      await player.toggleLike(song);
    case 'download':
      final err = await player.download(song);
      if (context.mounted) toast(context, err ?? 'Downloaded “${song.title}”');
    case 'undownload':
      await player.removeDownload(song);
    case 'share':
      // A YouTube Music link opens for anyone, in any app or the browser.
      final link = 'https://music.youtube.com/watch?v=${song.id}';
      if (!await Native.share('${song.title} · ${song.artist}\n$link')) {
        await Clipboard.setData(ClipboardData(text: link));
        if (context.mounted) toast(context, 'Link copied');
      }
    case 'suggest':
      await player.party.suggest(song);
      if (context.mounted) toast(context, 'Suggested to the party');
    case 'remove':
      onRemove?.call();
  }
}

/// A bottom sheet of actions for a song: opened by a long press anywhere a song shows.
Future<void> showSongSheet(BuildContext context, PlayerController player, api.SongView song, {VoidCallback? onRemove, String removeLabel = 'Remove'}) {
  final tok = context.tok;
  return showModalBottomSheet<void>(
    context: context,
    useRootNavigator: true,
    backgroundColor: Colors.transparent,
    isScrollControlled: true,
    builder: (ctx) {
      Widget item(String action, IconData icon, String label) => ListTile(
            leading: Icon(icon),
            title: Text(label),
            onTap: () {
              Navigator.pop(ctx);
              runSongAction(context, player, song, action, onRemove: onRemove);
            },
          );
      return GlassSurface(
        radius: Radii.sheet,
        blur: player.settings.fullEffects ? 18 : 0,
        child: SafeArea(
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: Row(children: [
                Cover(url: song.thumbnail, seed: song.id, size: 52),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 16)),
                    Text(song.artist, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim)),
                  ]),
                ),
              ]),
            ),
            const Divider(height: 1),
            if (player.current?.id != song.id) ...[
              item('next', Icons.playlist_play_rounded, 'Play next'),
              item('add', Icons.queue_music_rounded, 'Add to queue'),
            ],
            item('playlist', Icons.playlist_add_rounded, 'Add to playlist…'),
            item('like', song.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded, song.liked ? 'Remove from liked' : 'Like'),
            if (!song.id.startsWith('local:')) ...[
              if (song.downloaded) item('undownload', Icons.delete_outline_rounded, 'Remove download') else item('download', Icons.download_rounded, 'Download'),
              item('share', Icons.share_rounded, 'Share link'),
            ],
            if (player.party.role == 'guest' && !song.id.startsWith('local:')) item('suggest', Icons.groups_rounded, 'Suggest to the party'),
            if (onRemove != null) item('remove', Icons.remove_circle_outline_rounded, removeLabel),
            const SizedBox(height: 8),
          ]),
        ),
      );
    },
  );
}

/// Asks where to save an exported playlist and writes it there.
Future<void> saveExport(BuildContext context, api.ExportFile f) async {
  final mime = f.fileName.endsWith('.csv') ? 'text/csv' : (f.fileName.endsWith('.json') ? 'application/json' : 'audio/x-mpegurl');
  try {
    final where = await FilePicker.saveFile(fileName: f.fileName, bytes: Uint8List.fromList(utf8.encode(f.contents)), mimeType: mime, dialogTitle: 'Save ${f.fileName}');
    if (context.mounted) toast(context, where == null ? 'Not saved' : 'Saved ${f.fileName}');
  } catch (e) {
    if (context.mounted) toast(context, "Couldn't save: $e");
  }
}

/// Lets you pick CSV (Exportify columns), M3U or JSON.
Future<String?> pickExportFormat(BuildContext context) => showModalBottomSheet<String>(
      context: context,
      useRootNavigator: true,
      builder: (ctx) => SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          const Padding(padding: EdgeInsets.fromLTRB(16, 16, 16, 8), child: Text('Export as', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700))),
          ListTile(leading: const Icon(Icons.table_chart_rounded), title: const Text('CSV'), subtitle: const Text('Exportify columns: opens in spreadsheets, imports anywhere'), onTap: () => Navigator.pop(ctx, 'csv')),
          ListTile(leading: const Icon(Icons.queue_music_rounded), title: const Text('M3U'), subtitle: const Text('A playlist file most players understand'), onTap: () => Navigator.pop(ctx, 'm3u')),
          ListTile(leading: const Icon(Icons.data_object_rounded), title: const Text('JSON'), subtitle: const Text('Like Spotify’s own data export'), onTap: () => Navigator.pop(ctx, 'json')),
          const SizedBox(height: 8),
        ]),
      ),
    );


/// "Saved in": every playlist with a checkbox, ticked where the song already
/// is. Tick to add, untick to take it out; or make a new playlist with it.
Future<void> showSavedInSheet(BuildContext context, PlayerController player, api.SongView song) async {
  final lists = await api.playlists();
  final inside = (await api.playlistsContaining(songId: song.id)).map((e) => e.toInt()).toSet();
  if (!context.mounted) return;
  await showModalBottomSheet<void>(
    context: context,
    useRootNavigator: true,
    isScrollControlled: true,
    builder: (ctx) => StatefulBuilder(
      builder: (ctx, setSheet) {
        final tok = ctx.tok;
        Future<void> toggle(api.PlaylistView pl, bool on) async {
          if (on) {
            await api.playlistAdd(id: pl.id, songs: [song]);
            inside.add(pl.id);
            // A playlist you chose "Download All" for keeps itself downloaded.
            if (pl.autoDownload && !song.downloaded) unawaited(player.download(song));
          } else {
            await api.playlistRemoveSong(id: pl.id, songId: song.id);
            inside.remove(pl.id);
          }
          setSheet(() {});
          player.libraryRev.value++;
        }
        return SafeArea(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(ctx).height * 0.75),
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
                child: Row(children: [
                  Cover(url: song.thumbnail, seed: song.id, size: 44),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Text(inside.isEmpty ? 'Add to playlist' : 'Saved in ${inside.length} ${inside.length == 1 ? 'playlist' : 'playlists'}',
                          style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 17)),
                      Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim)),
                    ]),
                  ),
                ]),
              ),
              ListTile(
                leading: const Icon(Icons.add_rounded),
                title: const Text('New playlist'),
                onTap: () async {
                  final name = await promptText(ctx, title: 'New playlist', hint: 'Name');
                  if (name == null || name.trim().isEmpty) return;
                  final id = await api.playlistCreate(name: name);
                  await api.playlistAdd(id: id, songs: [song]);
                  player.libraryRev.value++;
                  if (ctx.mounted) Navigator.pop(ctx);
                  if (context.mounted) toast(context, 'Added to “${name.trim()}”');
                },
              ),
              const Divider(height: 1),
              Flexible(
                child: ListView(shrinkWrap: true, children: [
                  for (final pl in lists)
                    CheckboxListTile(
                      value: inside.contains(pl.id),
                      onChanged: (v) => toggle(pl, v ?? false),
                      secondary: PlaylistArt(covers: pl.covers, seed: 'pl${pl.id}', size: 40, radius: Radii.cover),
                      title: Text(pl.name, maxLines: 1, overflow: TextOverflow.ellipsis),
                      subtitle: Text('${pl.count} songs', style: TextStyle(color: tok.textDim, fontSize: 12.5)),
                    ),
                ]),
              ),
              const SizedBox(height: 8),
            ]),
          ),
        );
      },
    ),
  );
}
