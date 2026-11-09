import 'dart:ui';

import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../theme/spectrum_waves.dart';
import '../widgets/cover.dart';
import '../widgets/lyrics_pane.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
import 'collection.dart';

String _fmt(Duration d) {
  final m = d.inMinutes, s = d.inSeconds % 60;
  return '$m:${s.toString().padLeft(2, '0')}';
}

/// The full-screen player: artwork-coloured backdrop, big artwork, plain controls,
/// and lyrics in place of the artwork.
class NowPlayingScreen extends StatefulWidget {
  const NowPlayingScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<NowPlayingScreen> createState() => _NowPlayingScreenState();
}

class _NowPlayingScreenState extends State<NowPlayingScreen> {
  bool _lyrics = false;

  PlayerController get player => widget.player;

  void _openArtist(api.SongView s) {
    final id = s.artistId;
    if (id == null || !id.startsWith('UC')) return;
    Navigator.of(context).push(MaterialPageRoute(
      builder: (_) => CollectionScreen(
        player: player,
        info: api.CollectionView(id: id, title: s.artist.split(RegExp(r'[,&]')).first.trim(), subtitle: 'Artist', thumbnail: '', kind: 'artist'),
      ),
    ));
  }

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<Color>(
      valueListenable: player.accent,
      builder: (context, accent, _) => Theme(
        // Follows the app's light / dark setting.
        data: harpTheme(accent, Theme.of(context).brightness),
        child: ListenableBuilder(
          listenable: player,
          builder: (context, _) {
            final s = player.current;
            return Scaffold(
              backgroundColor: context.tok.ink,
              body: s == null
                  ? const Center(child: Text('Nothing playing'))
                  : Stack(children: [
                      Positioned.fill(child: _Backdrop(song: s, accent: accent, blurArt: !player.settings.flatBackground)),
                      // Waves shaped by the live spectrum; still (but shown) with effects reduced.
                      if (player.settings.reactiveBackground && !player.settings.flatBackground)
                        Positioned.fill(
                          child: SpectrumWaves(
                            bands: player.viz.bands,
                            level: player.viz.level,
                            color: Theme.of(context).brightness == Brightness.dark ? Colors.white : HSLColor.fromColor(accent).withLightness(0.32).toColor(),
                            active: player.playing && player.settings.fullEffects,
                          ),
                        ),
                      SafeArea(
                        child: GestureDetector(
                          // Swipe down to close, like a music app's player.
                          onVerticalDragEnd: (d) {
                            if ((d.primaryVelocity ?? 0) > 700) Navigator.of(context).maybePop();
                          },
                          child: LayoutBuilder(builder: (context, c) => _body(context, s, c)),
                        ),
                      ),
                    ]),
            );
          },
        ),
      ),
    );
  }

  Widget _body(BuildContext context, api.SongView s, BoxConstraints c) {
    final fg = _fg(context);
    final wide = c.maxWidth > 820;
    final artSide = wide ? 380.0 : (c.maxWidth - 48).clamp(200, 420).toDouble();
    final art = _lyrics
        ? SizedBox(width: wide ? 460 : double.infinity, height: wide ? 460 : c.maxHeight * 0.5, child: LyricsPane(player: player, song: s))
        : GestureDetector(
            // Swipe the artwork: left for the next song, right for the previous.
            onHorizontalDragEnd: (d) {
              final v = d.primaryVelocity ?? 0;
              if (v < -400) player.next();
              if (v > 400) player.previous();
            },
            child: Center(
            child: AnimatedScale(
              scale: player.playing ? 1 : 0.86,
              duration: const Duration(milliseconds: 350),
              curve: Curves.easeOutBack,
              child: DecoratedBox(
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(12),
                  boxShadow: const [BoxShadow(color: Colors.black54, blurRadius: 40, offset: Offset(0, 18))],
                ),
                child: Hero(tag: 'np-art', child: Cover(url: s.thumbnail, seed: s.id, size: artSide, radius: 12)),
              ),
            ),
          ),
          );
    final controls = _Controls(player: player, song: s, lyricsOn: _lyrics, onLyrics: () => setState(() => _lyrics = !_lyrics), onArtist: () => _openArtist(s));
    final top = Row(children: [
      IconButton(icon: Icon(Icons.keyboard_arrow_down_rounded, size: 32, color: fg), onPressed: () => Navigator.of(context).pop()),
      const Spacer(),
      IconButton(icon: Icon(Icons.more_horiz_rounded, color: fg), onPressed: () => showSongSheet(context, player, s)),
    ]);
    if (wide) {
      return Column(children: [
        Padding(padding: const EdgeInsets.fromLTRB(8, 4, 8, 0), child: top),
        Expanded(child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [art, const SizedBox(width: 56), SizedBox(width: 400, child: controls)])),
      ]);
    }
    return Column(children: [
      Padding(padding: const EdgeInsets.fromLTRB(8, 4, 8, 0), child: top),
      Expanded(child: Padding(padding: const EdgeInsets.symmetric(horizontal: 24), child: art)),
      Padding(padding: const EdgeInsets.fromLTRB(24, 8, 24, 12), child: controls),
    ]);
  }
}

/// A wash of the artwork's colour with the artwork itself blurred behind:
/// deep in dark mode, pale in light mode. Drawn once per song (no animation).
class _Backdrop extends StatelessWidget {
  const _Backdrop({required this.song, required this.accent, required this.blurArt});
  final api.SongView song;
  final Color accent;
  final bool blurArt;

  @override
  Widget build(BuildContext context) {
    final dark = Theme.of(context).brightness == Brightness.dark;
    final hsl = HSLColor.fromColor(accent);
    final topC = dark ? hsl.withLightness((hsl.lightness * 0.55).clamp(0.12, 0.34)).toColor() : hsl.withLightness(0.86).withSaturation((hsl.saturation * 0.7).clamp(0.0, 0.6)).toColor();
    final bottom = dark ? hsl.withLightness(0.06).toColor() : hsl.withLightness(0.96).withSaturation((hsl.saturation * 0.4).clamp(0.0, 0.3)).toColor();
    return RepaintBoundary(
      child: Stack(fit: StackFit.expand, children: [
        DecoratedBox(decoration: BoxDecoration(gradient: LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [topC, bottom]))),
        if (blurArt)
          Opacity(
            opacity: dark ? 0.45 : 0.30,
            child: ImageFiltered(
              imageFilter: ImageFilter.blur(sigmaX: 60, sigmaY: 60, tileMode: TileMode.decal),
              child: FittedBox(fit: BoxFit.cover, child: Cover(url: song.thumbnail, seed: song.id, size: 200, radius: 0)),
            ),
          ),
        DecoratedBox(
          decoration: BoxDecoration(
            gradient: LinearGradient(
              begin: Alignment.topCenter,
              end: Alignment.bottomCenter,
              colors: [Colors.transparent, dark ? Colors.black45 : Colors.white54],
            ),
          ),
        ),
      ]),
    );
  }
}

/// Text and icon colour on the player: white on dark, near-black on light.
Color _fg(BuildContext context) => Theme.of(context).brightness == Brightness.dark ? Colors.white : const Color(0xFF16161C);

class _Controls extends StatelessWidget {
  const _Controls({required this.player, required this.song, required this.lyricsOn, required this.onLyrics, required this.onArtist});
  final PlayerController player;
  final api.SongView song;
  final bool lyricsOn;
  final VoidCallback onLyrics;
  final VoidCallback onArtist;

  @override
  Widget build(BuildContext context) {
    final s = song;
    final fg = _fg(context);
    final dim = fg.withValues(alpha: 0.7);
    return Column(mainAxisSize: MainAxisSize.min, children: [
      Row(children: [
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(s.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 22, fontWeight: FontWeight.w700, color: fg)),
            GestureDetector(
              onTap: onArtist,
              child: Text(s.artist, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 18, color: dim)),
            ),
          ]),
        ),
        // Tap to like; hold to superlike (counts four times, and seeds picks first).
        // (No tooltip: on phones a tooltip also takes the long press.)
        InkResponse(
          radius: 24,
          onTap: player.toggleLike,
          onLongPress: () {
            player.superlike();
            toast(context, 'Superliked: more like this, sooner');
          },
          child: Padding(
            padding: const EdgeInsets.all(10),
            child: Icon(
              player.currentSuperliked ? Icons.auto_awesome_rounded : (s.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded),
              color: fg,
              size: 26,
            ),
          ),
        ),
      ]),
      if (player.currentReason != null)
        Align(
          alignment: Alignment.centerLeft,
          child: Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              Icon(Icons.auto_awesome_rounded, size: 13, color: dim),
              const SizedBox(width: 6),
              Flexible(child: Text(player.currentReason!, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12.5, color: dim))),
            ]),
          ),
        ),
      const SizedBox(height: 8),
      ValueListenableBuilder<Duration>(
        valueListenable: player.position,
        builder: (_, pos, _) => ValueListenableBuilder<Duration>(
          valueListenable: player.duration,
          builder: (_, dur, _) {
            final f = dur.inMilliseconds == 0 ? 0.0 : (pos.inMilliseconds / dur.inMilliseconds).clamp(0.0, 1.0);
            return Column(children: [
              SliderTheme(
                data: SliderTheme.of(context).copyWith(
                  activeTrackColor: fg,
                  inactiveTrackColor: fg.withValues(alpha: 0.16),
                  thumbColor: fg,
                  overlayShape: SliderComponentShape.noOverlay,
                ),
                child: Slider(value: f, onChanged: dur == Duration.zero ? null : player.seekFraction),
              ),
              Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                Text(_fmt(pos), style: TextStyle(color: dim, fontSize: 12)),
                Text(dur == Duration.zero ? '' : '-${_fmt(dur - pos)}', style: TextStyle(color: dim, fontSize: 12)),
              ]),
            ]);
          },
        ),
      ),
      const SizedBox(height: 10),
      Row(mainAxisAlignment: MainAxisAlignment.spaceEvenly, children: [
        GlassIconButton(icon: Icons.fast_rewind_rounded, size: 64, color: fg, onPressed: player.previous),
        player.loading
            ? SizedBox(width: 84, height: 84, child: Padding(padding: EdgeInsets.all(26), child: CircularProgressIndicator(strokeWidth: 3, color: fg)))
            : GlassIconButton(icon: player.playing ? Icons.pause_rounded : Icons.play_arrow_rounded, size: 84, color: fg, onPressed: player.togglePlay),
        GlassIconButton(icon: Icons.fast_forward_rounded, size: 64, color: fg, onPressed: player.next),
      ]),
      if (player.error != null)
        Padding(padding: const EdgeInsets.only(top: 6), child: Text(player.error!, textAlign: TextAlign.center, maxLines: 3, overflow: TextOverflow.ellipsis, style: const TextStyle(color: Color(0xFFFF8A8A), fontSize: 13))),
      if (player.skippedRecently != null) ...[
        const SizedBox(height: 8),
        Text('Skipped “${player.skippedRecently!.title}”. Why?', style: TextStyle(color: dim, fontSize: 12.5)),
        const SizedBox(height: 6),
        Wrap(alignment: WrapAlignment.center, spacing: 6, runSpacing: 6, children: [
          for (final (k, l) in const [('wrong_genre', 'Wrong genre'), ('not_artist', 'Not this artist'), ('not_mood', 'Not the mood'), ('overplayed', 'Overplayed')])
            ActionChip(label: Text(l), onPressed: () => player.tagSkip(k), backgroundColor: fg.withValues(alpha: 0.10), side: BorderSide.none, labelStyle: TextStyle(color: fg, fontSize: 12.5)),
        ]),
      ],
      const SizedBox(height: 10),
      Row(mainAxisAlignment: MainAxisAlignment.spaceAround, children: [
        IconButton(
          tooltip: 'Lyrics',
          icon: Icon(Icons.lyrics_rounded, color: lyricsOn ? fg : dim),
          style: IconButton.styleFrom(backgroundColor: lyricsOn ? fg.withValues(alpha: 0.16) : Colors.transparent),
          onPressed: onLyrics,
        ),
        IconButton(tooltip: 'Add to playlist', icon: Icon(Icons.playlist_add_rounded, color: dim), onPressed: () => addToPlaylistDialog(context, player, [s])),
        ValueListenableBuilder<Set<String>>(
          valueListenable: player.downloading,
          builder: (context, busy, _) => IconButton(
            tooltip: s.downloaded ? 'Downloaded' : 'Download',
            icon: Icon(busy.contains(s.id) ? Icons.downloading_rounded : (s.downloaded ? Icons.download_done_rounded : Icons.download_rounded), color: dim),
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
        ),
        ListenableBuilder(
          listenable: player.settings,
          builder: (context, _) {
            final sp = player.settings.speed;
            return TextButton(
              onPressed: () => showSpeedSheet(context, player),
              style: TextButton.styleFrom(foregroundColor: sp == 1 ? dim : fg, backgroundColor: sp == 1 ? Colors.transparent : fg.withValues(alpha: 0.16), minimumSize: Size(44, 40)),
              child: Text(speedLabel(sp), style: const TextStyle(fontWeight: FontWeight.w700)),
            );
          },
        ),
        IconButton(
          tooltip: player.sleepAfterSong
              ? 'Sleep: after this song'
              : player.sleepAt != null
                  ? 'Sleep in ${player.sleepAt!.difference(DateTime.now()).inMinutes + 1} min'
                  : 'Sleep timer',
          icon: Icon(Icons.bedtime_rounded, color: player.sleepAt != null || player.sleepAfterSong ? fg : dim),
          style: IconButton.styleFrom(backgroundColor: player.sleepAt != null || player.sleepAfterSong ? fg.withValues(alpha: 0.16) : Colors.transparent),
          onPressed: () => showSleepSheet(context, player),
        ),
        IconButton(tooltip: 'Queue', icon: Icon(Icons.format_list_bulleted_rounded, color: dim), onPressed: () => showQueueSheet(context, player)),
      ]),
    ]);
  }
}

/// The queue, with shuffle and repeat at the top. Tap a song to jump to it.
void showQueueSheet(BuildContext context, PlayerController player) {
  showModalBottomSheet(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    shape: const RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(Radii.sheet))),
    builder: (_) => DraggableScrollableSheet(
      expand: false,
      initialChildSize: 0.75,
      maxChildSize: 0.95,
      builder: (context, scroll) => ListenableBuilder(
        listenable: player,
        builder: (context, _) {
          final q = player.queue;
          final tok = context.tok;
          final accent = Theme.of(context).colorScheme.primary;
          return ListView(controller: scroll, padding: const EdgeInsets.only(bottom: 24), children: [
            Center(child: Container(margin: const EdgeInsets.only(top: 8), width: 36, height: 5, decoration: BoxDecoration(color: tok.textFaint, borderRadius: BorderRadius.circular(3)))),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 8, 4),
              child: Row(children: [
                const Expanded(child: Text('Playing Next', style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700))),
                IconButton(tooltip: 'Shuffle', icon: Icon(Icons.shuffle_rounded, color: accent), onPressed: player.shuffleUpcoming),
                IconButton(
                  tooltip: 'Repeat: ${q.repeat}',
                  icon: Icon(q.repeat == 'one' ? Icons.repeat_one_rounded : Icons.repeat_rounded, color: q.repeat == 'off' ? tok.textFaint : accent),
                  onPressed: player.cycleRepeat,
                ),
              ]),
            ),
            if (q.user.isEmpty && q.autoplay.isEmpty) const Padding(padding: EdgeInsets.all(24), child: EmptyState(icon: Icons.queue_music_rounded, text: 'Nothing queued')),
            for (var i = 0; i < q.user.length; i++)
              SongTile(
                song: q.user[i],
                player: player,
                onPlay: () => player.jumpTo('user', i),
                trailing: IconButton(tooltip: 'Remove', icon: Icon(Icons.remove_circle_outline_rounded, color: tok.textDim), onPressed: () => player.removeFromQueue(i)),
              ),
            if (q.autoplay.isNotEmpty) ...[
              const SectionHeader('Autoplay', subtitle: 'Picked from your taste'),
              for (var i = 0; i < q.autoplay.length; i++)
                SongTile(song: q.autoplay[i], player: player, onPlay: () => player.jumpTo('auto', i), trailing: const SizedBox(width: 8)),
            ],
          ]);
        },
      ),
    ),
  );
}

/// Sleep timer: pause after a while, or at the end of this song.
void showSleepSheet(BuildContext context, PlayerController player) {
  showModalBottomSheet<void>(
    context: context,
    builder: (ctx) {
      Widget opt(String label, VoidCallback f) => ListTile(title: Text(label), onTap: () { f(); Navigator.pop(ctx); });
      final on = player.sleepAt != null || player.sleepAfterSong;
      return SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          const Padding(padding: EdgeInsets.fromLTRB(16, 16, 16, 4), child: Text('Sleep Timer', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700))),
          for (final m in const [15, 30, 45, 60]) opt('$m minutes', () => player.setSleep(Duration(minutes: m))),
          opt('End of this song', player.setSleepAfterSong),
          if (on) opt('Turn off', () => player.setSleep(null)),
          const SizedBox(height: 8),
        ]),
      );
    },
  );
}

String speedLabel(double v) => '${v.toStringAsFixed(v == v.roundToDouble() ? 0 : 2).replaceFirst(RegExp(r'0$'), '')}×';

/// Playback speed, for podcasts and long talks. The pitch stays the same.
void showSpeedSheet(BuildContext context, PlayerController player) {
  showModalBottomSheet<void>(
    context: context,
    builder: (ctx) => SafeArea(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        const Padding(padding: EdgeInsets.fromLTRB(16, 16, 16, 4), child: Text('Playback Speed', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700))),
        for (final v in const [0.75, 1.0, 1.25, 1.5, 1.75, 2.0])
          ListTile(
            title: Text(v == 1 ? '1× (normal)' : speedLabel(v)),
            trailing: player.settings.speed == v ? Icon(Icons.check_rounded, color: Theme.of(ctx).colorScheme.primary) : null,
            onTap: () {
              player.settings.setSpeed(v);
              Navigator.pop(ctx);
            },
          ),
        const SizedBox(height: 8),
      ]),
    ),
  );
}
