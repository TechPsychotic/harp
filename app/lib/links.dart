import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'player/controller.dart';
import 'screens/playlist.dart';
import 'src/rust/api/engine.dart' as api;
import 'src/rust/api/party.dart' as net;
import 'screens/party_screen.dart';
import 'screens/sync_screen.dart';

/// harp:// links (party and sync invites) open Harp at the right place.
///
/// They arrive three ways: the arguments of the first launch (Linux), the
/// intent that started the app (Android: tapped, or shared text containing a
/// link), and later while Harp runs (both).
class Links {
  static const _channel = MethodChannel('dev.harp/links');
  static final navigator = GlobalKey<NavigatorState>();
  static String? _pending;
  static PlayerController? _player;

  /// Call early, with main()'s arguments.
  static void listen(List<String> args) {
    _pending = args.where((a) => a.startsWith('harp://')).firstOrNull;
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'link' && call.arguments is String) open(call.arguments as String);
    });
  }

  /// The player is ready: open a link that arrived while starting.
  static Future<void> ready(PlayerController p) async {
    _player = p;
    if (Platform.isAndroid) {
      try {
        _pending ??= await _channel.invokeMethod<String>('initial');
      } catch (_) {}
    }
    final l = _pending;
    _pending = null;
    if (l != null) {
      // After the first frame, so the navigator exists.
      WidgetsBinding.instance.addPostFrameCallback((_) => open(l));
    }
  }

  static void open(String link) {
    final p = _player;
    final nav = navigator.currentState;
    if (p == null || nav == null) {
      _pending = link;
      return;
    }
    if (link.startsWith('harp://party/')) {
      nav.push(MaterialPageRoute(builder: (_) => PartyScreen(player: p, invite: link)));
    } else if (link.startsWith('harp://sync/')) {
      nav.push(MaterialPageRoute(builder: (_) => SyncScreen(player: p, invite: link)));
    } else if (link.startsWith('harp://playlist/')) {
      _receivePlaylist(nav, p, link);
    }
  }

  /// A friend's shared playlist: show what it is, save a copy on yes.
  static Future<void> _receivePlaylist(NavigatorState nav, PlayerController p, String link) async {
    final ctx = nav.context;
    showDialog<void>(context: ctx, barrierDismissible: false, builder: (_) => const Center(child: CircularProgressIndicator()));
    (String, List<api.SongView>)? got;
    String? err;
    try {
      got = await net.playlistPeek(link: link);
    } catch (e) {
      err = '$e';
    }
    nav.pop();
    if (!nav.mounted) return;
    if (got == null) {
      await showDialog<void>(
        context: nav.context,
        builder: (c) => AlertDialog(
          title: const Text('Couldn’t open the playlist'),
          content: Text('The person sharing it may have stopped sharing, or Harp isn’t open on their device.\n\n$err'),
          actions: [TextButton(onPressed: () => Navigator.pop(c), child: const Text('OK'))],
        ),
      );
      return;
    }
    final (name, songs) = got;
    final ok = await showDialog<bool>(
      context: nav.context,
      builder: (c) => AlertDialog(
        title: Text('Save “$name”?'),
        content: Text('A shared playlist with ${songs.length} songs. A copy goes into your library.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(c, false), child: const Text('Not now')),
          FilledButton(onPressed: () => Navigator.pop(c, true), child: const Text('Save')),
        ],
      ),
    );
    if (ok != true) return;
    final id = await api.playlistCreate(name: name);
    // Added at the top in one batch, so they keep the sharer's order.
    await api.playlistAdd(id: id, songs: songs);
    p.libraryRev.value++;
    final pl = (await api.playlists()).where((x) => x.id == id).firstOrNull;
    if (pl != null && nav.mounted) nav.push(MaterialPageRoute(builder: (_) => PlaylistScreen(player: p, playlist: pl)));
  }
}
