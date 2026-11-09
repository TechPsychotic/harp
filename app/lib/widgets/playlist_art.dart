import 'package:flutter/material.dart';

import '../theme/glass.dart';
import 'cover.dart';

/// A playlist's picture: its first song's artwork, or a 2×2 mosaic once it has
/// four songs (like Apple Music and Spotify). Empty playlists get a note.
class PlaylistArt extends StatelessWidget {
  const PlaylistArt({super.key, required this.covers, required this.seed, required this.size, this.radius = Radii.tile});
  final List<String> covers;
  final String seed;
  final double size;
  final double radius;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    Widget body;
    if (covers.isEmpty) {
      body = Container(color: tok.surface, child: Icon(Icons.music_note_rounded, size: size * 0.45, color: tok.textFaint));
    } else if (covers.length < 4) {
      body = Cover(url: covers.first, seed: seed, size: size, radius: 0);
    } else {
      final h = size / 2;
      Widget c(int i) => Cover(url: covers[i], seed: '$seed$i', size: h, radius: 0);
      body = Column(children: [
        Row(children: [c(0), c(1)]),
        Row(children: [c(2), c(3)]),
      ]);
    }
    return ClipRRect(borderRadius: BorderRadius.circular(radius), child: SizedBox(width: size, height: size, child: body));
  }
}
