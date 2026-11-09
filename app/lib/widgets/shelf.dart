import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import 'cover.dart';
import 'song_actions.dart';

/// A titled horizontal row of songs.
class SongShelf extends StatelessWidget {
  const SongShelf({super.key, required this.title, this.subtitle, required this.songs, required this.player, this.onPlay});
  final String title;
  final String? subtitle;
  final List<api.SongView> songs;
  final PlayerController player;
  final void Function(api.SongView)? onPlay;

  @override
  Widget build(BuildContext context) {
    if (songs.isEmpty) return const SizedBox.shrink();
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      ShelfHeader(title: title, subtitle: subtitle),
      SizedBox(
        height: 196,
        child: ListView.builder(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 16),
          itemCount: songs.length,
          itemExtent: 148,
          itemBuilder: (_, i) => SongCard(
            song: songs[i],
            onTap: () => (onPlay ?? player.playSong)(songs[i]),
            onLongPress: () => showSongSheet(context, player, songs[i]),
          ),
        ),
      ),
      const SizedBox(height: 14),
    ]);
  }
}

class ShelfHeader extends StatelessWidget {
  const ShelfHeader({super.key, required this.title, this.subtitle});
  final String title;
  final String? subtitle;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 10),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(title, style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w700)),
        if (subtitle != null) Text(subtitle!, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12.5, color: context.tok.textDim)),
      ]),
    );
  }
}

class SongCard extends StatelessWidget {
  const SongCard({super.key, required this.song, required this.onTap, this.onLongPress});
  final api.SongView song;
  final VoidCallback onTap;
  final VoidCallback? onLongPress;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return InkWell(
      borderRadius: BorderRadius.circular(Radii.tile),
      onTap: onTap,
      onLongPress: onLongPress,
      child: Padding(
        padding: const EdgeInsets.only(right: 12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Stack(children: [
            Cover(url: song.thumbnail, seed: song.id, size: 136, radius: Radii.tile),
            if (song.downloaded)
              const Positioned(right: 6, bottom: 6, child: Icon(Icons.download_done_rounded, size: 18, color: Colors.white)),
          ]),
          const SizedBox(height: 8),
          Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 13.5)),
          Text(song.artist, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 12.5)),
        ]),
      ),
    );
  }
}
