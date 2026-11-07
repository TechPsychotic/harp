import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../theme/glass.dart';
import '../widgets/cover.dart';
import 'home.dart';
import 'library.dart';
import 'now_playing.dart';
import 'search.dart';

/// Adaptive shell: a side rail on wide windows, a bottom bar on phones.
class Shell extends StatefulWidget {
  const Shell({super.key, required this.player});
  final PlayerController player;

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  int _tab = 0;
  static const _items = [
    (Icons.home_rounded, 'Home'),
    (Icons.search_rounded, 'Search'),
    (Icons.library_music_rounded, 'Library'),
  ];

  @override
  Widget build(BuildContext context) {
    final p = widget.player;
    final pages = [HomeScreen(player: p), SearchScreen(player: p), LibraryScreen(player: p)];
    return ListenableBuilder(
      listenable: p,
      builder: (context, _) {
        final accent = HarpColors.accentFor(p.current?.id);
        return Theme(
          data: harpTheme(accent),
          child: AuroraBackground(
            accent: accent,
            child: LayoutBuilder(builder: (context, c) {
              final wide = c.maxWidth >= 760;
              final body = Stack(children: [
                IndexedStack(index: _tab, children: pages),
                Positioned(
                  left: 12,
                  right: 12,
                  bottom: wide ? 14 : 84,
                  child: _MiniPlayer(player: p, onOpen: () => Navigator.of(context).push(PageRouteBuilder(
                    pageBuilder: (_, _, _) => NowPlayingScreen(player: p),
                    transitionsBuilder: (_, a, __, child) => FadeTransition(opacity: a, child: child),
                  ))),
                ),
              ]);
              return Scaffold(
                body: wide
                    ? Row(children: [
                        SafeArea(
                          child: Padding(
                            padding: const EdgeInsets.all(12),
                            child: GlassCard(
                              radius: 28,
                              padding: const EdgeInsets.symmetric(vertical: 18, horizontal: 6),
                              child: SizedBox(
                                width: 84,
                                child: Column(children: [
                                  const Icon(Icons.graphic_eq_rounded, size: 30),
                                  const Text('Harp', style: TextStyle(fontWeight: FontWeight.w700, letterSpacing: 1.2)),
                                  const SizedBox(height: 26),
                                  for (var i = 0; i < _items.length; i++)
                                    _RailItem(icon: _items[i].$1, label: _items[i].$2, selected: _tab == i, onTap: () => setState(() => _tab = i)),
                                ]),
                              ),
                            ),
                          ),
                        ),
                        Expanded(child: SafeArea(child: body)),
                      ])
                    : SafeArea(bottom: false, child: body),
                bottomNavigationBar: wide
                    ? null
                    : Padding(
                        padding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
                        child: SafeArea(
                          top: false,
                          child: GlassCard(
                            radius: 26,
                            padding: const EdgeInsets.symmetric(vertical: 6),
                            child: Row(mainAxisAlignment: MainAxisAlignment.spaceAround, children: [
                              for (var i = 0; i < _items.length; i++)
                                _RailItem(icon: _items[i].$1, label: _items[i].$2, selected: _tab == i, onTap: () => setState(() => _tab = i)),
                            ]),
                          ),
                        ),
                      ),
              );
            }),
          ),
        );
      },
    );
  }
}

class _RailItem extends StatelessWidget {
  const _RailItem({required this.icon, required this.label, required this.selected, required this.onTap});
  final IconData icon;
  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.primary;
    return InkWell(
      borderRadius: BorderRadius.circular(18),
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(18),
          color: selected ? accent.withValues(alpha: 0.28) : Colors.transparent,
        ),
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Icon(icon, color: selected ? Colors.white : HarpColors.textDim),
          const SizedBox(height: 2),
          Text(label, style: TextStyle(fontSize: 11, color: selected ? Colors.white : HarpColors.textDim)),
        ]),
      ),
    );
  }
}

class _MiniPlayer extends StatelessWidget {
  const _MiniPlayer({required this.player, required this.onOpen});
  final PlayerController player;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    final s = player.current;
    if (s == null) return const SizedBox.shrink();
    return GlassCard(
      radius: 22,
      padding: const EdgeInsets.fromLTRB(10, 8, 10, 8),
      onTap: onOpen,
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        Row(children: [
          Cover(url: s.thumbnail, seed: s.id, size: 44, radius: 12),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(s.title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600)),
              Text(s.artist, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(color: HarpColors.textDim, fontSize: 12.5)),
            ]),
          ),
          GlassIconButton(
            icon: player.loading ? Icons.hourglass_top_rounded : (player.playing ? Icons.pause_rounded : Icons.play_arrow_rounded),
            size: 42,
            filled: true,
            onPressed: player.togglePlay,
          ),
          const SizedBox(width: 8),
          GlassIconButton(icon: Icons.skip_next_rounded, size: 42, onPressed: player.next),
        ]),
        const SizedBox(height: 8),
        ClipRRect(
          borderRadius: BorderRadius.circular(3),
          child: LinearProgressIndicator(
            value: player.progress,
            minHeight: 3,
            backgroundColor: Colors.white.withValues(alpha: 0.12),
          ),
        ),
      ]),
    );
  }
}
