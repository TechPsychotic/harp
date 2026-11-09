import 'dart:ui';

import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
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
        // Always the dark look here, like music apps' players.
        data: harpTheme(accent, Brightness.dark),
        child: ListenableBuilder(
          listenable: player,
          builder: (context, _) {
            final s = player.current;
            return Scaffold(
              backgroundColor: Colors.black,
              body: s == null
                  ? const Center(child: Text('Nothing playing'))
                  : Stack(children: [
                      Positioned.fill(child: _Backdrop(song: s, accent: accent, blurArt: !player.settings.flatBackground)),
                      if (player.settings.fullEffects && player.settings.reactiveBackground)
                        Positioned.fill(child: ReactiveWaves(color: Colors.white, level: player.viz.level, active: player.playing)),
                      SafeArea(child: LayoutBuilder(builder: (context, c) => _body(context, s, c))),
                    ]),
            );
          },
        ),
      ),
    );
  }

  Widget _body(BuildContext context, api.SongView s, BoxConstraints c) {
    final wide = c.maxWidth > 820;
    final artSide = wide ? 380.0 : (c.maxWidth - 48).clamp(200, 420).toDouble();
    final art = _lyrics
        ? SizedBox(width: wide ? 460 : double.infinity, height: wide ? 460 : c.maxHeight * 0.5, child: LyricsPane(player: player, song: s))
        : Center(
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
          );
    final controls = _Controls(player: player, song: s, lyricsOn: _lyrics, onLyrics: () => setState(() => _lyrics = !_lyrics), onArtist: () => _openArtist(s));
    final top = Row(children: [
      IconButton(icon: const Icon(Icons.keyboard_arrow_down_rounded, size: 32, color: Colors.white), onPressed: () => Navigator.of(context).pop()),
      const Spacer(),
      Container(width: 36, height: 5, decoration: BoxDecoration(color: Colors.white38, borderRadius: BorderRadius.circular(3))),
      const Spacer(),
      IconButton(icon: const Icon(Icons.more_horiz_rounded, color: Colors.white), onPressed: () => showSongSheet(context, player, s)),
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

/// A deep wash of the artwork's colour with the artwork itself blurred behind.
/// Drawn once per song (it does not animate).
class _Backdrop extends StatelessWidget {
  const _Backdrop({required this.song, required this.accent, required this.blurArt});
  final api.SongView song;
  final Color accent;
  final bool blurArt;

  @override
  Widget build(BuildContext context) {
    final hsl = HSLColor.fromColor(accent);
    final topC = hsl.withLightness((hsl.lightness * 0.55).clamp(0.12, 0.34)).toColor();
    final bottom = hsl.withLightness(0.06).toColor();
    return RepaintBoundary(
      child: Stack(fit: StackFit.expand, children: [
        DecoratedBox(decoration: BoxDecoration(gradient: LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [topC, bottom]))),
        if (blurArt)
          Opacity(
            opacity: 0.45,
            child: ImageFiltered(
              imageFilter: ImageFilter.blur(sigmaX: 60, sigmaY: 60, tileMode: TileMode.decal),
              child: FittedBox(fit: BoxFit.cover, child: Cover(url: song.thumbnail, seed: song.id, size: 200, radius: 0)),
            ),
          ),
        const DecoratedBox(decoration: BoxDecoration(gradient: LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [Colors.transparent, Colors.black45]))),
      ]),
    );
  }
}

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
    const dim = Color(0xB3FFFFFF);
    return Column(mainAxisSize: MainAxisSize.min, children: [
      Row(children: [
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(s.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w700, color: Colors.white)),
            GestureDetector(
              onTap: onArtist,
              child: Text(s.artist, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 18, color: dim)),
            ),
          ]),
        ),
        IconButton(
          tooltip: 'Like',
          icon: Icon(s.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded, color: Colors.white, size: 26),
          onPressed: player.toggleLike,
        ),
      ]),
      if (player.currentReason != null)
        Align(
          alignment: Alignment.centerLeft,
          child: Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              const Icon(Icons.auto_awesome_rounded, size: 13, color: dim),
              const SizedBox(width: 6),
              Flexible(child: Text(player.currentReason!, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12.5, color: dim))),
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
                  activeTrackColor: Colors.white,
                  inactiveTrackColor: Colors.white24,
                  thumbColor: Colors.white,
                  overlayShape: SliderComponentShape.noOverlay,
                ),
                child: Slider(value: f, onChanged: dur == Duration.zero ? null : player.seekFraction),
              ),
              Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                Text(_fmt(pos), style: const TextStyle(color: dim, fontSize: 12)),
                Text(dur == Duration.zero ? '' : '-${_fmt(dur - pos)}', style: const TextStyle(color: dim, fontSize: 12)),
              ]),
            ]);
          },
        ),
      ),
      const SizedBox(height: 10),
      Row(mainAxisAlignment: MainAxisAlignment.spaceEvenly, children: [
        GlassIconButton(icon: Icons.fast_rewind_rounded, size: 64, color: Colors.white, onPressed: player.previous),
        player.loading
            ? const SizedBox(width: 84, height: 84, child: Padding(padding: EdgeInsets.all(26), child: CircularProgressIndicator(strokeWidth: 3, color: Colors.white)))
            : GlassIconButton(icon: player.playing ? Icons.pause_rounded : Icons.play_arrow_rounded, size: 84, color: Colors.white, onPressed: player.togglePlay),
        GlassIconButton(icon: Icons.fast_forward_rounded, size: 64, color: Colors.white, onPressed: player.next),
      ]),
      if (player.error != null)
        Padding(padding: const EdgeInsets.only(top: 6), child: Text(player.error!, textAlign: TextAlign.center, maxLines: 3, overflow: TextOverflow.ellipsis, style: const TextStyle(color: Color(0xFFFF8A8A), fontSize: 13))),
      if (player.skippedRecently != null) ...[
        const SizedBox(height: 8),
        Text('Skipped “${player.skippedRecently!.title}”. Why?', style: const TextStyle(color: dim, fontSize: 12.5)),
        const SizedBox(height: 6),
        Wrap(alignment: WrapAlignment.center, spacing: 6, runSpacing: 6, children: [
          for (final (k, l) in const [('wrong_genre', 'Wrong genre'), ('not_artist', 'Not this artist'), ('not_mood', 'Not the mood'), ('overplayed', 'Overplayed')])
            ActionChip(label: Text(l), onPressed: () => player.tagSkip(k), backgroundColor: Colors.white12, side: BorderSide.none, labelStyle: const TextStyle(color: Colors.white, fontSize: 12.5)),
        ]),
      ],
      const SizedBox(height: 10),
      Row(mainAxisAlignment: MainAxisAlignment.spaceAround, children: [
        IconButton(
          tooltip: 'Lyrics',
          icon: Icon(Icons.lyrics_rounded, color: lyricsOn ? Colors.white : dim),
          style: IconButton.styleFrom(backgroundColor: lyricsOn ? Colors.white24 : Colors.transparent),
          onPressed: onLyrics,
        ),
        IconButton(tooltip: 'Superlike: more like this, sooner', icon: Icon(Icons.auto_awesome_rounded, color: player.currentSuperliked ? Colors.white : dim), onPressed: player.superlike),
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
        IconButton(tooltip: 'Queue', icon: const Icon(Icons.format_list_bulleted_rounded, color: dim), onPressed: () => showQueueSheet(context, player)),
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
