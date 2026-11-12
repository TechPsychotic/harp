import 'dart:convert';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
import '../widgets/song_actions.dart';
import 'playlist.dart' show AuroraScaffold;
import 'spotify_screen.dart';

/// Bring playlists over from Spotify, Apple Music, Amazon Music or YouTube.
class ImportScreen extends StatefulWidget {
  const ImportScreen({super.key, required this.player});
  final PlayerController player;

  @override
  State<ImportScreen> createState() => _ImportScreenState();
}

enum _Step { source, pick, matching, review }

class _Row {
  _Row(this.track, this.m);
  final api.ImportTrackView track;
  api.MatchView? m;
  api.SongView? chosen; // user's choice, else the match
  bool skip = false;
  api.SongView? get song => skip ? null : (chosen ?? m?.best);
}

class _ImportScreenState extends State<ImportScreen> {
  final _link = TextEditingController();
  final _paste = TextEditingController();
  _Step _step = _Step.source;
  String? _error;
  bool _busy = false;
  List<api.ImportPlaylistView> _found = [];
  api.ImportPlaylistView? _chosen;
  List<_Row> _rows = [];
  int _done = 0;
  bool _cancel = false;
  String? _notice; // e.g. Spotify cut the list at 100
  final _name = TextEditingController();

  PlayerController get p => widget.player;

  Future<void> _fromLink() async {
    setState(() { _busy = true; _error = null; });
    try {
      _found = await api.importLink(url: _link.text);
      _afterFound();
    } catch (e) {
      _error = '$e';
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _fromFile() async {
    final files = await FilePicker.pickFiles();
    if (files.isEmpty) return;
    final f = files.first;
    setState(() { _busy = true; _error = null; });
    try {
      final text = utf8.decode(await File(f.path!).readAsBytes(), allowMalformed: true);
      final name = f.name.replaceAll(RegExp(r'\.[^.]+$'), '');
      _found = await api.importParse(text: text, nameHint: name);
      _afterFound();
    } catch (e) {
      _error = '$e';
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _fromPaste() async {
    setState(() { _busy = true; _error = null; });
    _found = await api.importParse(text: _paste.text, nameHint: 'Pasted playlist');
    _afterFound();
    if (mounted) setState(() => _busy = false);
  }

  void _afterFound() {
    if (_found.isEmpty) {
      _error = "Couldn't find any songs there.";
    } else if (_found.length == 1) {
      _start(_found.first);
    } else {
      _step = _Step.pick;
    }
  }

  Future<void> _start(api.ImportPlaylistView pl) async {
    _chosen = pl;
    _notice = pl.notice;
    _name.text = pl.name;
    _cancel = false;
    _done = 0;
    if (pl.resolved.isNotEmpty) {
      // A YouTube playlist: already playable songs, nothing to match.
      _rows = [for (final s in pl.resolved) _Row(api.ImportTrackView(title: s.title, artist: s.artist), api.MatchView(best: s, score: 1, confidence: 'confident', alternatives: const []))];
      setState(() => _step = _Step.review);
      return;
    }
    _rows = [for (final t in pl.tracks) _Row(t, null)];
    setState(() => _step = _Step.matching);
    // Four searches at a time: a thousand songs takes minutes, not a quarter of an hour.
    var next = 0;
    Future<void> worker() async {
      while (!_cancel && mounted) {
        final i = next++;
        if (i >= _rows.length) break;
        try {
          _rows[i].m = await api.importMatch(t: _rows[i].track);
        } catch (_) {
          _rows[i].m = const api.MatchView(score: 0, confidence: 'none', alternatives: []);
        }
        if (mounted) setState(() => _done++);
      }
    }

    await Future.wait(List.generate(4, (_) => worker()));
    if (mounted && !_cancel) {
      ThumbCache.precache(_rows.map((r) => r.song?.thumbnail ?? ''));
      setState(() => _step = _Step.review);
    }
  }

  Future<void> _save() async {
    final songs = [for (final r in _rows) if (r.song != null) r.song!];
    final id = await api.playlistCreate(name: _name.text);
    await api.playlistAdd(id: id, songs: songs);
    p.libraryRev.value++;
    if (!mounted) return;
    toast(context, 'Imported ${songs.length} songs');
    Navigator.pop(context);
  }

  @override
  void dispose() {
    _cancel = true;
    _link.dispose();
    _paste.dispose();
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return AuroraScaffold(
      player: p,
      child: Scaffold(
        appBar: AppBar(backgroundColor: Colors.transparent, title: const Text('Import playlist')),
        body: switch (_step) {
          _Step.source => ListView(padding: const EdgeInsets.fromLTRB(16, 0, 16, 40), children: [
              Text('Bring your playlists over. Nothing is uploaded and no account is needed.', style: TextStyle(color: tok.textDim)),
              const SizedBox(height: 16),
              _Card(
                icon: Icons.account_circle_rounded,
                title: 'From your Spotify account',
                subtitle: 'Every playlist and Liked Songs, no 100-song limit (like Exportify)',
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton.icon(
                    onPressed: _busy
                        ? null
                        : () async {
                            final pl = await Navigator.of(context).push<api.ImportPlaylistView>(MaterialPageRoute(builder: (_) => const SpotifyScreen()));
                            if (pl == null || !mounted) return;
                            setState(() { _found = [pl]; _afterFound(); });
                          },
                    icon: const Icon(Icons.login_rounded),
                    label: const Text('Open Spotify'),
                  ),
                ),
              ),
              _Card(
                icon: Icons.link_rounded,
                title: 'From a link',
                subtitle: 'A public playlist link: Spotify (first 100 songs), Apple Music and others',
                child: Row(children: [
                  Expanded(child: TextField(controller: _link, decoration: const InputDecoration(hintText: 'https://open.spotify.com/playlist/…'))),
                  const SizedBox(width: 8),
                  FilledButton(onPressed: _busy ? null : _fromLink, child: const Text('Read')),
                ]),
              ),
              _Card(
                icon: Icons.upload_file_rounded,
                title: 'From a file',
                subtitle: 'Spotify data export (JSON) or Exportify CSV, Apple Music export (text), M3U',
                child: Align(alignment: Alignment.centerLeft, child: OutlinedButton.icon(onPressed: _busy ? null : _fromFile, icon: const Icon(Icons.folder_open_rounded), label: const Text('Choose file'))),
              ),
              _Card(
                icon: Icons.content_paste_rounded,
                title: 'Paste a list',
                subtitle: 'One song per line, “Artist - Title”. Works for Amazon Music and anything else',
                child: Column(children: [
                  TextField(controller: _paste, minLines: 4, maxLines: 8, decoration: const InputDecoration(hintText: 'Daft Punk - Get Lucky\nThe Weeknd - Blinding Lights')),
                  const SizedBox(height: 8),
                  Align(alignment: Alignment.centerRight, child: FilledButton(onPressed: _busy ? null : _fromPaste, child: const Text('Read list'))),
                ]),
              ),
              if (_busy) const Padding(padding: EdgeInsets.all(16), child: Center(child: CircularProgressIndicator())),
              if (_error != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(_error!, style: const TextStyle(color: Color(0xFFFF8A8A)))),
              const SizedBox(height: 8),
              Text('Spotify: Share › Copy link (public playlists), or connect your account above for everything. Apple Music: Share › Copy Link, or File › Library › Export Playlist. Amazon Music shares nothing with other apps, so paste its song list.', style: TextStyle(color: tok.textFaint, fontSize: 12.5)),
            ]),
          _Step.pick => ListView(padding: const EdgeInsets.all(16), children: [
              const Text('Which playlist?', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
              for (final pl in _found)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: GlassCard(onTap: () => _start(pl), child: Row(children: [Expanded(child: Text(pl.name)), Text('${pl.tracks.length} songs', style: TextStyle(color: tok.textDim))])),
                ),
            ]),
          _Step.matching => Center(
              child: Padding(
                padding: const EdgeInsets.all(32),
                child: Column(mainAxisSize: MainAxisSize.min, children: [
                  Text('Finding the songs of “${_chosen!.name}”…', textAlign: TextAlign.center),
                  const SizedBox(height: 16),
                  LinearProgressIndicator(value: _rows.isEmpty ? null : _done / _rows.length),
                  const SizedBox(height: 8),
                  Text('$_done of ${_rows.length}', style: TextStyle(color: tok.textDim)),
                  if (_notice != null) Padding(padding: const EdgeInsets.only(top: 12), child: Text(_notice!, textAlign: TextAlign.center, style: TextStyle(color: tok.textDim, fontSize: 12.5))),
                  const SizedBox(height: 16),
                  TextButton(onPressed: () { _cancel = true; setState(() => _step = _Step.source); }, child: const Text('Cancel')),
                ]),
              ),
            ),
          _Step.review => _review(context),
        },
      ),
    );
  }

  Widget _review(BuildContext context) {
    final tok = context.tok;
    final weak = _rows.where((r) => r.m?.confidence == 'weak' && !r.skip && r.chosen == null).length;
    final none = _rows.where((r) => r.song == null).length;
    return Column(children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
        child: Column(children: [
          if (_notice != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: GlassCard(
                padding: const EdgeInsets.all(12),
                child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  const Icon(Icons.info_outline_rounded, size: 18),
                  const SizedBox(width: 10),
                  Expanded(child: Text(_notice!, style: TextStyle(color: tok.textDim, fontSize: 12.5))),
                ]),
              ),
            ),
          TextField(controller: _name, decoration: const InputDecoration(labelText: 'Playlist name')),
          const SizedBox(height: 8),
          Align(
            alignment: Alignment.centerLeft,
            child: Text('${_rows.length - none} matched · $weak to check · $none left out', style: TextStyle(color: tok.textDim)),
          ),
        ]),
      ),
      Expanded(
        child: ListView.builder(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
          itemCount: _rows.length,
          itemBuilder: (_, i) => _reviewRow(context, _rows[i]),
        ),
      ),
      SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: SizedBox(
            width: double.infinity,
            child: FilledButton.icon(
              onPressed: _rows.any((r) => r.song != null) ? _save : null,
              icon: const Icon(Icons.check_rounded),
              label: Text('Save ${_rows.where((r) => r.song != null).length} songs'),
            ),
          ),
        ),
      ),
    ]);
  }

  Widget _reviewRow(BuildContext context, _Row r) {
    final tok = context.tok;
    final s = r.song;
    final weak = r.m?.confidence == 'weak' && r.chosen == null;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: GlassCard(
        padding: const EdgeInsets.all(8),
        radius: Radii.tile,
        child: Row(children: [
          if (s != null) Cover(url: s.thumbnail, seed: s.id, size: 44) else const SizedBox(width: 44, height: 44, child: Icon(Icons.help_outline_rounded)),
          const SizedBox(width: 10),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('${r.track.artist.isEmpty ? '' : '${r.track.artist} – '}${r.track.title}', maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: tok.textDim, fontSize: 12.5)),
              Text(s == null ? (r.skip ? 'Skipped' : 'No match found') : '${s.title} · ${s.artist}', maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600)),
            ]),
          ),
          if (weak) const Padding(padding: EdgeInsets.only(right: 4), child: Icon(Icons.warning_amber_rounded, size: 18, color: Color(0xFFFFB84D))),
          PopupMenuButton<int>(
            icon: const Icon(Icons.more_vert_rounded),
            onSelected: (v) => setState(() {
              if (v == -1) {
                r.skip = !r.skip;
              } else {
                r.skip = false;
                r.chosen = r.m!.alternatives[v];
              }
            }),
            itemBuilder: (_) => [
              PopupMenuItem(value: -1, child: Text(r.skip ? 'Include' : 'Leave out')),
              if (r.m != null)
                for (var i = 0; i < r.m!.alternatives.length; i++)
                  PopupMenuItem(value: i, child: Text('Use: ${r.m!.alternatives[i].title} · ${r.m!.alternatives[i].artist}', maxLines: 1, overflow: TextOverflow.ellipsis)),
            ],
          ),
        ]),
      ),
    );
  }
}

class _Card extends StatelessWidget {
  const _Card({required this.icon, required this.title, required this.subtitle, required this.child});
  final IconData icon;
  final String title;
  final String subtitle;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: GlassCard(
        padding: const EdgeInsets.all(14),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [Icon(icon), const SizedBox(width: 10), Text(title, style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 16))]),
          Padding(padding: const EdgeInsets.fromLTRB(34, 2, 0, 10), child: Text(subtitle, style: TextStyle(color: context.tok.textDim, fontSize: 12.5))),
          child,
        ]),
      ),
    );
  }
}
