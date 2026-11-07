import 'dart:async';

import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_tile.dart';

class SearchScreen extends StatefulWidget {
  const SearchScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends State<SearchScreen> {
  final _ctl = TextEditingController();
  Timer? _debounce;
  int _gen = 0;
  List<api.SongView> _results = [];
  bool _busy = false;
  String? _error;

  bool get _offline => widget.player.settings.offline;

  void _onChanged(String q) {
    _debounce?.cancel();
    if (q.trim().length < 2) {
      setState(() { _results = []; _busy = false; _error = null; });
      return;
    }
    _debounce = Timer(const Duration(milliseconds: 450), () => _run(q));
  }

  Future<void> _run(String q) async {
    final gen = ++_gen;
    setState(() { _busy = true; _error = null; });
    try {
      final List<api.SongView> r;
      if (_offline) {
        final needle = q.trim().toLowerCase();
        r = (await api.downloads())
            .map((d) => d.song)
            .where((s) => s.title.toLowerCase().contains(needle) || s.artist.toLowerCase().contains(needle))
            .toList();
      } else {
        r = await api.search(query: q.trim());
        ThumbCache.precache(r.map((s) => s.thumbnail));
      }
      if (gen == _gen && mounted) setState(() { _results = r; _busy = false; });
    } catch (e) {
      if (gen == _gen && mounted) setState(() { _error = '$e'; _busy = false; });
    }
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _ctl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Column(children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
        child: GlassCard(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 2),
          child: Row(children: [
            Icon(Icons.search_rounded, color: tok.textDim),
            const SizedBox(width: 10),
            Expanded(
              child: TextField(
                controller: _ctl,
                onChanged: _onChanged,
                onSubmitted: _run,
                textInputAction: TextInputAction.search,
                decoration: InputDecoration(
                  hintText: _offline ? 'Search your downloads' : 'Search songs, artists',
                  border: InputBorder.none,
                  filled: false,
                ),
              ),
            ),
            if (_busy)
              const SizedBox(width: 20, height: 20, child: CircularProgressIndicator(strokeWidth: 2))
            else if (_ctl.text.isNotEmpty)
              IconButton(icon: const Icon(Icons.close_rounded), onPressed: () { _ctl.clear(); _onChanged(''); }),
          ]),
        ),
      ),
      Expanded(
        child: _error != null
            ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text(_error!, style: const TextStyle(color: Color(0xFFFF8A8A)))))
            : _results.isEmpty
                ? Center(child: Text(_busy ? '' : (_offline ? 'Offline: searching downloaded songs' : 'Type to search'), style: TextStyle(color: tok.textFaint)))
                : ListView.builder(
                    padding: const EdgeInsets.fromLTRB(16, 4, 16, 16),
                    itemCount: _results.length,
                    itemBuilder: (_, i) => SongTile(song: _results[i], player: widget.player),
                  ),
      ),
    ]);
  }
}
