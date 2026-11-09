import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';

import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_actions.dart';

/// Your Spotify account, read like Exportify does: every playlist and Liked Songs,
/// with no 100-song limit. Pops with the chosen playlist's songs.
class SpotifyScreen extends StatefulWidget {
  const SpotifyScreen({super.key});

  @override
  State<SpotifyScreen> createState() => _SpotifyScreenState();
}

enum _Stage { checking, setup, waiting, list, fetching }

class _SpotifyScreenState extends State<SpotifyScreen> {
  final _clientId = TextEditingController();
  _Stage _stage = _Stage.checking;
  String _redirect = '';
  String? _error;
  List<api.SpotifyPlaylistView> _playlists = [];
  String _fetching = '';

  @override
  void initState() {
    super.initState();
    _start();
  }

  Future<void> _start() async {
    _redirect = await api.spotifyRedirectUri();
    _clientId.text = await api.spotifyClientId() ?? '';
    if (await api.spotifyConnected()) {
      await _loadPlaylists();
    } else {
      setState(() => _stage = _Stage.setup);
    }
  }

  Future<void> _loadPlaylists() async {
    setState(() { _stage = _Stage.checking; _error = null; });
    try {
      final v = await api.spotifyPlaylists();
      setState(() { _playlists = v; _stage = _Stage.list; });
    } catch (e) {
      setState(() { _error = '$e'; _stage = _Stage.setup; });
    }
  }

  Future<void> _connect() async {
    setState(() => _error = null);
    try {
      final url = await api.spotifyConnect(clientId: _clientId.text);
      setState(() => _stage = _Stage.waiting);
      final opened = await launchUrl(Uri.parse(url), mode: LaunchMode.externalApplication);
      if (!opened) throw 'Could not open the browser.';
      await api.spotifyWait();
      await _loadPlaylists();
    } catch (e) {
      if (mounted) setState(() { _error = '$e'; _stage = _Stage.setup; });
    }
  }

  Future<void> _pick(api.SpotifyPlaylistView pl) async {
    setState(() { _stage = _Stage.fetching; _fetching = pl.name; });
    try {
      final v = await api.spotifyFetch(id: pl.id, name: pl.name);
      if (mounted) Navigator.pop(context, v);
    } catch (e) {
      if (mounted) setState(() { _error = '$e'; _stage = _Stage.list; });
    }
  }

  @override
  void dispose() {
    if (_stage == _Stage.waiting) api.spotifyCancel();
    _clientId.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Spotify'),
        actions: [
          if (_stage == _Stage.list)
            TextButton(
              onPressed: () async {
                await api.spotifyDisconnect();
                setState(() => _stage = _Stage.setup);
              },
              child: const Text('Disconnect'),
            ),
        ],
      ),
      body: switch (_stage) {
        _Stage.checking => const Center(child: CircularProgressIndicator()),
        _Stage.waiting => Center(
            child: Padding(
              padding: const EdgeInsets.all(32),
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                const CircularProgressIndicator(),
                const SizedBox(height: 18),
                const Text('Sign in to Spotify in your browser, then come back here.', textAlign: TextAlign.center, style: TextStyle(fontSize: 16)),
                const SizedBox(height: 12),
                TextButton(onPressed: () { api.spotifyCancel(); setState(() => _stage = _Stage.setup); }, child: const Text('Cancel')),
              ]),
            ),
          ),
        _Stage.fetching => Center(
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              const CircularProgressIndicator(),
              const SizedBox(height: 16),
              Text('Reading every song of “$_fetching”…', style: TextStyle(color: tok.textDim)),
            ]),
          ),
        _Stage.list => ListView(children: [
            if (_error != null) Padding(padding: const EdgeInsets.all(16), child: Text(_error!, style: const TextStyle(color: Colors.redAccent))),
            Padding(padding: const EdgeInsets.fromLTRB(16, 8, 16, 8), child: Text('Pick a playlist to bring over. All of its songs are read, however many.', style: TextStyle(color: tok.textDim))),
            for (final pl in _playlists) ...[
              ListTile(
                leading: Icon(pl.id == 'liked' ? Icons.favorite_rounded : Icons.queue_music_rounded),
                title: Text(pl.name),
                subtitle: Text('${pl.total} songs${pl.owner.isNotEmpty && pl.owner != 'you' ? ' · ${pl.owner}' : ''}', style: TextStyle(color: tok.textDim)),
                trailing: Icon(Icons.chevron_right_rounded, color: tok.textFaint),
                onTap: () => _pick(pl),
              ),
              const Divider(indent: 56),
            ],
          ]),
        _Stage.setup => ListView(padding: const EdgeInsets.all(16), children: [
            Text('Read your whole Spotify library', style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w800)),
            const SizedBox(height: 6),
            Text(
              'Like Exportify: you sign in to your own Spotify account and Harp reads every playlist and your Liked Songs, with no 100-song limit. '
              'It only reads. Your sign-in stays on this device; nothing goes through anyone else.',
              style: TextStyle(color: tok.textDim),
            ),
            const SizedBox(height: 18),
            const _Step(n: 1, text: 'Open the Spotify developer dashboard and create an app (any name and description).'),
            Padding(
              padding: const EdgeInsets.only(left: 36, bottom: 8),
              child: Align(
                alignment: Alignment.centerLeft,
                child: TextButton.icon(
                  icon: const Icon(Icons.open_in_new_rounded, size: 18),
                  label: const Text('developer.spotify.com/dashboard'),
                  onPressed: () => launchUrl(Uri.parse('https://developer.spotify.com/dashboard'), mode: LaunchMode.externalApplication),
                ),
              ),
            ),
            const _Step(n: 2, text: 'Add this Redirect URI to the app, and tick "Web API":'),
            Padding(
              padding: const EdgeInsets.only(left: 36, bottom: 12),
              child: GlassCard(
                padding: const EdgeInsets.fromLTRB(12, 4, 4, 4),
                child: Row(children: [
                  Expanded(child: SelectableText(_redirect, style: const TextStyle(fontFamily: 'monospace', fontSize: 13))),
                  IconButton(
                    tooltip: 'Copy',
                    icon: const Icon(Icons.copy_rounded, size: 18),
                    onPressed: () {
                      Clipboard.setData(ClipboardData(text: _redirect));
                      toast(context, 'Copied');
                    },
                  ),
                ]),
              ),
            ),
            const _Step(n: 3, text: 'Copy the app\'s Client ID and paste it here:'),
            Padding(
              padding: const EdgeInsets.only(left: 36),
              child: TextField(controller: _clientId, decoration: const InputDecoration(hintText: 'Client ID (32 letters and digits)')),
            ),
            const SizedBox(height: 18),
            FilledButton.icon(onPressed: _connect, icon: const Icon(Icons.login_rounded), label: const Text('Connect to Spotify')),
            if (_error != null) Padding(padding: const EdgeInsets.only(top: 12), child: Text(_error!, style: const TextStyle(color: Colors.redAccent))),
            const SizedBox(height: 16),
            Text(
              'Spotify limits personal developer apps to a few users, which is fine for your own account. '
              'Playlists you neither made nor follow may not be readable.',
              style: TextStyle(color: tok.textFaint, fontSize: 12.5),
            ),
          ]),
      },
    );
  }
}

class _Step extends StatelessWidget {
  const _Step({required this.n, required this.text});
  final int n;
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        CircleAvatar(radius: 13, backgroundColor: Theme.of(context).colorScheme.primary, child: Text('$n', style: const TextStyle(color: Colors.white, fontSize: 13, fontWeight: FontWeight.w700))),
        const SizedBox(width: 10),
        Expanded(child: Padding(padding: const EdgeInsets.only(top: 3), child: Text(text, style: const TextStyle(fontSize: 15)))),
      ]),
    );
  }
}
