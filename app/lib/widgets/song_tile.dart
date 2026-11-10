import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import 'cover.dart';
import 'song_actions.dart';

String fmtDuration(int? secs) {
  if (secs == null) return '';
  final m = secs ~/ 60, s = secs % 60;
  return '$m:${s.toString().padLeft(2, '0')}';
}

/// A song row, as in a music app's lists: artwork, title, artist, and a "…" menu.
/// Tap plays, long-press opens the actions.
class SongTile extends StatelessWidget {
  const SongTile({
    super.key,
    required this.song,
    required this.player,
    this.onPlay,
    this.subtitle,
    this.onRemoveFromList,
    this.removeLabel = 'Remove',
    this.trailing,
    this.index,
    this.showArt = true,
  });
  final api.SongView song;
  final PlayerController player;
  final VoidCallback? onPlay;
  final String? subtitle;
  final VoidCallback? onRemoveFromList;
  final String removeLabel;
  final Widget? trailing;

  /// A track number instead of artwork (album pages).
  final int? index;
  final bool showArt;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    final playing = player.current?.id == song.id;
    final sub = [song.artist, subtitle ?? fmtDuration(song.durationSecs)].where((e) => e.isNotEmpty).join(' · ');
    return InkWell(
      onTap: onPlay ?? () => player.playSong(song),
      onLongPress: () => showSongSheet(context, player, song, onRemove: onRemoveFromList, removeLabel: removeLabel),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 7),
        child: Row(children: [
          if (index != null)
            SizedBox(
              width: 28,
              child: playing
                  ? Icon(Icons.graphic_eq_rounded, size: 18, color: accent)
                  : Text('$index', textAlign: TextAlign.center, style: TextStyle(color: tok.textDim, fontSize: 15)),
            )
          else if (showArt)
            Stack(children: [
              Cover(url: song.thumbnail, seed: song.id, size: 50),
              if (playing)
                Positioned.fill(
                  child: DecoratedBox(
                    decoration: BoxDecoration(color: Colors.black45, borderRadius: BorderRadius.circular(Radii.cover)),
                    child: const Icon(Icons.graphic_eq_rounded, color: Colors.white, size: 22),
                  ),
                ),
            ]),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 16, color: playing ? accent : tok.text)),
              const SizedBox(height: 2),
              Row(children: [
                // Downloaded: a filled badge (not the hollow outline).
                if (song.downloaded) Padding(padding: const EdgeInsets.only(right: 4), child: Icon(Icons.download_for_offline_rounded, size: 15, color: accent)),
                Expanded(child: Text(sub, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 13.5))),
              ]),
            ]),
          ),
          if (song.liked) Padding(padding: const EdgeInsets.only(left: 6), child: Icon(Icons.favorite_rounded, size: 15, color: accent)),
          trailing ?? SongMenu(song: song, player: player, onRemoveFromList: onRemoveFromList, removeLabel: removeLabel),
        ]),
      ),
    );
  }
}
