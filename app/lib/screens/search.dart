import 'dart:async';

import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
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
      final r = await api.search(query: q.trim());
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
    return Column(children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(20, 24, 20, 8),
        child: GlassCard(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 2),
          radius: 30,
          child: Row(children: [
            const Icon(Icons.search_rounded, color: HarpColors.textDim),
            const SizedBox(width: 10),
            Expanded(
              child: TextField(
                controller: _ctl,
                onChanged: _onChanged,
                onSubmitted: _run,
                textInputAction: TextInputAction.search,
                decoration: const InputDecoration(
                  hintText: 'Search songs, artists',
                  border: InputBorder.none,
                  filled: false,
                ),
              ),
            ),
            if (_busy) const SizedBox(width: 20, height: 20, child: CircularProgressIndicator(strokeWidth: 2))
            else if (_ctl.text.isNotEmpty)
              IconButton(
                icon: const Icon(Icons.close_rounded),
                onPressed: () { _ctl.clear(); _onChanged(''); },
              ),
          ]),
        ),
      ),
      Expanded(
        child: _error != null
            ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text(_error!, style: const TextStyle(color: Color(0xFFFF8A8A)))))
            : _results.isEmpty
                ? Center(child: Text(_busy ? '' : 'Type to search', style: const TextStyle(color: HarpColors.textFaint)))
                : ListView.builder(
                    padding: const EdgeInsets.fromLTRB(20, 4, 20, 140),
                    itemCount: _results.length,
                    itemBuilder: (_, i) => SongTile(song: _results[i], player: widget.player),
                  ),
      ),
    ]);
  }
}
