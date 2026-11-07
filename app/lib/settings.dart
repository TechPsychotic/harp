import 'package:flutter/material.dart';

import 'src/rust/api/engine.dart' as api;

/// App settings, saved on the device through the Rust core.
class AppSettings extends ChangeNotifier {
  ThemeMode themeMode = ThemeMode.system;

  /// "low", "medium" or "high"
  String streamQuality = 'high';
  String downloadQuality = 'high';

  /// Play only downloaded songs; never touch the network.
  bool offline = false;

  /// Let the background move with the music.
  bool reactiveBackground = true;

  Future<void> load() async {
    Future<String?> get(String k) => api.settingGet(key: k);
    themeMode = switch (await get('theme')) {
      'dark' => ThemeMode.dark,
      'light' => ThemeMode.light,
      _ => ThemeMode.system,
    };
    streamQuality = await get('stream_quality') ?? 'high';
    downloadQuality = await get('download_quality') ?? 'high';
    offline = await get('offline') == '1';
    reactiveBackground = await get('reactive_bg') != '0';
    notifyListeners();
  }

  Future<void> _save(String k, String v) => api.settingSet(key: k, value: v);

  Future<void> setTheme(ThemeMode m) async {
    themeMode = m;
    notifyListeners();
    await _save('theme', m.name);
  }

  Future<void> setStreamQuality(String q) async {
    streamQuality = q;
    notifyListeners();
    await _save('stream_quality', q);
  }

  Future<void> setDownloadQuality(String q) async {
    downloadQuality = q;
    notifyListeners();
    await _save('download_quality', q);
  }

  Future<void> setOffline(bool v) async {
    offline = v;
    notifyListeners();
    await _save('offline', v ? '1' : '0');
  }

  Future<void> setReactiveBackground(bool v) async {
    reactiveBackground = v;
    notifyListeners();
    await _save('reactive_bg', v ? '1' : '0');
  }
}

String qualityLabel(String q) => switch (q) {
      'low' => 'Low · saves data',
      'medium' => 'Medium',
      _ => 'High · best quality',
    };
