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
  String effectsPref = 'reduced';
  String _autoLevel = 'full';

  /// Shows frame times on screen, to see what is slow.
  bool perfOverlay = false;

  /// Size of the play cache in MB (0 = off). Songs about to play are fetched ahead of time.
  int cacheMb = 512;

  /// Also fetch ahead on mobile data (off: only on Wi-Fi / unmetered networks).
  bool prefetchOnMobile = false;

  /// Lyrics in Hindi, Urdu, Punjabi and other Indian scripts: "both" (the
  /// original with English letters under it), "roman" (English letters only)
  /// or "original".
  String lyricsScript = 'both';

  /// Equalizer preset: "flat", "bass", "vocal", "treble", "loudness" or "custom".
  String eq = 'flat';

  /// The custom equalizer: gain in dB (−12…+12) for each of [eqBands].
  List<double> customEq = List.filled(10, 0);
  static const eqBands = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];

  /// Even out the volume between songs (desktop for now).
  bool normalize = false;

  /// Headphone correction (AutoEq): the model's name, its preamp (dB, ≤ 0)
  /// and filters as (kind, freq, gain dB, Q). Empty name: off.
  String hpName = '';
  double hpPreamp = 0;
  List<(String, double, double, double)> hpFilters = const [];

  /// Crossfade between songs, seconds (0 = off: songs follow gaplessly).
  int crossfade = 0;

  /// Playback speed (1 = normal). Pitch stays the same.
  double speed = 1;

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
    effectsPref = await get('effects') ?? 'reduced';
    perfOverlay = await get('perf_overlay') == '1';
    _autoLevel = await get('effects_auto') ?? 'full';
    spatial = await get('spatial') ?? 'off';
    cacheMb = int.tryParse(await get('cache_mb') ?? '') ?? 512;
    prefetchOnMobile = await get('prefetch_mobile') == '1';
    lyricsScript = await get('lyrics_script') ?? 'both';
    eq = await get('eq') ?? 'flat';
    final ce = (await get('eq_custom') ?? '').split(',').map(double.tryParse).toList();
    if (ce.length == 10 && ce.every((v) => v != null)) customEq = [for (final v in ce) v!.clamp(-12.0, 12.0)];
    normalize = await get('normalize') == '1';
    crossfade = int.tryParse(await get('crossfade') ?? '') ?? 0;
    _readHp(await get('headphone_eq'));
    speed = double.tryParse(await get('speed') ?? '') ?? 1;
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

  Future<void> setPrefetchOnMobile(bool v) async {
    prefetchOnMobile = v;
    notifyListeners();
    await _save('prefetch_mobile', v ? '1' : '0');
  }

  Future<void> setEq(String v) async {
    eq = v;
    notifyListeners();
    await _save('eq', v);
  }

  Future<void> setNormalize(bool v) async {
    normalize = v;
    notifyListeners();
    await _save('normalize', v ? '1' : '0');
  }

  /// One band of the custom equalizer (and switches to it).
  Future<void> setCustomBand(int i, double db) async {
    customEq = [...customEq]..[i] = db.clamp(-12.0, 12.0);
    eq = 'custom';
    notifyListeners();
    await _save('eq', 'custom');
    await _save('eq_custom', customEq.map((v) => v.toStringAsFixed(1)).join(','));
  }

  Future<void> resetCustomEq() async {
    customEq = List.filled(10, 0);
    notifyListeners();
    await _save('eq_custom', customEq.join(','));
  }

  void _readHp(String? v) {
    // name|preamp|kind,freq,gain,q;kind,freq,gain,q…
    final parts = (v ?? '').split('|');
    if (parts.length != 3 || parts[0].isEmpty) {
      hpName = '';
      hpFilters = const [];
      hpPreamp = 0;
      return;
    }
    hpName = parts[0];
    hpPreamp = double.tryParse(parts[1]) ?? 0;
    hpFilters = [
      for (final f in parts[2].split(';'))
        if (f.split(',').length == 4)
          (f.split(',')[0], double.parse(f.split(',')[1]), double.parse(f.split(',')[2]), double.parse(f.split(',')[3])),
    ];
  }

  /// Turns headphone correction on with a model's filters (or off with an empty name).
  Future<void> setHeadphone(String name, double preamp, List<(String, double, double, double)> filters) async {
    final v = name.isEmpty ? '' : '${name.replaceAll('|', '/')}|$preamp|${filters.map((f) => '${f.$1},${f.$2},${f.$3},${f.$4}').join(';')}';
    _readHp(v);
    notifyListeners();
    await _save('headphone_eq', v);
  }

  Future<void> setCrossfade(int v) async {
    crossfade = v;
    notifyListeners();
    await _save('crossfade', '$v');
  }

  Future<void> setSpeed(double v) async {
    speed = v;
    notifyListeners();
    await _save('speed', '$v');
  }

  Future<void> setLyricsScript(String v) async {
    lyricsScript = v;
    notifyListeners();
    await _save('lyrics_script', v);
  }

  Future<void> setCacheMb(int v) async {
    cacheMb = v;
    notifyListeners();
    await _save('cache_mb', '$v');
  }

  Future<void> setPerfOverlay(bool v) async {
    perfOverlay = v;
    notifyListeners();
    await _save('perf_overlay', v ? '1' : '0');
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
      'immersive' => 'Immersive (wide + crossfeed)',
      _ => 'Off',
    };

String eqLabel(String s) => switch (s) {
      'bass' => 'Bass boost',
      'vocal' => 'Vocal',
      'treble' => 'Treble boost',
      'loudness' => 'Loudness (low volume)',
      'custom' => 'Custom',
      _ => 'Flat',
    };
