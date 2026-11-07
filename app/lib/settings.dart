import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

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

  /// "auto" (watches the frame rate and backs off on slow machines), "full",
  /// "reduced" (still background, no blur) or "off" (flat).
  String effectsPref = 'auto';
  String _autoLevel = 'full';

  /// "off", "wide", "headphone" or "immersive": a virtual spatial effect (not Dolby Atmos).
  String spatial = 'off';

  /// What is actually used now.
  String get effects => effectsPref == 'auto' ? _autoLevel : effectsPref;
  bool get fullEffects => effects == 'full';
  bool get flatBackground => effects == 'off';

  /// Set by the player: something is playing, so the background animates.
  bool animating = false;
  final List<bool> _slowFrames = [];
  bool _watching = false;

  /// Starts watching frame times. If more than a third of the frames take
  /// over 24 ms while the background is animating, effects step down one level
  /// (and stay there next launch). It never steps back up on its own.
  void watchPerformance() {
    if (_watching) return;
    _watching = true;
    SchedulerBinding.instance.addTimingsCallback(_onTimings);
  }

  void _onTimings(List<FrameTiming> timings) {
    if (effectsPref != 'auto' || _autoLevel == 'off' || !animating) {
      _slowFrames.clear();
      return;
    }
    for (final t in timings) {
      _slowFrames.add(t.totalSpan.inMilliseconds > 24);
    }
    if (_slowFrames.length < 90) return;
    final slow = _slowFrames.where((s) => s).length / _slowFrames.length;
    _slowFrames.clear();
    if (slow > 0.34) {
      _autoLevel = _autoLevel == 'full' ? 'reduced' : 'off';
      _save('effects_auto', _autoLevel);
      notifyListeners();
    }
  }

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
    effectsPref = await get('effects') ?? 'auto';
    _autoLevel = await get('effects_auto') ?? 'full';
    spatial = await get('spatial') ?? 'off';
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

  Future<void> setEffects(String v) async {
    effectsPref = v;
    if (v == 'auto') _autoLevel = 'full'; // measure again
    notifyListeners();
    await _save('effects', v);
    await _save('effects_auto', _autoLevel);
  }

  Future<void> setSpatial(String v) async {
    spatial = v;
    notifyListeners();
    await _save('spatial', v);
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

String effectsLabel(String e) => switch (e) {
      'full' => 'Full · blur and live motion',
      'reduced' => 'Reduced · still background, no blur',
      'off' => 'Off · flat, fastest',
      _ => 'Auto · adapts to your machine',
    };

String spatialLabel(String s) => switch (s) {
      'wide' => 'Wide stage',
      'headphone' => 'Headphone crossfeed',
      'immersive' => 'Virtual 3D',
      _ => 'Off',
    };
