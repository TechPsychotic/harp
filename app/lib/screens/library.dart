import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_tile.dart';

class LibraryScreen extends StatefulWidget {
  const LibraryScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<LibraryScreen> createState() => _LibraryScreenState();
}

class _LibraryScreenState extends State<LibraryScreen> with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 2, vsync: this);
  List<api.SongView> _liked = [], _history = [];

  @override
  void initState() {
    super.initState();
    _load();
    widget.player.addListener(_load);
  }

  Future<void> _load() async {
    final l = await api.likedSongs();
    final h = await api.history(limit: 100);
    if (mounted) setState(() { _liked = l; _history = h; });
  }

  @override
  void dispose() {
    widget.player.removeListener(_load);
    _tabs.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    Widget list(List<api.SongView> songs, String empty) => songs.isEmpty
        ? Center(child: Text(empty, style: const TextStyle(color: HarpColors.textFaint)))
        : ListView(
            padding: const EdgeInsets.fromLTRB(20, 8, 20, 140),
            children: [for (final s in songs) SongTile(song: s, player: widget.player)],
          );
    return Column(children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(20, 24, 20, 8),
        child: Align(
          alignment: Alignment.centerLeft,
          child: Text('Library', style: Theme.of(context).textTheme.headlineMedium?.copyWith(fontWeight: FontWeight.w700)),
        ),
      ),
      TabBar(
        controller: _tabs,
        dividerColor: Colors.transparent,
        indicatorColor: Theme.of(context).colorScheme.primary,
        tabs: const [Tab(text: 'Liked'), Tab(text: 'History')],
      ),
      Expanded(
        child: TabBarView(controller: _tabs, children: [
          list(_liked, 'Heart a song to keep it here'),
          list(_history, 'Nothing played yet'),
        ]),
      ),
    ]);
  }
}
