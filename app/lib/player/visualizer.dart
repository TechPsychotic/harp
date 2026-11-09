import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart' as mak;

/// A 0..1 loudness value that the background follows, and the chain of audio
/// effects (equalizer, spatial, even volume).
///
/// The level comes from the engine's live spectrum (`mpv_audio_kit`'s FFT of
/// the audio going to the speakers, after the effects), on every platform.
/// If no spectrum arrives (the stream isn't running yet), a gentle beat-like
/// pulse seeded by the song stands in so the background never freezes.
class Visualizer {
  final ValueNotifier<double> level = ValueNotifier(0);

  /// The live spectrum in [bandCount] bands, low to high, each 0..1 (eased,
  /// and settling to zero when paused). Empty until the engine sends any.
  final ValueNotifier<List<double>> bands = ValueNotifier(const []);
  static const bandCount = 24;
  List<double> _bandTarget = const [];

  /// True once real levels arrived from the engine.
  bool get isReal => _real;

  bool _enabled = true;
  bool get enabled => _enabled;

  /// The spectrum is only listened to while the background uses it (it costs CPU).
  set enabled(bool v) {
    if (v == _enabled) return;
    _enabled = v;
    _syncFft();
  }

  bool playing = false;
  String songSeed = '';
  Duration Function() position = () => Duration.zero;

  bool _real = false;
  DateTime _lastReal = DateTime.fromMillisecondsSinceEpoch(0);
  double _target = 0;
  Timer? _timer;
  mak.Player? _player;
  StreamSubscription<mak.FftFrame>? _fft;

  // --- effects ---

  /// Spatial modes (virtual effects, not Dolby Atmos), checked on the engine.
  static List<mak.AudioEffect> spatialEffects(String mode) => switch (mode) {
        'wide' => const [mak.StereowidenEffect(delay: 20, feedback: 0.3, crossfeed: 0.3, drymix: 0.8)],
        'headphone' => const [mak.CrossfeedEffect(strength: 0.35, range: 0.5, levelIn: 0.9, levelOut: 1)],
        'immersive' => const [
            mak.StereowidenEffect(delay: 24, feedback: 0.35, crossfeed: 0.35, drymix: 0.75),
            mak.CrossfeedEffect(strength: 0.2, range: 0.5),
          ],
        _ => const [],
      };

  /// Equalizer presets.
  static List<mak.AudioEffect> eqEffects(String preset) {
    mak.AudioEffect band(double f, double g) => mak.EqualizerEffect(frequency: f, gain: g, width: 1, widthType: mak.EqualizerWidthType.q);
    return switch (preset) {
      'bass' => [band(60, 6), band(150, 3.5), band(400, -1)],
      'vocal' => [band(250, -2), band(1500, 3), band(3500, 3), band(9000, -1)],
      'treble' => [band(4000, 2), band(8000, 4), band(14000, 5)],
      'loudness' => [band(60, 5), band(3000, -1), band(10000, 4)],
      _ => const [],
    };
  }

  /// How far a boosting preset turns the volume down (dB) so loud songs don't
  /// clip. Applied with the engine's own volume gain, after the effects (its
  /// FFmpeg has no `volume` filter; checked).
  static double eqHeadroomDb(String preset) => switch (preset) {
        'bass' => 5,
        'vocal' => 3,
        'treble' || 'loudness' => 4,
        _ => 0,
      };

  /// Even volume: EBU R128 loudness normalisation (songs land at the same
  /// loudness). It used to stall the old engine; this one runs it fine,
  /// checked on a phone and on the laptop.
  static const normalizeEffect = mak.LoudnormEffect(i: -16, tp: -1.5, lra: 11);

  /// The whole chain, in order: levelling, then tone, then space.
  static List<mak.AudioEffect> chain({required String spatial, String eq = 'flat', bool normalize = false}) => [
        if (normalize) normalizeEffect,
        ...eqEffects(eq),
        ...spatialEffects(spatial),
      ];

  Future<void> attach(mak.Player player) async {
    _player = player;
    _syncFft();
    _timer = Timer.periodic(const Duration(milliseconds: 50), (_) => _tick());
  }

  /// Applies the effects. Never throws: a refused chain leaves the sound as it was.
  Future<void> applyEffects({required String spatial, String eq = 'flat', bool normalize = false}) async {
    try {
      await _player?.setAudioEffects(chain(spatial: spatial, eq: eq, normalize: normalize));
      await _player?.setVolumeGain(-eqHeadroomDb(eq));
    } catch (_) {}
  }

  void _syncFft() {
    final p = _player;
    if (p == null) return;
    if (_enabled && _fft == null) {
      _fft = p.stream.fft.listen(_onFft);
    } else if (!_enabled && _fft != null) {
      _fft!.cancel();
      _fft = null;
    }
  }

  /// Spectrum bands (0..1, low to high) to one loudness value. The low end
  /// (kick, bass) counts more: that's what a beat feels like.
  static double levelFromBands(List<double> bands) {
    if (bands.isEmpty) return 0;
    final lowN = math.max(1, bands.length ~/ 4);
    var low = 0.0, all = 0.0;
    for (var i = 0; i < bands.length; i++) {
      all += bands[i];
      if (i < lowN) low += bands[i];
    }
    return ((0.6 * low / lowN + 0.4 * all / bands.length) * 1.35).clamp(0.0, 1.0);
  }

  void _onFft(mak.FftFrame f) {
    _real = true;
    _lastReal = DateTime.now();
    _target = levelFromBands(f.bands);
    _bandTarget = squeeze(f.bands, bandCount);
  }

  /// Averages a spectrum down to [n] bands.
  static List<double> squeeze(List<double> src, int n) {
    if (src.isEmpty) return const [];
    return [
      for (var i = 0; i < n; i++)
        () {
          final a = i * src.length ~/ n, b = math.max(a + 1, (i + 1) * src.length ~/ n);
          var sum = 0.0;
          for (var j = a; j < b; j++) {
            sum += src[j];
          }
          return (sum / (b - a)).clamp(0.0, 1.0);
        }(),
    ];
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
    if (!_enabled || !playing) {
      target = 0;
    } else if (DateTime.now().difference(_lastReal) < const Duration(milliseconds: 1500)) {
      target = _target;
    } else {
      target = synthetic(songSeed, position().inMilliseconds / 1000);
    }
    // Bands ease the same way (settle to zero when paused or without a spectrum).
    final live = _enabled && playing && DateTime.now().difference(_lastReal) < const Duration(milliseconds: 1500);
    final goal = live ? _bandTarget : const <double>[];
    final now = bands.value;
    if (goal.isNotEmpty || now.any((v) => v > 0.005)) {
      bands.value = [
        for (var i = 0; i < bandCount; i++)
          () {
            final c = i < now.length ? now[i] : 0.0;
            final t = i < goal.length ? goal[i] : 0.0;
            return t > c ? c + (t - c) * 0.55 : c + (t - c) * 0.12;
          }(),
      ];
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
    _fft?.cancel();
    level.dispose();
    bands.dispose();
  }
}
