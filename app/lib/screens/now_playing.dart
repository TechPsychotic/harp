import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_tile.dart';

String _fmt(Duration d) {
  final m = d.inMinutes, s = d.inSeconds % 60;
  return '$m:${s.toString().padLeft(2, '0')}';
}

/// Full-screen player, opened from the mini player.
class NowPlayingScreen extends StatelessWidget {
  const NowPlayingScreen({super.key, required this.player});
  final PlayerController player;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: player,
      builder: (context, _) {
        final s = player.current;
        final accent = HarpColors.accentFor(s?.id);
        return Theme(
          data: harpTheme(accent),
          child: Scaffold(
            body: AuroraBackground(
              accent: accent,
              child: SafeArea(
                child: s == null
                    ? const Center(child: Text('Nothing playing'))
                    : LayoutBuilder(builder: (context, c) {
                        final wide = c.maxWidth > 820;
                        final art = Cover(url: s.thumbnail, seed: s.id, size: wide ? 360 : (c.maxWidth - 64).clamp(180, 380), radius: 28);
                        final controls = _Controls(player: player);
                        return Column(children: [
                          Padding(
                            padding: const EdgeInsets.fromLTRB(8, 4, 8, 0),
                            child: Row(children: [
                              IconButton(icon: const Icon(Icons.keyboard_arrow_down_rounded, size: 30), onPressed: () => Navigator.of(context).pop()),
                              const Spacer(),
                              IconButton(icon: const Icon(Icons.queue_music_rounded), onPressed: () => showQueueSheet(context, player)),
                            ]),
                          ),
                          Expanded(
                            child: wide
                                ? Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                                    art,
                                    const SizedBox(width: 56),
                                    SizedBox(width: 420, child: controls),
                                  ])
                                : SingleChildScrollView(
                                    padding: const EdgeInsets.symmetric(horizontal: 24),
                                    child: Column(children: [art, const SizedBox(height: 24), controls]),
                                  ),
                          ),
                        ]);
                      }),
              ),
            ),
          ),
        );
      },
    );
  }
}

class _Controls extends StatelessWidget {
  const _Controls({required this.player});
  final PlayerController player;

  @override
  Widget build(BuildContext context) {
    final s = player.current!;
    final repeat = player.queue.repeat;
    return Column(mainAxisSize: MainAxisSize.min, children: [
      Align(
        alignment: Alignment.centerLeft,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(s.title, maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w700)),
          const SizedBox(height: 4),
          Text(s.artist, style: const TextStyle(fontSize: 16, color: HarpColors.textDim)),
        ]),
      ),
      if (player.currentReason != null) ...[
        const SizedBox(height: 14),
        Align(
          alignment: Alignment.centerLeft,
          child: GlassCard(
            radius: 16,
            blur: 12,
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              const Icon(Icons.auto_awesome_rounded, size: 14),
              const SizedBox(width: 8),
              Flexible(child: Text(player.currentReason!, style: const TextStyle(fontSize: 12.5, color: HarpColors.textDim))),
            ]),
          ),
        ),
      ],
      const SizedBox(height: 18),
      Slider(value: player.progress, onChanged: player.duration == Duration.zero ? null : player.seekFraction),
      Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
        Text(_fmt(player.position), style: const TextStyle(color: HarpColors.textDim, fontSize: 12)),
        Text(_fmt(player.duration), style: const TextStyle(color: HarpColors.textDim, fontSize: 12)),
      ]),
      const SizedBox(height: 10),
      Row(mainAxisAlignment: MainAxisAlignment.spaceEvenly, children: [
        GlassIconButton(
          icon: repeat == 'one' ? Icons.repeat_one_rounded : Icons.repeat_rounded,
          color: repeat == 'off' ? null : Theme.of(context).colorScheme.primary,
          tooltip: 'Repeat: $repeat',
          onPressed: player.cycleRepeat,
        ),
        GlassIconButton(icon: Icons.skip_previous_rounded, size: 52, onPressed: player.previous),
        GlassIconButton(
          icon: player.loading ? Icons.hourglass_top_rounded : (player.playing ? Icons.pause_rounded : Icons.play_arrow_rounded),
          size: 72,
          filled: true,
          onPressed: player.togglePlay,
        ),
        GlassIconButton(icon: Icons.skip_next_rounded, size: 52, onPressed: player.next),
        GlassIconButton(icon: Icons.shuffle_rounded, tooltip: 'Shuffle up next', onPressed: player.shuffleUpcoming),
      ]),
      const SizedBox(height: 18),
      Row(mainAxisAlignment: MainAxisAlignment.center, children: [
        GlassIconButton(
          icon: s.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded,
          color: s.liked ? const Color(0xFFFF6B9A) : null,
          tooltip: 'Like',
          onPressed: player.toggleLike,
        ),
        const SizedBox(width: 14),
        GlassIconButton(
          icon: Icons.auto_awesome_rounded,
          filled: player.currentSuperliked,
          tooltip: 'Superlike: more like this, much sooner',
          onPressed: player.superlike,
        ),
      ]),
      if (player.skippedRecently != null) ...[
        const SizedBox(height: 18),
        Text('Skipped “${player.skippedRecently!.title}”. Why?', style: const TextStyle(color: HarpColors.textDim, fontSize: 13)),
        const SizedBox(height: 8),
        Wrap(alignment: WrapAlignment.center, spacing: 8, runSpacing: 8, children: [
          GlassChip(label: 'Wrong genre', onTap: () => player.tagSkip('wrong_genre')),
          GlassChip(label: 'Not this artist', onTap: () => player.tagSkip('not_artist')),
          GlassChip(label: 'Not the mood', onTap: () => player.tagSkip('not_mood')),
          GlassChip(label: 'Overplayed', onTap: () => player.tagSkip('overplayed')),
        ]),
      ],
      if (player.error != null) ...[
        const SizedBox(height: 14),
        Text(player.error!, textAlign: TextAlign.center, style: const TextStyle(color: Color(0xFFFF8A8A), fontSize: 13)),
      ],
    ]);
  }
}

void showQueueSheet(BuildContext context, PlayerController player) {
  showModalBottomSheet(
    context: context,
    backgroundColor: Colors.transparent,
    isScrollControlled: true,
    builder: (_) => DraggableScrollableSheet(
      expand: false,
      initialChildSize: 0.7,
      maxChildSize: 0.95,
      builder: (context, scroll) => ListenableBuilder(
        listenable: player,
        builder: (context, _) {
          final q = player.queue;
          return GlassCard(
            radius: 28,
            blur: 30,
            padding: EdgeInsets.zero,
            child: ListView(controller: scroll, padding: const EdgeInsets.fromLTRB(16, 16, 16, 32), children: [
              const Text('Up next', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
              const SizedBox(height: 8),
              if (q.user.isEmpty && q.autoplay.isEmpty)
                const Padding(padding: EdgeInsets.all(16), child: Text('Queue is empty', style: TextStyle(color: HarpColors.textFaint))),
              for (var i = 0; i < q.user.length; i++)
                _QueueRow(song: q.user[i], trailing: IconButton(icon: const Icon(Icons.close_rounded), onPressed: () => player.removeFromQueue(i))),
              if (q.autoplay.isNotEmpty) ...[
                const Padding(
                  padding: EdgeInsets.only(top: 18, bottom: 6),
                  child: Text('Autoplay · picked from your taste', style: TextStyle(color: HarpColors.textDim, fontWeight: FontWeight.w600)),
                ),
                for (final s in q.autoplay) _QueueRow(song: s),
              ],
            ]),
          );
        },
      ),
    ),
  );
}

class _QueueRow extends StatelessWidget {
  const _QueueRow({required this.song, this.trailing});
  final dynamic song;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: Cover(url: song.thumbnail as String, seed: song.id as String, size: 44, radius: 10),
      title: Text(song.title as String, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: Text('${song.artist} · ${fmtDuration(song.durationSecs as int?)}', maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(color: HarpColors.textDim)),
      trailing: trailing,
    );
  }
}
