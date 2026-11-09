import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import 'cover.dart';
import 'song_actions.dart';

/// A titled horizontal row of songs.
class SongShelf extends StatelessWidget {
  const SongShelf({super.key, required this.title, this.subtitle, required this.songs, required this.player, this.onPlay, this.big = false});
  final String title;
  final String? subtitle;
  final List<api.SongView> songs;
  final PlayerController player;
  final void Function(api.SongView)? onPlay;

  /// Larger cards (top picks).
  final bool big;

  @override
  Widget build(BuildContext context) {
    if (songs.isEmpty) return const SizedBox.shrink();
    final side = big ? 200.0 : 150.0;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      SectionHeader(title, subtitle: subtitle),
      SizedBox(
        height: side + 52,
        child: ListView.builder(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 16),
          itemCount: songs.length,
          itemExtent: side + 14,
          itemBuilder: (_, i) => SongCard(
            song: songs[i],
            size: side,
            onTap: () => (onPlay ?? player.playSong)(songs[i]),
            onLongPress: () => showSongSheet(context, player, songs[i]),
          ),
        ),
      ),
    ]);
  }
}

/// Kept for screens that build their own headers.
class ShelfHeader extends StatelessWidget {
  const ShelfHeader({super.key, required this.title, this.subtitle});
  final String title;
  final String? subtitle;

  @override
  Widget build(BuildContext context) => SectionHeader(title, subtitle: subtitle);
}

class SongCard extends StatelessWidget {
  const SongCard({super.key, required this.song, required this.onTap, this.onLongPress, this.size = 150});
  final api.SongView song;
  final VoidCallback onTap;
  final VoidCallback? onLongPress;
  final double size;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return GestureDetector(
      onTap: onTap,
      onLongPress: onLongPress,
      child: Padding(
        padding: const EdgeInsets.only(right: 14),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Cover(url: song.thumbnail, seed: song.id, size: size, radius: Radii.tile),
          const SizedBox(height: 7),
          Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w500, fontSize: 14)),
          Text(song.artist, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 13)),
        ]),
      ),
    );
  }
}

/// An album, artist or podcast card.
class CollectionCard extends StatelessWidget {
  const CollectionCard({super.key, required this.c, required this.onTap, this.size = 150});
  final api.CollectionView c;
  final VoidCallback onTap;
  final double size;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final round = c.kind == 'artist';
    return GestureDetector(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.only(right: 14),
        child: SizedBox(
          width: size,
          child: Column(crossAxisAlignment: round ? CrossAxisAlignment.center : CrossAxisAlignment.start, children: [
            Cover(url: c.thumbnail, seed: c.id, size: size, radius: Radii.tile, round: round),
            const SizedBox(height: 7),
            Text(c.title, maxLines: 1, overflow: TextOverflow.ellipsis, textAlign: round ? TextAlign.center : TextAlign.start, style: const TextStyle(fontWeight: FontWeight.w500, fontSize: 14)),
            if (!round && c.subtitle.isNotEmpty)
              Text(c.subtitle, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 13)),
          ]),
        ),
      ),
    );
  }
}

/// A titled horizontal row of albums / artists / podcasts.
class CollectionShelf extends StatelessWidget {
  const CollectionShelf({super.key, required this.title, required this.items, required this.onOpen, this.size = 150});
  final String title;
  final List<api.CollectionView> items;
  final void Function(api.CollectionView) onOpen;
  final double size;

  @override
  Widget build(BuildContext context) {
    if (items.isEmpty) return const SizedBox.shrink();
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      SectionHeader(title),
      SizedBox(
        height: size + 52,
        child: ListView.builder(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 16),
          itemCount: items.length,
          itemExtent: size + 14,
          itemBuilder: (_, i) => CollectionCard(c: items[i], size: size, onTap: () => onOpen(items[i])),
        ),
      ),
    ]);
  }
}
