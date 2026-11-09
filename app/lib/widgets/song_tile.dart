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

/// A song row: tap plays it, the menu queues / downloads it.
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
  });
  final api.SongView song;
  final PlayerController player;
  final VoidCallback? onPlay;
  final String? subtitle;
  final VoidCallback? onRemoveFromList;
  final String removeLabel;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final playing = player.current?.id == song.id;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: GlassCard(
        padding: const EdgeInsets.all(8),
        radius: Radii.tile,
        tint: playing ? Theme.of(context).colorScheme.primary : null,
        onTap: onPlay ?? () => player.playSong(song),
        onLongPress: () => showSongSheet(context, player, song, onRemove: onRemoveFromList, removeLabel: removeLabel),
        child: Row(children: [
          Cover(url: song.thumbnail, seed: song.id, size: 48),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15)),
              const SizedBox(height: 2),
              Text(
                [song.artist, subtitle ?? fmtDuration(song.durationSecs)].where((e) => e.isNotEmpty).join(' · '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(color: tok.textDim, fontSize: 13),
              ),
            ]),
          ),
          if (song.downloaded) Padding(padding: const EdgeInsets.only(right: 4), child: Icon(Icons.download_done_rounded, size: 18, color: tok.textDim)),
          if (song.liked) const Padding(padding: EdgeInsets.only(right: 2), child: Icon(Icons.favorite_rounded, size: 17, color: Color(0xFFFF6B9A))),
          trailing ?? SongMenu(song: song, player: player, onRemoveFromList: onRemoveFromList, removeLabel: removeLabel),
        ]),
      ),
    );
  }
}
