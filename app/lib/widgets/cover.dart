import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';

import '../theme/glass.dart';

/// Covers are fetched once into an on-device cache, so they show instantly,
/// can be precached, and work offline.
///
/// This runs in Dart on purpose: the Rust worker pool is shared with playback,
/// search and queue calls, and a screen full of covers downloading there made
/// everything else wait. File names match the Rust side (SHA-256 of the URL).
class ThumbCache {
  static Directory? _dir;
  static final Map<String, String> _paths = {};
  static final Map<String, Future<String>> _pending = {};
  static final HttpClient _http = HttpClient()..connectionTimeout = const Duration(seconds: 8);
  static int _active = 0;
  static final List<Completer<void>> _waiting = [];
  static const _maxParallel = 6;

  static Future<void> init(String dataDir) async {
    _dir = Directory('$dataDir/thumbs');
    await _dir!.create(recursive: true);
  }

  static String _file(String url) => '${_dir!.path}/${sha256.convert(utf8.encode(url)).toString().substring(0, 24)}.img';

  /// The cached file if there already is one (no waiting).
  static String? peek(String url) {
    final known = _paths[url];
    if (known != null) return known;
    if (_dir == null) return null;
    final f = File(_file(url));
    if (f.existsSync() && f.lengthSync() > 0) return _paths[url] = f.path;
    return null;
  }

  static void forgetAll() {
    _paths.clear();
  }

  static Future<String> path(String url) {
    final hit = peek(url);
    if (hit != null) return Future.value(hit);
    if (_dir == null || !url.startsWith('http')) return Future.value('');
    return _pending.putIfAbsent(url, () => _fetch(url).whenComplete(() => _pending.remove(url)));
  }

  static Future<String> _fetch(String url) async {
    if (_active >= _maxParallel) {
      final c = Completer<void>();
      _waiting.add(c);
      await c.future;
    }
    _active++;
    try {
      final req = await _http.getUrl(Uri.parse(url)).timeout(const Duration(seconds: 12));
      final resp = await req.close().timeout(const Duration(seconds: 15));
      if (resp.statusCode != 200) return '';
      final bytes = BytesBuilder(copy: false);
      await for (final chunk in resp) {
        bytes.add(chunk);
        if (bytes.length > (3 << 20)) return '';
      }
      if (bytes.isEmpty) return '';
      final f = File(_file(url));
      final tmp = File('${f.path}.part');
      await tmp.writeAsBytes(bytes.takeBytes(), flush: false);
      await tmp.rename(f.path);
      return _paths[url] = f.path;
    } catch (_) {
      return '';
    } finally {
      _active--;
      if (_waiting.isNotEmpty) _waiting.removeAt(0).complete();
    }
  }

  /// Fetch a batch in the background.
  static void precache(Iterable<String> urls) {
    for (final u in urls.where((u) => u.isNotEmpty).toSet()) {
      if (peek(u) == null) unawaited(path(u));
    }
  }
}

/// Album art with a graceful placeholder.
class Cover extends StatelessWidget {
  const Cover({super.key, required this.url, required this.seed, this.size = 56, this.radius = Radii.cover, this.round = false});
  final String? url;
  final String seed;
  final double size;
  final double radius;

  /// A circle (artists).
  final bool round;

  Widget _placeholder() {
    final accent = HarpColors.accentFor(seed);
    return DecoratedBox(
      decoration: BoxDecoration(gradient: LinearGradient(colors: [accent.withValues(alpha: 0.7), accent.withValues(alpha: 0.2)])),
      child: Center(child: Icon(Icons.music_note_rounded, size: size * 0.4, color: Colors.white70)),
    );
  }

  @override
  Widget build(BuildContext context) {
    final dpr = MediaQuery.devicePixelRatioOf(context);
    Widget image(String path) => Image.file(
          File(path),
          fit: BoxFit.cover,
          cacheWidth: (size * dpr).round(),
          gaplessPlayback: true,
          errorBuilder: (_, _, _) => _placeholder(),
        );
    Widget body;
    final u = url;
    if (u == null || u.isEmpty) {
      body = _placeholder();
    } else if (ThumbCache.peek(u) != null) {
      body = image(ThumbCache.peek(u)!);
    } else {
      body = FutureBuilder<String>(
        future: ThumbCache.path(u),
        builder: (_, snap) => snap.hasData && snap.data!.isNotEmpty ? image(snap.data!) : _placeholder(),
      );
    }
    return RepaintBoundary(
      child: ClipRRect(
        borderRadius: BorderRadius.circular(round ? size / 2 : radius),
        clipBehavior: Clip.hardEdge,
        child: SizedBox(width: size, height: size, child: body),
      ),
    );
  }
}
