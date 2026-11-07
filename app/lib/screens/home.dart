import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_tile.dart';

/// Smart shuffle, genre nudges and recent plays.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  List<api.SongView> _recent = [];
  api.TasteStats? _stats;
  List<(String, String)> _genres = [];

  @override
  void initState() {
    super.initState();
    _refresh();
    widget.player.addListener(_maybeRefresh);
  }

  String? _lastId;
  void _maybeRefresh() {
    final id = widget.player.current?.id;
    if (id != _lastId) {
      _lastId = id;
      _refresh();
    }
  }

  Future<void> _refresh() async {
    final r = await api.history(limit: 12);
    final s = await api.tasteStats();
    final g = await api.genres();
    if (mounted) setState(() { _recent = r; _stats = s; _genres = g; });
  }

  @override
  void dispose() {
    widget.player.removeListener(_maybeRefresh);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.player;
    final hour = DateTime.now().hour;
    final greeting = hour < 5 ? 'Late night' : hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
    return ListView(
      padding: const EdgeInsets.fromLTRB(20, 24, 20, 140),
      children: [
        Text(greeting, style: Theme.of(context).textTheme.headlineMedium?.copyWith(fontWeight: FontWeight.w700)),
        const SizedBox(height: 4),
        const Text('Music that learns what you like, on this device only.', style: TextStyle(color: HarpColors.textDim)),
        const SizedBox(height: 20),
        if (p.resumeOffer != null) ...[
          GlassCard(
            onTap: p.resume,
            child: Row(children: [
              const Icon(Icons.history_rounded),
              const SizedBox(width: 12),
              Expanded(child: Text('Resume “${p.resumeOffer!.title}”', maxLines: 1, overflow: TextOverflow.ellipsis)),
              const Icon(Icons.play_arrow_rounded),
            ]),
          ),
          const SizedBox(height: 14),
        ],
        GlassCard(
          padding: const EdgeInsets.all(22),
          tint: Theme.of(context).colorScheme.primary,
          onTap: p.loading ? null : () => p.smartShuffle(),
          child: Row(children: [
            const Icon(Icons.auto_awesome_rounded, size: 34),
            const SizedBox(width: 16),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                const Text('Smart shuffle', style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700)),
                const SizedBox(height: 4),
                Text(
                  (_stats?.plays ?? 0) == 0
                      ? 'Starts broad. Every skip and finish teaches it your taste.'
                      : '${_stats!.plays} plays · ${_stats!.artists} artists learned',
                  style: const TextStyle(color: HarpColors.textDim),
                ),
              ]),
            ),
            if (p.loading) const SizedBox(width: 24, height: 24, child: CircularProgressIndicator(strokeWidth: 2)) else const Icon(Icons.play_circle_fill_rounded, size: 40),
          ]),
        ),
        if (p.error != null) ...[
          const SizedBox(height: 12),
          Text(p.error!, style: const TextStyle(color: Color(0xFFFF8A8A))),
        ],
        const SizedBox(height: 22),
        const Text('Steer it', style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
        const SizedBox(height: 10),
        Wrap(spacing: 8, runSpacing: 8, children: [
          for (final (key, label) in _genres)
            GlassChip(
              label: label,
              icon: key == 'more_energy' ? Icons.bolt_rounded : key == 'less_energy' ? Icons.spa_rounded : null,
              onTap: () => p.smartShuffle(genre: key),
            ),
        ]),
        if (_recent.isNotEmpty) ...[
          const SizedBox(height: 26),
          const Text('Recently played', style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
          const SizedBox(height: 8),
          for (final s in _recent) SongTile(song: s, player: p),
        ],
      ],
    );
  }
}
