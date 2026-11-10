import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import 'native.dart';
import 'src/rust/api/engine.dart' as api;
import 'version.dart';

/// A newer Harp on GitHub Releases.
class Release {
  const Release({required this.tag, required this.url, required this.assets});
  final String tag;

  /// The release page.
  final String url;

  /// File name → download link.
  final Map<String, String> assets;

  Map<String, dynamic> toJson() => {'tag': tag, 'url': url, 'assets': assets};
  static Release? fromJson(String s) {
    try {
      final j = jsonDecode(s) as Map<String, dynamic>;
      return Release(tag: j['tag'] as String, url: j['url'] as String, assets: (j['assets'] as Map).cast<String, String>());
    } catch (_) {
      return null;
    }
  }
}

/// Looks for a newer Harp on GitHub Releases (at most once a day) and, on
/// Android and the Linux install, puts it in for you: it downloads the file,
/// checks it against the release's SHA256SUMS, then (Android) hands it to the
/// system installer, where you confirm, or (Linux) swaps it in and restarts.
/// Elsewhere it opens the release page.
class Updates {
  static final ValueNotifier<Release?> available = ValueNotifier(null);

  /// While an update is being put in: 0..1 downloaded, then the step.
  static final ValueNotifier<({double progress, String step})?> busy = ValueNotifier(null);

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

  static String get _home => Platform.environment['HOME'] ?? '';
  static String get _linuxDir => '$_home/.local/opt/harp';

  /// The file this device would install, if Harp can update itself here.
  static String? get assetName {
    if (Platform.isAndroid) return 'harp-android-arm64.apk';
    // Only the per-user install (scripts/install-linux.sh) can be replaced in place.
    if (Platform.isLinux && Platform.resolvedExecutable.startsWith('$_linuxDir/')) return 'harp-linux-x64.tar.gz';
    return null;
  }

  /// Whether this device can take [r] without leaving Harp.
  static bool canInstall(Release r) {
    final n = assetName;
    return n != null && r.assets.containsKey(n) && r.assets.containsKey('SHA256SUMS.txt');
  }

  static Future<void> check({bool force = false}) async {
    try {
      final last = int.tryParse(await api.settingGet(key: 'update_checked') ?? '') ?? 0;
      final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      if (!force && now - last < 86400) {
        final saved = Release.fromJson(await api.settingGet(key: 'update_release') ?? '');
        if (saved != null && newer(saved.tag, appVersion)) available.value = saved;
        return;
      }
      final j = jsonDecode(await _get('https://api.github.com/repos/$repo/releases/latest', json: true)) as Map<String, dynamic>;
      await api.settingSet(key: 'update_checked', value: '$now');
      final r = Release(
        tag: j['tag_name'] as String? ?? '',
        url: j['html_url'] as String? ?? 'https://github.com/$repo/releases',
        assets: {
          for (final a in (j['assets'] as List? ?? const []).cast<Map<String, dynamic>>())
            if (a['name'] is String && a['browser_download_url'] is String) a['name'] as String: a['browser_download_url'] as String,
        },
      );
      await api.settingSet(key: 'update_release', value: jsonEncode(r.toJson()));
      available.value = newer(r.tag, appVersion) ? r : null;
    } catch (_) {
      // Offline, or the repository isn't public: nothing to say.
    }
  }

  static Future<String> _get(String url, {bool json = false}) async {
    final client = HttpClient()..connectionTimeout = const Duration(seconds: 10);
    try {
      final req = await client.getUrl(Uri.parse(url));
      if (json) req.headers.set('Accept', 'application/vnd.github+json');
      req.headers.set('User-Agent', 'Harp/$appVersion');
      final res = await req.close().timeout(const Duration(seconds: 20));
      final body = await res.transform(utf8.decoder).join();
      if (res.statusCode != 200) throw HttpException('HTTP ${res.statusCode}');
      return body;
    } finally {
      client.close();
    }
  }

  /// The expected checksum of [name] in a SHA256SUMS file.
  @visibleForTesting
  static String? sumFor(String sums, String name) {
    for (final l in const LineSplitter().convert(sums)) {
      final m = RegExp(r'^([0-9a-fA-F]{64})\s+\*?(.+)$').firstMatch(l.trim());
      if (m != null && m.group(2)!.trim() == name) return m.group(1)!.toLowerCase();
    }
    return null;
  }

  static Future<File> _download(String url, File to) async {
    final client = HttpClient()..connectionTimeout = const Duration(seconds: 15);
    try {
      final req = await client.getUrl(Uri.parse(url));
      req.headers.set('User-Agent', 'Harp/$appVersion');
      final res = await req.close();
      if (res.statusCode != 200) throw HttpException('download failed (HTTP ${res.statusCode})');
      final total = res.contentLength;
      var got = 0;
      final sink = to.openWrite();
      await for (final chunk in res) {
        sink.add(chunk);
        got += chunk.length;
        if (total > 0) busy.value = (progress: got / total, step: 'Downloading');
      }
      await sink.close();
      return to;
    } finally {
      client.close();
    }
  }

  /// Downloads, checks and puts in [r]. Returns a message for the screen, or
  /// null when the system takes over (Android's prompt, or Harp restarting).
  static Future<String?> install(Release r) async {
    final name = assetName;
    if (name == null || !canInstall(r)) return 'This update has to be installed from its release page.';
    if (busy.value != null) return null;
    busy.value = (progress: 0, step: 'Downloading');
    try {
      final dir = Directory('${(await getTemporaryDirectory()).path}/harp-update');
      if (dir.existsSync()) dir.deleteSync(recursive: true);
      dir.createSync(recursive: true);
      final want = sumFor(await _get(r.assets['SHA256SUMS.txt']!), name);
      if (want == null) return 'The release has no checksum for $name; not installing it.';
      final file = await _download(r.assets[name]!, File('${dir.path}/$name'));
      busy.value = (progress: 1, step: 'Checking');
      final have = (await sha256.bind(file.openRead()).first).toString();
      if (have != want) {
        file.deleteSync();
        return 'The download doesn’t match its checksum; not installing it. Try again later.';
      }
      busy.value = (progress: 1, step: 'Installing');
      if (Platform.isAndroid) {
        final s = await Native.installApk(file.path);
        return s == 'permission' ? 'Allow Harp to install updates, then tap Update again.' : null;
      }
      return await _swapLinux(file, dir);
    } catch (e) {
      return 'The update didn’t finish: $e';
    } finally {
      busy.value = null;
    }
  }

  /// Unpacks next to the install, swaps it in only once that worked, restarts.
  static Future<String?> _swapLinux(File tar, Directory work) async {
    final fresh = Directory('$_linuxDir.new');
    final old = Directory('$_linuxDir.old');
    for (final d in [fresh, old]) {
      if (d.existsSync()) d.deleteSync(recursive: true);
    }
    fresh.createSync(recursive: true);
    final x = await Process.run('tar', ['-xzf', tar.path, '-C', fresh.path]);
    if (x.exitCode != 0 || !File('${fresh.path}/harp').existsSync()) {
      fresh.deleteSync(recursive: true);
      return 'The update couldn’t be unpacked; your Harp is unchanged.';
    }
    // The running copy keeps working from memory while its folder is renamed.
    Directory(_linuxDir).renameSync(old.path);
    fresh.renameSync(_linuxDir);
    try {
      old.deleteSync(recursive: true);
      work.deleteSync(recursive: true);
    } catch (_) {}
    final launcher = File('$_home/.local/bin/harp');
    // Harp is single-instance: start the new one once this one has gone.
    await Process.start('sh', ['-c', 'sleep 2; exec "\$0"', launcher.existsSync() ? launcher.path : '$_linuxDir/harp'], mode: ProcessStartMode.detached);
    exit(0);
  }
}
