import 'package:flutter/material.dart';

import '../theme/glass.dart';

/// Album art with a graceful placeholder.
class Cover extends StatelessWidget {
  const Cover({super.key, required this.url, required this.seed, this.size = 56, this.radius = 14});
  final String? url;
  final String seed;
  final double size;
  final double radius;

  @override
  Widget build(BuildContext context) {
    final accent = HarpColors.accentFor(seed);
    final placeholder = DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(colors: [accent.withValues(alpha: 0.7), accent.withValues(alpha: 0.2)]),
      ),
      child: Center(child: Icon(Icons.music_note_rounded, size: size * 0.45, color: Colors.white70)),
    );
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: SizedBox(
        width: size,
        height: size,
        child: url == null || url!.isEmpty
            ? placeholder
            : Image.network(
                url!,
                fit: BoxFit.cover,
                cacheWidth: (size * 2).round(),
                errorBuilder: (_, _, _) => placeholder,
                loadingBuilder: (_, child, p) => p == null ? child : placeholder,
              ),
      ),
    );
  }
}
