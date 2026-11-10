import 'package:flutter/material.dart';

import '../player/controller.dart';

/// The big Play button on a playlist, album or list page. When that list is
/// what's playing it becomes Pause (and Resume while paused), like other music
/// apps; otherwise it starts the list.
class ListPlayButton extends StatelessWidget {
  const ListPlayButton({super.key, required this.player, required this.kind, required this.id, required this.onPlay});
  final PlayerController player;

  /// The list's source kind and id ("playlist" / "12", "liked" / "", "album" / "MPRE…").
  final String kind;
  final String id;
  final VoidCallback? onPlay;

  bool get _isThis {
    final s = player.queue.source;
    return player.current != null && s != null && s.kind == kind && s.id == id;
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: player,
        builder: (context, _) {
          if (!_isThis) {
            return FilledButton.icon(onPressed: onPlay, icon: const Icon(Icons.play_arrow_rounded), label: const Text('Play'));
          }
          return FilledButton.icon(
            onPressed: player.togglePlay,
            icon: Icon(player.playing ? Icons.pause_rounded : Icons.play_arrow_rounded),
            label: Text(player.playing ? 'Pause' : 'Resume'),
          );
        },
      );
}
