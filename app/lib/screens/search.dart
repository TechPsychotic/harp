import 'dart:async';

import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/shelf.dart';
import '../widgets/song_actions.dart';
import '../widgets/song_tile.dart';
import 'browse.dart';
import 'collection.dart';

class SearchScreen extends StatefulWidget {
  const SearchScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

/// "All" is the default: everything, ranked by relevance. The others narrow it down.
const _kinds = [('all', 'All'), ('songs', 'Songs'), ('albums', 'Albums'), ('artists', 'Artists'), ('podcasts', 'Podcasts')];

class _SearchScreenState extends State<SearchScreen> {
  final _ctl = TextEditingController();
  final _focus = FocusNode();
  Timer? _debounce;
  int _gen = 0;
  String _kind = 'all';
  List<api.SearchHitView> _all = [];
  List<api.SongView> _songs = [];
  List<api.CollectionView> _collections = [];
  List<String> _history = [];
  bool _busy = false;
  String? _error;

  PlayerController get p => widget.player;
  bool get _offline => p.settings.offline;
  bool get _hasQuery => _ctl.text.trim().length >= 2;

  @override
  void initState() {
    super.initState();
    _loadHistory();
  }

  Future<void> _loadHistory() async {
    final h = await api.searchHistory(limit: 15);
    if (mounted) setState(() => _history = h);
  }

  /// Saves the search to history: only when you press search or open a result,
  /// never just because you typed.
  Future<void> _remember() async {
    final q = _ctl.text.trim();
    if (q.length < 2) return;
    await api.searchHistoryAdd(q: q);
    _loadHistory();
  }

  void _onChanged(String q) {
    _debounce?.cancel();
    setState(() {});
    if (q.trim().length < 2) {
      setState(() { _all = []; _songs = []; _collections = []; _busy = false; _error = null; });
      return;
    }
    _debounce = Timer(const Duration(milliseconds: 400), () => _run(q));
  }

  void _submit(String q) {
    _debounce?.cancel();
    _remember();
    _run(q);
  }

  Future<void> _run(String q) async {
    final gen = ++_gen;
    if (q.trim().length < 2) return;
    setState(() { _busy = true; _error = null; });
    try {
      var all = <api.SearchHitView>[];
      var songs = <api.SongView>[];
      var cols = <api.CollectionView>[];
      if (_offline) {
        final needle = q.trim().toLowerCase();
        songs = (await api.downloads())
            .map((d) => d.song)
            .where((s) => s.title.toLowerCase().contains(needle) || s.artist.toLowerCase().contains(needle))
            .toList();
      } else if (_kind == 'all') {
        all = await api.searchAll(query: q.trim());
        ThumbCache.precache(all.map((h) => h.song?.thumbnail ?? h.collection?.thumbnail ?? ''));
      } else if (_kind == 'songs') {
        songs = await api.search(query: q.trim());
        ThumbCache.precache(songs.map((s) => s.thumbnail));
      } else {
        cols = await api.searchCollections(kind: _kind, query: q.trim());
        ThumbCache.precache(cols.map((c) => c.thumbnail));
      }
      if (gen == _gen && mounted) setState(() { _all = all; _songs = songs; _collections = cols; _busy = false; });
    } catch (e) {
      if (gen == _gen && mounted) setState(() { _error = '$e'; _busy = false; });
    }
  }

  void _useHistory(String q) {
    _ctl.text = q;
    _ctl.selection = TextSelection.collapsed(offset: q.length);
    _submit(q);
  }

  void _playSong(api.SongView s) {
    _remember();
    p.playSearchResult(s);
  }

  void _open(api.CollectionView c) {
    _remember();
    Navigator.of(context).push(MaterialPageRoute(builder: (_) => CollectionScreen(player: p, info: c)));
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _ctl.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      const PageTitle('Search'),
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 4, 16, 10),
        child: Container(
          height: 44,
          decoration: BoxDecoration(color: tok.surface, borderRadius: BorderRadius.circular(Radii.control)),
          child: Row(children: [
            const SizedBox(width: 10),
            Icon(Icons.search_rounded, color: tok.textDim),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                controller: _ctl,
                focusNode: _focus,
                onChanged: _onChanged,
                onSubmitted: _submit,
                textInputAction: TextInputAction.search,
                style: const TextStyle(fontSize: 16),
                decoration: InputDecoration(
                  isCollapsed: true,
                  hintText: _offline ? 'Your downloads' : 'Artists, songs, albums, podcasts',
                  border: InputBorder.none,
                ),
              ),
            ),
            if (_busy)
              const Padding(padding: EdgeInsets.all(12), child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)))
            else if (_ctl.text.isNotEmpty)
              IconButton(icon: Icon(Icons.cancel_rounded, color: tok.textFaint, size: 20), onPressed: () { _ctl.clear(); _onChanged(''); }),
          ]),
        ),
      ),
      if (!_offline && _hasQuery)
        SizedBox(
          height: 36,
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
                      setState(() { _kind = key; _all = []; _songs = []; _collections = []; });
                      _run(_ctl.text);
                    },
                  ),
                ),
            ],
          ),
        ),
      Expanded(child: _results(tok)),
    ]);
  }

  Widget _results(HarpTokens tok) {
    if (_error != null) return EmptyState(icon: Icons.wifi_off_rounded, text: 'Search didn’t work.\n$_error');
    if (!_hasQuery) return _historyView(tok);
    final empty = _all.isEmpty && _songs.isEmpty && _collections.isEmpty;
    if (empty) return _busy ? const SizedBox.shrink() : const EmptyState(icon: Icons.search_off_rounded, text: 'No results');
    if (_offline || _kind == 'songs') {
      return ListView.builder(
        padding: const EdgeInsets.only(top: 6, bottom: 16),
        itemCount: _songs.length,
        itemBuilder: (_, i) => SongTile(song: _songs[i], player: p, onPlay: () => _playSong(_songs[i])),
      );
    }
    if (_kind != 'all') {
      return ListView.builder(
        padding: const EdgeInsets.only(top: 6, bottom: 16),
        itemCount: _collections.length,
        itemBuilder: (_, i) => _CollectionRow(c: _collections[i], onTap: () => _open(_collections[i])),
      );
    }
    return _allView(tok);
  }

  /// All results: the top one, then sections in the order YouTube Music ranked them.
  Widget _allView(HarpTokens tok) {
    final top = _all.first;
    final rest = _all.skip(1).toList();
    final songs = rest.where((h) => h.song != null).map((h) => (h.song!, h.kind)).toList();
    List<api.CollectionView> of(String kind) => rest.where((h) => h.kind == kind && h.collection != null).map((h) => h.collection!).toList();
    final artists = of('artist'), albums = of('album'), podcasts = of('podcast');
    return ListView(padding: const EdgeInsets.only(top: 6, bottom: 16), children: [
      const SectionHeader('Top Result'),
      Padding(padding: const EdgeInsets.symmetric(horizontal: 16), child: _TopResult(hit: top, player: p, onPlay: _playSong, onOpen: _open)),
      if (songs.isNotEmpty) ...[
        SectionHeader('Songs', action: 'See All', onAction: () { setState(() { _kind = 'songs'; }); _run(_ctl.text); }),
        for (final (s, kind) in songs.take(5)) SongTile(song: s, player: p, subtitle: kind == 'episode' ? 'Episode' : null, onPlay: () => _playSong(s)),
      ],
      CollectionShelf(title: 'Artists', items: artists, onOpen: _open, size: 120),
      CollectionShelf(title: 'Albums', items: albums, onOpen: _open),
      CollectionShelf(title: 'Podcasts', items: podcasts, onOpen: _open),
    ]);
  }

  Widget _historyView(HarpTokens tok) {
    if (_history.isEmpty && _offline) {
      return const EmptyState(icon: Icons.search_rounded, text: 'Offline: search your downloaded songs');
    }
    return ListView(padding: const EdgeInsets.only(top: 4, bottom: 16), children: [
      if (_history.isNotEmpty) Padding(
        padding: const EdgeInsets.fromLTRB(16, 8, 8, 0),
        child: Row(children: [
          const Expanded(child: Text('Recently Searched', style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700))),
          TextButton(onPressed: () async { await api.searchHistoryClear(); _loadHistory(); }, child: const Text('Clear')),
        ]),
      ),
      // A few recent searches, then moods, genres and charts to browse.
      for (final q in _history.take(6)) ...[
        ListTile(
          leading: Icon(Icons.history_rounded, color: tok.textDim),
          title: Text(q),
          trailing: IconButton(
            icon: Icon(Icons.close_rounded, size: 18, color: tok.textFaint),
            onPressed: () async { await api.searchHistoryRemove(q: q); _loadHistory(); },
          ),
          onTap: () => _useHistory(q),
        ),
        const Divider(indent: 56),
      ],
      BrowseGrid(player: p),
    ]);
  }
}

class _TopResult extends StatelessWidget {
  const _TopResult({required this.hit, required this.player, required this.onPlay, required this.onOpen});
  final api.SearchHitView hit;
  final PlayerController player;
  final void Function(api.SongView) onPlay;
  final void Function(api.CollectionView) onOpen;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final s = hit.song, c = hit.collection;
    final title = s?.title ?? c?.title ?? '';
    final kind = switch (hit.kind) { 'song' => 'Song', 'episode' => 'Episode', 'artist' => 'Artist', 'album' => 'Album', 'podcast' => 'Podcast', _ => '' };
    final sub = s != null ? '$kind · ${s.artist}' : (c!.kind == 'artist' ? kind : '$kind · ${c.subtitle}');
    // Hold (or ⋮) for Play next, Add to queue, Add to playlist… without playing it.
    return GestureDetector(
      onLongPress: s == null ? null : () => showSongSheet(context, player, s),
      child: GlassCard(
      padding: const EdgeInsets.fromLTRB(14, 14, 4, 14),
      onTap: () => s != null ? onPlay(s) : onOpen(c!),
      child: Row(children: [
        Cover(url: s?.thumbnail ?? c!.thumbnail, seed: s?.id ?? c!.id, size: 92, radius: Radii.tile, round: c?.kind == 'artist'),
        const SizedBox(width: 14),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title, maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w800)),
            const SizedBox(height: 4),
            Text(sub, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 14)),
          ]),
        ),
        if (s != null) ...[
          Icon(Icons.play_circle_fill_rounded, size: 44, color: Theme.of(context).colorScheme.primary),
          SongMenu(song: s, player: player),
        ],
      ]),
      ),
    );
  }
}

class _CollectionRow extends StatelessWidget {
  const _CollectionRow({required this.c, required this.onTap});
  final api.CollectionView c;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 7),
        child: Row(children: [
          Cover(url: c.thumbnail, seed: c.id, size: 56, round: c.kind == 'artist'),
          const SizedBox(width: 12),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(c.title, maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 16)),
              if (c.subtitle.isNotEmpty) Text(c.subtitle, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 13.5)),
            ]),
          ),
          Icon(Icons.chevron_right_rounded, color: tok.textFaint),
        ]),
      ),
    );
  }
}
