import 'package:flutter/material.dart';

import '../perf.dart';
import '../player/controller.dart';
import '../theme/glass.dart';
import '../theme/logo.dart';
import '../widgets/cover.dart';
import 'home.dart';
import 'library.dart';
import 'now_playing.dart';
import 'search.dart';
import 'settings_screen.dart';

/// The app frame. Phones: a floating mini player above a tab bar. Wide windows:
/// a sidebar, with the mini player along the bottom. Everything is tinted with
/// the playing song's artwork colour.
class Shell extends StatefulWidget {
  const Shell({super.key, required this.player});
  final PlayerController player;

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  int _tab = 0;
  late final List<Widget> _pages = [
    HomeScreen(player: widget.player),
    SearchScreen(player: widget.player),
    LibraryScreen(player: widget.player),
    SettingsScreen(player: widget.player, embedded: true),
  ];
  static const _items = [
    (Icons.play_circle_fill_rounded, 'Listen Now'),
    (Icons.search_rounded, 'Search'),
    (Icons.library_music_rounded, 'Library'),
    (Icons.settings_rounded, 'Settings'),
  ];

  void _openNowPlaying() => Navigator.of(context).push(MaterialPageRoute(fullscreenDialog: true, builder: (_) => NowPlayingScreen(player: widget.player)));

  @override
  Widget build(BuildContext context) {
    final p = widget.player;
    final brightness = Theme.of(context).brightness;
    return ValueListenableBuilder<Color>(
      valueListenable: p.accent,
      builder: (context, accent, _) => Theme(
        data: harpTheme(accent, brightness),
        child: Builder(builder: (context) {
          return LayoutBuilder(builder: (context, c) {
            final wide = c.maxWidth >= 760;
            final content = IndexedStack(index: _tab, children: _pages);
            final mini = _MiniPlayer(player: p, onOpen: _openNowPlaying, floating: !wide);
            return Scaffold(
              body: Stack(children: [
                AuroraBackground(
                  accent: accent,
                  flat: p.settings.flatBackground,
                  child: SafeArea(
                    bottom: false,
                    child: wide
                        ? Row(children: [
                            _Sidebar(tab: _tab, onTap: (i) => setState(() => _tab = i)),
                            VerticalDivider(width: 0.6, thickness: 0.6, color: context.tok.separator),
                            Expanded(child: Column(children: [Expanded(child: content), mini])),
                          ])
                        : content,
                  ),
                ),
                if (p.settings.perfOverlay) Positioned(top: MediaQuery.paddingOf(context).top + 4, right: 8, child: const FrameMeter()),
              ]),
              bottomNavigationBar: wide
                  ? null
                  : Column(mainAxisSize: MainAxisSize.min, children: [
                      mini,
                      _TabBar(tab: _tab, onTap: (i) => setState(() => _tab = i), blur: p.settings.fullEffects),
                    ]),
            );
          });
        }),
      ),
    );
  }
}

class _Sidebar extends StatelessWidget {
  const _Sidebar({required this.tab, required this.onTap});
  final int tab;
  final ValueChanged<int> onTap;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    return SizedBox(
      width: 220,
      child: ListView(padding: const EdgeInsets.fromLTRB(10, 16, 10, 16), children: [
        const Padding(
          padding: EdgeInsets.fromLTRB(8, 0, 8, 18),
          child: Row(children: [HarpLogo(size: 30), SizedBox(width: 10), Text('Harp', style: TextStyle(fontSize: 20, fontWeight: FontWeight.w800))]),
        ),
        for (var i = 0; i < _ShellState._items.length; i++)
          Padding(
            padding: const EdgeInsets.only(bottom: 2),
            child: Material(
              color: tab == i ? tok.surface : Colors.transparent,
              borderRadius: BorderRadius.circular(8),
              child: InkWell(
                borderRadius: BorderRadius.circular(8),
                onTap: () => onTap(i),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
                  child: Row(children: [
                    Icon(_ShellState._items[i].$1, size: 20, color: accent),
                    const SizedBox(width: 12),
                    Text(_ShellState._items[i].$2, style: TextStyle(fontSize: 15, fontWeight: tab == i ? FontWeight.w600 : FontWeight.w400)),
                  ]),
                ),
              ),
            ),
          ),
      ]),
    );
  }
}

class _TabBar extends StatelessWidget {
  const _TabBar({required this.tab, required this.onTap, required this.blur});
  final int tab;
  final ValueChanged<int> onTap;
  final bool blur;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    return GlassSurface(
      topBorderOnly: true,
      blur: blur ? 20 : 0,
      child: SafeArea(
        top: false,
        child: SizedBox(
          height: 56,
          child: Row(children: [
            for (var i = 0; i < _ShellState._items.length; i++)
              Expanded(
                child: InkResponse(
                  onTap: () => onTap(i),
                  child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
                    Icon(_ShellState._items[i].$1, size: 25, color: tab == i ? accent : tok.textFaint),
                    const SizedBox(height: 2),
                    Text(_ShellState._items[i].$2, style: TextStyle(fontSize: 10.5, fontWeight: FontWeight.w500, color: tab == i ? accent : tok.textFaint)),
                  ]),
                ),
              ),
          ]),
        ),
      ),
    );
  }
}

/// The mini player. Floats as a card above the tab bar on phones.
class _MiniPlayer extends StatelessWidget {
  const _MiniPlayer({required this.player, required this.onOpen, required this.floating});
  final PlayerController player;
  final VoidCallback onOpen;
  final bool floating;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: player,
      builder: (context, _) {
        final s = player.current;
        if (s == null) return const SizedBox.shrink();
        final tok = context.tok;
        final bar = Material(
          color: tok.raised,
          elevation: floating ? 8 : 0,
          shadowColor: Colors.black54,
          borderRadius: BorderRadius.circular(floating ? 14 : 0),
          clipBehavior: Clip.antiAlias,
          child: InkWell(
            onTap: onOpen,
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(8, 8, 4, 8),
                child: Row(children: [
                  Hero(tag: 'np-art', child: Cover(url: s.thumbnail, seed: s.id, size: 44, radius: Radii.cover)),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Text(s.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15)),
                      Text(
                        player.error != null ? 'Couldn’t play this song' : s.artist,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(color: player.error != null ? Colors.redAccent : tok.textDim, fontSize: 13),
                      ),
                    ]),
                  ),
                  if (player.loading)
                    const SizedBox(width: 44, height: 44, child: Padding(padding: EdgeInsets.all(12), child: CircularProgressIndicator(strokeWidth: 2)))
                  else
                    GlassIconButton(icon: player.playing ? Icons.pause_rounded : Icons.play_arrow_rounded, size: 44, onPressed: player.togglePlay),
                  GlassIconButton(icon: Icons.fast_forward_rounded, size: 44, onPressed: player.next),
                ]),
              ),
              ValueListenableBuilder<Duration>(
                valueListenable: player.position,
                builder: (_, pos, _) => ValueListenableBuilder<Duration>(
                  valueListenable: player.duration,
                  builder: (_, dur, _) => LinearProgressIndicator(
                    value: dur.inMilliseconds == 0 ? 0 : (pos.inMilliseconds / dur.inMilliseconds).clamp(0.0, 1.0),
                    minHeight: 2,
                    backgroundColor: Colors.transparent,
                    color: tok.text.withValues(alpha: 0.6),
                  ),
                ),
              ),
            ]),
          ),
        );
        return floating ? Padding(padding: const EdgeInsets.fromLTRB(8, 0, 8, 6), child: bar) : bar;
      },
    );
  }
}
