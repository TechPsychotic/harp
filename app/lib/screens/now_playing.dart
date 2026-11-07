import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
import 'playlist.dart' show AuroraScaffold;

String _fmt(Duration d) {
  final m = d.inMinutes, s = d.inSeconds % 60;
  return '$m:${s.toString().padLeft(2, '0')}';
}

/// Full-screen player, opened from the dock.
class NowPlayingScreen extends StatelessWidget {
  const NowPlayingScreen({super.key, required this.player});
  final PlayerController player;

  @override
  Widget build(BuildContext context) {
    return AuroraScaffold(
      player: player,
      child: Scaffold(
        body: SafeArea(
          child: ListenableBuilder(
            listenable: player,
            builder: (context, _) {
              final s = player.current;
              if (s == null) return const Center(child: Text('Nothing playing'));
              return LayoutBuilder(builder: (context, c) {
                final wide = c.maxWidth > 820;
                final art = Cover(url: s.thumbnail, seed: s.id, size: wide ? 360 : (c.maxWidth - 64).clamp(180, 380).toDouble(), radius: 14);
                final controls = _Controls(player: player);
                return Column(children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(8, 4, 8, 0),
                    child: Row(children: [
                      IconButton(icon: const Icon(Icons.keyboard_arrow_down_rounded, size: 30), onPressed: () => Navigator.of(context).pop()),
                      const Spacer(),
                      IconButton(tooltip: 'Queue', icon: const Icon(Icons.queue_music_rounded), onPressed: () => showQueueSheet(context, player)),
                    ]),
                  ),
                  Expanded(
                    child: wide
                        ? Row(mainAxisAlignment: MainAxisAlignment.center, children: [art, const SizedBox(width: 56), SizedBox(width: 420, child: controls)])
                        : SingleChildScrollView(padding: const EdgeInsets.symmetric(horizontal: 24), child: Column(children: [art, const SizedBox(height: 24), controls])),
                  ),
                ]);
              });
            },
          ),
        ),
      ),
    );
  }
}

class _Controls extends StatelessWidget {
  const _Controls({required this.player});
  final PlayerController player;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final s = player.current!;
    final repeat = player.queue.repeat;
    return Column(mainAxisSize: MainAxisSize.min, children: [
      Align(
        alignment: Alignment.centerLeft,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(s.title, maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 24, fontWeight: FontWeight.w700)),
          const SizedBox(height: 4),
          Text(s.artist, style: TextStyle(fontSize: 16, color: tok.textDim)),
        ]),
      ),
      if (player.currentReason != null) ...[
        const SizedBox(height: 12),
        Align(
          alignment: Alignment.centerLeft,
          child: GlassCard(
            radius: Radii.chip,
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              const Icon(Icons.auto_awesome_rounded, size: 14),
              const SizedBox(width: 8),
              Flexible(child: Text(player.currentReason!, style: TextStyle(fontSize: 12.5, color: tok.textDim))),
            ]),
          ),
        ),
      ],
      const SizedBox(height: 16),
      // Only this part rebuilds as the song plays.
      ValueListenableBuilder<Duration>(
        valueListenable: player.position,
        builder: (_, pos, _) => ValueListenableBuilder<Duration>(
          valueListenable: player.duration,
          builder: (_, dur, _) {
            final f = dur.inMilliseconds == 0 ? 0.0 : (pos.inMilliseconds / dur.inMilliseconds).clamp(0.0, 1.0);
            return Column(children: [
              Slider(value: f, onChanged: dur == Duration.zero ? null : player.seekFraction),
              Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                Text(_fmt(pos), style: TextStyle(color: tok.textDim, fontSize: 12)),
                Text(_fmt(dur), style: TextStyle(color: tok.textDim, fontSize: 12)),
              ]),
            ]);
          },
        ),
      ),
      const SizedBox(height: 8),
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
          size: 68,
          filled: true,
          round: true,
          onPressed: player.togglePlay,
        ),
        GlassIconButton(icon: Icons.skip_next_rounded, size: 52, onPressed: player.next),
        GlassIconButton(icon: Icons.shuffle_rounded, tooltip: 'Shuffle up next', onPressed: player.shuffleUpcoming),
      ]),
      const SizedBox(height: 16),
      ValueListenableBuilder<Set<String>>(
        valueListenable: player.downloading,
        builder: (context, busy, _) => Row(mainAxisAlignment: MainAxisAlignment.center, children: [
          GlassIconButton(
            icon: s.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded,
            color: s.liked ? const Color(0xFFFF6B9A) : null,
            tooltip: 'Like',
            onPressed: player.toggleLike,
          ),
          const SizedBox(width: 12),
          GlassIconButton(icon: Icons.auto_awesome_rounded, filled: player.currentSuperliked, tooltip: 'Superlike: more like this, much sooner', onPressed: player.superlike),
          const SizedBox(width: 12),
          GlassIconButton(
            icon: busy.contains(s.id) ? Icons.hourglass_top_rounded : (s.downloaded ? Icons.download_done_rounded : Icons.download_rounded),
            tooltip: s.downloaded ? 'Downloaded (tap to remove)' : 'Download for offline',
            onPressed: busy.contains(s.id)
                ? null
                : () async {
                    if (s.downloaded) {
                      await player.removeDownload(s);
                    } else {
                      final err = await player.download(s);
                      if (context.mounted) toast(context, err ?? 'Downloaded');
                    }
                  },
          ),
          const SizedBox(width: 12),
          GlassIconButton(icon: Icons.playlist_add_rounded, tooltip: 'Add to playlist', onPressed: () => addToPlaylistDialog(context, player, [s])),
        ]),
      ),
      if (player.skippedRecently != null) ...[
        const SizedBox(height: 16),
        Text('Skipped “${player.skippedRecently!.title}”. Why?', style: TextStyle(color: tok.textDim, fontSize: 13)),
        const SizedBox(height: 8),
        Wrap(alignment: WrapAlignment.center, spacing: 8, runSpacing: 8, children: [
          GlassChip(label: 'Wrong genre', onTap: () => player.tagSkip('wrong_genre')),
          GlassChip(label: 'Not this artist', onTap: () => player.tagSkip('not_artist')),
          GlassChip(label: 'Not the mood', onTap: () => player.tagSkip('not_mood')),
          GlassChip(label: 'Overplayed', onTap: () => player.tagSkip('overplayed')),
        ]),
      ],
      if (player.error != null) ...[
        const SizedBox(height: 12),
        Text(player.error!, textAlign: TextAlign.center, style: const TextStyle(color: Color(0xFFFF8A8A), fontSize: 13)),
      ],
    ]);
  }
}

/// The queue: tap a song to jump to it (skipping the ones before), ✕ removes.
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
          void jump(String section, int i) {
            player.jumpTo(section, i);
          }

          return GlassSurface(
            radius: Radii.sheet,
            child: ListView(controller: scroll, padding: const EdgeInsets.fromLTRB(12, 14, 12, 32), children: [
              Row(children: [
                const Expanded(child: Text('Up next', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700))),
                if (q.user.isNotEmpty) TextButton(onPressed: player.shuffleUpcoming, child: const Text('Shuffle')),
              ]),
              if (q.user.isEmpty && q.autoplay.isEmpty)
                Padding(padding: const EdgeInsets.all(16), child: Text('Queue is empty', style: TextStyle(color: context.tok.textFaint))),
              for (var i = 0; i < q.user.length; i++)
                _QueueRow(
                  song: q.user[i],
                  onTap: () => jump('user', i),
                  trailing: IconButton(tooltip: 'Remove', icon: const Icon(Icons.close_rounded), onPressed: () => player.removeFromQueue(i)),
                ),
              if (q.autoplay.isNotEmpty) ...[
                Padding(
                  padding: const EdgeInsets.only(top: 18, bottom: 4),
                  child: Text('Autoplay · picked from your taste', style: TextStyle(color: context.tok.textDim, fontWeight: FontWeight.w600)),
                ),
                for (var i = 0; i < q.autoplay.length; i++) _QueueRow(song: q.autoplay[i], onTap: () => jump('auto', i)),
              ],
            ]),
          );
        },
      ),
    ),
  );
}

class _QueueRow extends StatelessWidget {
  const _QueueRow({required this.song, required this.onTap, this.trailing});
  final api.SongView song;
  final VoidCallback onTap;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      onTap: onTap,
      leading: Cover(url: song.thumbnail, seed: song.id, size: 44, radius: Radii.cover),
      title: Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: Text('${song.artist} · ${fmtDuration(song.durationSecs)}', maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: context.tok.textDim)),
      trailing: trailing ?? Icon(Icons.skip_next_rounded, color: context.tok.textFaint),
    );
  }
}
