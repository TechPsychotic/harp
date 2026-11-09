import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:media_kit/media_kit.dart';

/// A 0..1 loudness value that the background follows.
///
/// The real signal: mpv runs an `astats` audio filter and reports the RMS
/// level of the audio it is playing, which is read here ~20 times a second.
/// If a build of libmpv has no such filter (or it reports nothing), a gentle
/// beat-like pulse seeded by the song stands in, so the background still moves
/// with playback instead of freezing.
class Visualizer {
  final ValueNotifier<double> level = ValueNotifier(0);

  /// True once real levels arrived from the player.
  bool get isReal => _real;

  bool enabled = true;
  bool playing = false;
  String songSeed = '';
  Duration Function() position = () => Duration.zero;

  bool _real = false;
  DateTime _lastReal = DateTime.fromMillisecondsSinceEpoch(0);
  double _target = 0;
  Timer? _timer;

  NativePlayer? _np;

  /// mpv audio filters for a spatial mode (virtual effects, not Dolby Atmos).
  /// All of these were checked against a real libmpv.
  static String? spatialFilter(String mode) => switch (mode) {
        'wide' => 'stereowiden=delay=20:feedback=0.3:crossfeed=0.3:drymix=0.8',
        'headphone' => 'crossfeed=strength=0.35:range=0.5:level_in=0.9:level_out=1',
        'immersive' => 'stereowiden=delay=24:feedback=0.35:crossfeed=0.35:drymix=0.75,crossfeed=strength=0.2:range=0.5',
        _ => null,
      };

  /// The whole `af` value: optional spatial effect, then the loudness analyzer
  /// (after the effect, so the background follows what you actually hear).
  /// Equalizer presets as FFmpeg filters. Boosting presets first lower the
  /// volume a little so loud songs don't clip.
  static String? eqFilter(String preset) {
    String band(int f, double g) => 'equalizer=f=$f:t=q:w=1:g=$g';
    return switch (preset) {
      'bass' => ['volume=-5dB', band(60, 6), band(150, 3.5), band(400, -1)].join(','),
      'vocal' => ['volume=-3dB', band(250, -2), band(1500, 3), band(3500, 3), band(9000, -1)].join(','),
      'treble' => ['volume=-4dB', band(4000, 2), band(8000, 4), band(14000, 5)].join(','),
      'loudness' => ['volume=-4dB', band(60, 5), band(3000, -1), band(10000, 4)].join(','),
      _ => null,
    };
  }

  /// Evens out the volume between songs (and within them, gently): a slow
  /// leveller. (FFmpeg's `loudnorm` would be the textbook choice, but it stalls
  /// libmpv's playback: checked.)
  static const normalizeFilter = 'dynaudnorm=f=500:g=31:p=0.9:m=8';

  static String filterChain({required String spatial, required bool analyze, String eq = 'flat', bool normalize = false}) {
    final parts = <String>[];
    if (normalize) parts.add('@norm:lavfi=[$normalizeFilter]');
    final e = eqFilter(eq);
    if (e != null) parts.add('@eq:lavfi=[$e]');
    final fx = spatialFilter(spatial);
    if (fx != null) parts.add('@fx:lavfi=[$fx]');
    if (analyze) parts.add('@harp:lavfi=[astats=metadata=1:reset=1:length=0.05]');
    return parts.join(',');
  }

  Future<void> attach(Player player, {bool native = true}) async {
    try {
      if (!native) throw 'no native filters on this platform';
      _np = player.platform as NativePlayer;
      await _np!.observeProperty('af-metadata/harp', (s) async => _onMetadata(s));
    } catch (_) {
      // No filter support: the stand-in pulse is used.
    }
    _timer = Timer.periodic(const Duration(milliseconds: 50), (_) => _tick());
  }

  /// (Re)applies the audio filters. Safe if the player has no filter support.
  Future<void> applyFilters({required String spatial, required bool analyze, String eq = 'flat', bool normalize = false}) async {
    try {
      await _np?.setProperty('af', filterChain(spatial: spatial, analyze: analyze, eq: eq, normalize: normalize));
    } catch (_) {}
  }

  /// Parses mpv's metadata text for the RMS level in dB, e.g. -23.4 (or "-inf" for silence).
  static double? parseRmsDb(String s) {
    final m = RegExp(r'Overall\.RMS_level["\x27]?\s*[:=]\s*["\x27]?(-?inf|-?[0-9]+(?:\.[0-9]+)?)').firstMatch(s);
    if (m == null) return null;
    final v = m.group(1)!;
    if (v.endsWith('inf')) return -90;
    return double.tryParse(v);
  }

  /// dB (about -60 quiet .. -8 loud) to 0..1.
  static double dbToLevel(double db) => ((db + 55) / 45).clamp(0.0, 1.0);

  void _onMetadata(String s) {
    final db = parseRmsDb(s);
    if (db == null) return;
    _real = true;
    _lastReal = DateTime.now();
    _target = dbToLevel(db);
  }

  /// A pulse that feels like a beat: tempo and strength come from the song id.
  static double synthetic(String seed, double seconds) {
    var h = 7;
    for (final c in seed.codeUnits) {
      h = (h * 31 + c) & 0x7fffffff;
    }
    final bpm = 78 + (h % 64); // 78..141
    final strength = 0.45 + (h % 40) / 100; // .45..84
    final beat = (seconds * bpm / 60) % 1;
    final pulse = math.pow(1 - beat, 3).toDouble(); // sharp hit, slow decay
    final swell = 0.65 + 0.35 * math.sin(seconds / 7.0 + h);
    return (0.18 + strength * pulse) * swell;
  }

  void _tick() {
    double target;
    if (!enabled || !playing) {
      target = 0;
    } else if (DateTime.now().difference(_lastReal) < const Duration(milliseconds: 1500)) {
      target = _target;
    } else {
      target = synthetic(songSeed, position().inMilliseconds / 1000);
    }
    final cur = level.value;
    // Quick attack, slower release: feels like a meter.
    final next = target > cur ? cur + (target - cur) * 0.6 : cur + (target - cur) * 0.14;
    if ((next - cur).abs() > 0.012 || (target == 0 && next > 0.004)) {
      level.value = target == 0 && next < 0.01 ? 0 : next;
    }
  }

  void dispose() {
    _timer?.cancel();
    level.dispose();
  }
}
