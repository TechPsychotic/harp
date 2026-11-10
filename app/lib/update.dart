import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

import 'src/rust/api/engine.dart' as api;
import 'version.dart';

/// Looks for a newer Harp on GitHub Releases (at most once a day). Nothing is
/// downloaded or installed by itself: Settings › About shows the update and
/// opens its page.
class Updates {
  /// The newer release (tag, page), or null.
  static final ValueNotifier<({String tag, String url})?> available = ValueNotifier(null);

  /// "1.2.3" or "v1.2.3" → [1, 2, 3].
  static List<int> parse(String v) => v.replaceFirst(RegExp(r'^v'), '').split(RegExp(r'[.+-]')).take(3).map((p) => int.tryParse(p) ?? 0).toList();

  static bool newer(String a, String than) {
    final x = parse(a), y = parse(than);
    for (var i = 0; i < 3; i++) {
      final xi = i < x.length ? x[i] : 0, yi = i < y.length ? y[i] : 0;
      if (xi != yi) return xi > yi;
    }
    return false;
  }

  static Future<void> check({bool force = false}) async {
    try {
      final last = int.tryParse(await api.settingGet(key: 'update_checked') ?? '') ?? 0;
      final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      if (!force && now - last < 86400) {
        final saved = await api.settingGet(key: 'update_latest');
        if (saved != null && saved.contains('|') && newer(saved.split('|')[0], appVersion)) {
          available.value = (tag: saved.split('|')[0], url: saved.split('|')[1]);
        }
        return;
      }
      final client = HttpClient()..connectionTimeout = const Duration(seconds: 10);
      final req = await client.getUrl(Uri.parse('https://api.github.com/repos/$repo/releases/latest'));
      req.headers.set('Accept', 'application/vnd.github+json');
      req.headers.set('User-Agent', 'Harp/$appVersion');
      final res = await req.close().timeout(const Duration(seconds: 15));
      final body = await res.transform(utf8.decoder).join();
      client.close();
      await api.settingSet(key: 'update_checked', value: '$now');
      if (res.statusCode != 200) return;
      final j = jsonDecode(body) as Map<String, dynamic>;
      final tag = j['tag_name'] as String? ?? '';
      final url = j['html_url'] as String? ?? 'https://github.com/$repo/releases';
      await api.settingSet(key: 'update_latest', value: '$tag|$url');
      available.value = newer(tag, appVersion) ? (tag: tag, url: url) : null;
    } catch (_) {
      // Offline, or the repository isn't public: nothing to say.
    }
  }
}
