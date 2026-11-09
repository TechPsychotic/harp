import 'dart:async';

import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_tile.dart';
import 'collection.dart';

class SearchScreen extends StatefulWidget {
  const SearchScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

const _kinds = [('songs', 'Songs'), ('artists', 'Artists'), ('albums', 'Albums'), ('playlists', 'Playlists'), ('podcasts', 'Podcasts')];

class _SearchScreenState extends State<SearchScreen> {
  final _ctl = TextEditingController();
  Timer? _debounce;
  int _gen = 0;
  String _kind = 'songs';
  List<api.SongView> _songs = [];
  List<api.CollectionView> _collections = [];
  List<String> _history = [];
  bool _busy = false;
  String? _error;

  bool get _offline => widget.player.settings.offline;
  bool get _hasQuery => _ctl.text.trim().length >= 2;

  @override
  void initState() {
    super.initState();
    _loadHistory();
  }

  Future<void> _loadHistory() async {
    final h = await api.searchHistory(limit: 12);
    if (mounted) setState(() => _history = h);
  }

  void _onChanged(String q) {
    _debounce?.cancel();
    if (q.trim().length < 2) {
      setState(() { _songs = []; _collections = []; _busy = false; _error = null; });
      return;
    }
    _debounce = Timer(const Duration(milliseconds: 450), () => _run(q));
  }

  Future<void> _run(String q) async {
    final gen = ++_gen;
    if (q.trim().length < 2) return;
    setState(() { _busy = true; _error = null; });
    try {
      List<api.SongView> songs = [];
      List<api.CollectionView> cols = [];
      if (_kind == 'songs') {
        if (_offline) {
          final needle = q.trim().toLowerCase();
          songs = (await api.downloads())
              .map((d) => d.song)
              .where((s) => s.title.toLowerCase().contains(needle) || s.artist.toLowerCase().contains(needle))
              .toList();
        } else {
          songs = await api.search(query: q.trim());
          ThumbCache.precache(songs.map((s) => s.thumbnail));
        }
      } else {
        cols = await api.searchCollections(kind: _kind, query: q.trim());
        ThumbCache.precache(cols.map((c) => c.thumbnail));
      }
      if (gen == _gen && mounted) {
        setState(() { _songs = songs; _collections = cols; _busy = false; });
        _loadHistory();
      }
    } catch (e) {
      if (gen == _gen && mounted) setState(() { _error = '$e'; _busy = false; });
    }
  }

  void _useHistory(String q) {
    _ctl.text = q;
    _ctl.selection = TextSelection.collapsed(offset: q.length);
    _run(q);
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
    final p = widget.player;
    final hasResults = _kind == 'songs' ? _songs.isNotEmpty : _collections.isNotEmpty;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      const PageTitle('Search'),
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
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
                  hintText: _offline ? 'Search your downloads' : 'Songs, artists, albums, playlists, podcasts',
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
      if (!_offline)
        SizedBox(
          height: 40,
          child: ListView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.symmetric(horizontal: 16),
            children: [
              for (final (key, label) in _kinds)
                Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: GlassChip(
                    label: label,
                    selected: _kind == key,
                    onTap: () {
                      setState(() { _kind = key; _songs = []; _collections = []; });
                      if (_hasQuery) _run(_ctl.text);
                    },
                  ),
                ),
            ],
          ),
        ),
      const SizedBox(height: 4),
      Expanded(
        child: _error != null
            ? Center(child: Padding(padding: const EdgeInsets.all(24), child: Text(_error!, style: const TextStyle(color: Color(0xFFFF8A8A)))))
            : !_hasQuery
                ? _historyView(tok)
                : !hasResults
                    ? (_busy ? const SizedBox.shrink() : const EmptyState(icon: Icons.search_off_rounded, text: 'No results'))
                    : _kind == 'songs'
                        ? ListView.builder(
                            padding: const EdgeInsets.fromLTRB(16, 4, 16, 16),
                            itemCount: _songs.length,
                            itemBuilder: (_, i) => SongTile(song: _songs[i], player: p),
                          )
                        : ListView.builder(
                            padding: const EdgeInsets.fromLTRB(16, 4, 16, 16),
                            itemCount: _collections.length,
                            itemBuilder: (_, i) => _CollectionRow(c: _collections[i], player: p),
                          ),
      ),
    ]);
  }

  Widget _historyView(HarpTokens tok) {
    if (_history.isEmpty) {
      return EmptyState(icon: Icons.search_rounded, text: _offline ? 'Offline: searching downloaded songs' : 'Search for songs, artists, albums, playlists and podcasts');
    }
    return ListView(padding: const EdgeInsets.fromLTRB(16, 8, 16, 16), children: [
      Row(children: [
        Expanded(child: Text('Recent searches', style: TextStyle(fontWeight: FontWeight.w700, color: tok.textDim))),
        TextButton(onPressed: () async { await api.searchHistoryClear(); _loadHistory(); }, child: const Text('Clear')),
      ]),
      for (final q in _history)
        ListTile(
          contentPadding: EdgeInsets.zero,
          leading: Icon(Icons.history_rounded, color: tok.textDim),
          title: Text(q),
          trailing: IconButton(
            icon: Icon(Icons.close_rounded, size: 18, color: tok.textFaint),
            onPressed: () async { await api.searchHistoryRemove(q: q); _loadHistory(); },
          ),
          onTap: () => _useHistory(q),
        ),
    ]);
  }
}

class _CollectionRow extends StatelessWidget {
  const _CollectionRow({required this.c, required this.player});
  final api.CollectionView c;
  final PlayerController player;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: GlassCard(
        padding: const EdgeInsets.all(8),
        radius: Radii.tile,
        onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => CollectionScreen(player: player, info: c))),
        child: Row(children: [
          Cover(url: c.thumbnail, seed: c.id, size: 56, round: c.kind == 'artist'),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(c.title, maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15)),
              const SizedBox(height: 2),
              Text(c.subtitle, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 13)),
            ]),
          ),
          Icon(Icons.chevron_right_rounded, color: tok.textFaint),
        ]),
      ),
    );
  }
}
