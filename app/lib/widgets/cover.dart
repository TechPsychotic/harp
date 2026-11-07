import 'dart:io';

import 'package:flutter/material.dart';

import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';

/// Covers are fetched once into the on-device cache (see `thumbs` in the
/// Rust core), so they show instantly, can be precached, and work offline.
class ThumbCache {
  static final Map<String, String> _paths = {};
  static final Map<String, Future<String>> _pending = {};

  static String? peek(String url) => _paths[url];

  static Future<String> path(String url) {
    final p = _paths[url];
    if (p != null) return Future.value(p);
    return _pending.putIfAbsent(url, () async {
      final r = await api.cachedThumbnail(url: url);
      if (r.isNotEmpty) _paths[url] = r;
      _pending.remove(url);
      return r;
    });
  }

  /// Fetch a batch in the background (the core does 4 at a time).
  static void precache(Iterable<String> urls) {
    final todo = urls.where((u) => u.isNotEmpty && !_paths.containsKey(u)).toSet().toList();
    if (todo.isNotEmpty) api.precacheThumbnails(urls: todo);
  }
}

/// Album art with a graceful placeholder.
class Cover extends StatelessWidget {
  const Cover({super.key, required this.url, required this.seed, this.size = 56, this.radius = Radii.cover});
  final String? url;
  final String seed;
  final double size;
  final double radius;

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
      child: ClipRRect(borderRadius: BorderRadius.circular(radius), child: SizedBox(width: size, height: size, child: body)),
    );
  }
}
