import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import 'cover.dart';

String fmtDuration(int? secs) {
  if (secs == null) return '';
  final m = secs ~/ 60, s = secs % 60;
  return '$m:${s.toString().padLeft(2, '0')}';
}

/// A song row: tap plays it, the menu queues it.
class SongTile extends StatelessWidget {
  const SongTile({super.key, required this.song, required this.player, this.onPlay, this.subtitle});
  final api.SongView song;
  final PlayerController player;
  final VoidCallback? onPlay;
  final String? subtitle;

  @override
  Widget build(BuildContext context) {
    final playing = player.current?.id == song.id;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: GlassCard(
        padding: const EdgeInsets.all(10),
        radius: 18,
        blur: 16,
        tint: playing ? Theme.of(context).colorScheme.primary : null,
        onTap: onPlay ?? () => player.playSearchResult(song),
        child: Row(children: [
          Cover(url: song.thumbnail, seed: song.id, size: 52),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15)),
              const SizedBox(height: 2),
              Text(
                [song.artist, subtitle ?? fmtDuration(song.durationSecs)].where((e) => e.isNotEmpty).join(' · '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: HarpColors.textDim, fontSize: 13),
              ),
            ]),
          ),
          if (song.liked) const Padding(padding: EdgeInsets.only(right: 4), child: Icon(Icons.favorite_rounded, size: 18, color: Color(0xFFFF6B9A))),
          PopupMenuButton<String>(
            icon: const Icon(Icons.more_vert_rounded, color: HarpColors.textDim),
            color: const Color(0xEE1B1B2A),
            shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
            onSelected: (v) {
              switch (v) {
                case 'next':
                  player.playNext(song);
                case 'add':
                  player.addToQueue(song);
                case 'like':
                  player.toggleLike(song);
              }
            },
            itemBuilder: (_) => [
              const PopupMenuItem(value: 'next', child: ListTile(leading: Icon(Icons.playlist_play_rounded), title: Text('Play next'), dense: true)),
              const PopupMenuItem(value: 'add', child: ListTile(leading: Icon(Icons.queue_music_rounded), title: Text('Add to queue'), dense: true)),
              PopupMenuItem(
                value: 'like',
                child: ListTile(
                  leading: Icon(song.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded),
                  title: Text(song.liked ? 'Remove from liked' : 'Like'),
                  dense: true,
                ),
              ),
            ],
          ),
        ]),
      ),
    );
  }
}
