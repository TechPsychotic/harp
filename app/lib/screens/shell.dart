import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../theme/glass.dart';
import '../theme/logo.dart';
import '../widgets/cover.dart';
import 'home.dart';
import 'library.dart';
import 'now_playing.dart';
import 'search.dart';
import 'settings_screen.dart';

/// Adaptive shell: a side rail on wide windows, a docked bar on phones. The
/// dock (mini player + navigation) is attached to the bottom edge.
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
    (Icons.home_rounded, 'Home'),
    (Icons.search_rounded, 'Search'),
    (Icons.library_music_rounded, 'Library'),
    (Icons.tune_rounded, 'Settings'),
  ];

  void _openNowPlaying() => Navigator.of(context).push(MaterialPageRoute(builder: (_) => NowPlayingScreen(player: widget.player)));

  @override
  Widget build(BuildContext context) {
    final p = widget.player;
    return ListenableBuilder(
      listenable: p,
      builder: (context, _) {
        final accent = HarpColors.accentFor(p.current?.id);
        return AuroraBackground(
          accent: accent,
          active: p.playing && p.settings.fullEffects,
          flat: p.settings.flatBackground,
          level: p.settings.reactiveBackground && p.settings.fullEffects ? p.viz.level : null,
          child: LayoutBuilder(builder: (context, c) {
            final wide = c.maxWidth >= 760;
            final content = IndexedStack(index: _tab, children: _pages);
            return Scaffold(
              body: SafeArea(
                bottom: false,
                child: Column(children: [
                  Expanded(
                    child: wide
                        ? Row(children: [
                            _Rail(tab: _tab, onTap: (i) => setState(() => _tab = i)),
                            Expanded(child: content),
                          ])
                        : content,
                  ),
                  _Dock(player: p, blur: p.settings.fullEffects, onOpen: _openNowPlaying, nav: wide ? null : _NavBar(tab: _tab, onTap: (i) => setState(() => _tab = i))),
                ]),
              ),
            );
          }),
        );
      },
    );
  }
}

class _Rail extends StatelessWidget {
  const _Rail({required this.tab, required this.onTap});
  final int tab;
  final ValueChanged<int> onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 12, 0, 12),
      child: GlassCard(
        padding: const EdgeInsets.symmetric(vertical: 16, horizontal: 6),
        child: SizedBox(
          width: 80,
          child: Column(children: [
            const HarpLogo(size: 40),
            const SizedBox(height: 22),
            for (var i = 0; i < _ShellState._items.length; i++)
              _NavItem(icon: _ShellState._items[i].$1, label: _ShellState._items[i].$2, selected: tab == i, onTap: () => onTap(i)),
          ]),
        ),
      ),
    );
  }
}

class _NavBar extends StatelessWidget {
  const _NavBar({required this.tab, required this.onTap});
  final int tab;
  final ValueChanged<int> onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(mainAxisAlignment: MainAxisAlignment.spaceAround, children: [
        for (var i = 0; i < _ShellState._items.length; i++)
          _NavItem(icon: _ShellState._items[i].$1, label: _ShellState._items[i].$2, selected: tab == i, onTap: () => onTap(i)),
      ]),
    );
  }
}

class _NavItem extends StatelessWidget {
  const _NavItem({required this.icon, required this.label, required this.selected, required this.onTap});
  final IconData icon;
  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    final fg = selected ? tok.text : tok.textDim;
    return InkWell(
      borderRadius: BorderRadius.circular(Radii.control),
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
        decoration: BoxDecoration(borderRadius: BorderRadius.circular(Radii.control), color: selected ? accent.withValues(alpha: 0.22) : Colors.transparent),
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Icon(icon, color: fg),
          const SizedBox(height: 2),
          Text(label, style: TextStyle(fontSize: 11, color: fg)),
        ]),
      ),
    );
  }
}

/// The bottom dock: the mini player, then the navigation (on phones). One
/// frosted surface attached to the bottom edge of the window.
class _Dock extends StatelessWidget {
  const _Dock({required this.player, required this.onOpen, this.nav, this.blur = true});
  final PlayerController player;
  final bool blur;
  final VoidCallback onOpen;
  final Widget? nav;

  @override
  Widget build(BuildContext context) {
    final s = player.current;
    if (s == null && nav == null) return const SizedBox.shrink();
    return GlassSurface(
      topBorderOnly: true,
      blur: blur ? 18 : 0,
      child: SafeArea(
        top: false,
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          if (s != null)
            InkWell(
              onTap: onOpen,
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
                  child: Row(children: [
                    Cover(url: s.thumbnail, seed: s.id, size: 42, radius: Radii.cover),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                        Text(s.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600)),
                        Text(s.artist, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: context.tok.textDim, fontSize: 12.5)),
                      ]),
                    ),
                    GlassIconButton(
                      icon: s.liked ? Icons.favorite_rounded : Icons.favorite_border_rounded,
                      color: s.liked ? const Color(0xFFFF6B9A) : null,
                      size: 38,
                      onPressed: player.toggleLike,
                    ),
                    const SizedBox(width: 8),
                    GlassIconButton(
                      icon: player.loading ? Icons.hourglass_top_rounded : (player.playing ? Icons.pause_rounded : Icons.play_arrow_rounded),
                      size: 42,
                      filled: true,
                      round: true,
                      onPressed: player.togglePlay,
                    ),
                    const SizedBox(width: 8),
                    GlassIconButton(icon: Icons.skip_next_rounded, size: 38, onPressed: player.next),
                  ]),
                ),
                // Only the thin progress bar rebuilds on every tick.
                ValueListenableBuilder<Duration>(
                  valueListenable: player.position,
                  builder: (_, pos, _) => ValueListenableBuilder<Duration>(
                    valueListenable: player.duration,
                    builder: (_, dur, _) => LinearProgressIndicator(
                      value: dur.inMilliseconds == 0 ? 0 : (pos.inMilliseconds / dur.inMilliseconds).clamp(0.0, 1.0),
                      minHeight: 2,
                      backgroundColor: Colors.transparent,
                    ),
                  ),
                ),
              ]),
            ),
          ?nav,
        ]),
      ),
    );
  }
}
