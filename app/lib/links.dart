import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'player/controller.dart';
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
    }
  }
}
