// Measures what each preset does to the sound (desktop): low/high energy vs
// Flat from the spectrum, and stereo width (side vs mid) from the samples.
// HARP_PINK=/path/pink.ogg flutter test test_engine/effects_measure_test.dart
// ignore_for_file: avoid_print
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter_test/flutter_test.dart';
import 'package:harp/player/visualizer.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  test('presets make an audible difference', () async {
    MpvAudioKit.ensureInitialized();
    final p = Player();
    var low = 0.0, high = 0.0, mid = 0.0;
    var side = 0.0, center = 0.0;
    p.stream.fft.listen((f) {
      final b = f.bands;
      final q = b.length ~/ 4;
      for (var i = 0; i < b.length; i++) {
        final e = math.pow(10, (b[i] * 70 - 90) / 10).toDouble(); // band 0..1 over a 70 dB window → power
        if (i < q) {
          low += e;
        } else if (i >= b.length - q) {
          high += e;
        } else {
          mid += e;
        }
      }
    });
    p.stream.pcm.listen((f) {
      if (f.channels != 2) return;
      final s = f.samples;
      for (var i = 0; i + 1 < s.length; i += 2) {
        final m = (s[i] + s[i + 1]) / 2, d = (s[i] - s[i + 1]) / 2;
        center += m * m;
        side += d * d;
      }
    });
    await p.open(Media(Platform.environment['HARP_PINK']!), play: true);
    await Future<void>.delayed(const Duration(seconds: 1));
    Future<(double, double, double)> measure(List<AudioEffect> fx, double gain) async {
      await p.setAudioEffects(fx);
      await p.setVolumeGain(-gain);
      await Future<void>.delayed(const Duration(milliseconds: 700));
      low = high = mid = side = center = 0;
      await Future<void>.delayed(const Duration(milliseconds: 2500));
      double db(double a, double b) => 10 * math.log(a / b) / math.ln10;
      return (db(low, mid), db(high, mid), db(side, center));
    }
    final flat = await measure(const [], 0);
    print('flat: low/mid ${flat.$1.toStringAsFixed(1)} dB, high/mid ${flat.$2.toStringAsFixed(1)} dB, side/mid ${flat.$3.toStringAsFixed(1)} dB');
    for (final eq in ['bass', 'vocal', 'treble', 'loudness']) {
      final m = await measure(Visualizer.eqEffects(eq), Visualizer.eqHeadroomDb(eq));
      print('$eq: lows ${(m.$1 - flat.$1).toStringAsFixed(1)} dB, highs ${(m.$2 - flat.$2).toStringAsFixed(1)} dB (vs flat)');
    }
    for (final sp in ['wide', 'headphone', 'immersive']) {
      final m = await measure(Visualizer.spatialEffects(sp), 0);
      print('$sp: stereo width ${(m.$3 - flat.$3).toStringAsFixed(1)} dB (side vs mid, vs flat)');
    }
    // The previous presets, for comparison.
    EqualizerEffect band(double f, double g) => EqualizerEffect(frequency: f, gain: g, width: 1, widthType: EqualizerWidthType.q);
    final oldBass = await measure([band(60, 6), band(150, 3.5), band(400, -1)], 5);
    final oldTreble = await measure([band(4000, 2), band(8000, 4), band(14000, 5)], 4);
    final oldWide = await measure(const [StereowidenEffect(delay: 20, feedback: 0.3, crossfeed: 0.3, drymix: 0.8)], 0);
    print('OLD bass: lows ${(oldBass.$1 - flat.$1).toStringAsFixed(1)} dB; OLD treble: highs ${(oldTreble.$2 - flat.$2).toStringAsFixed(1)} dB; OLD wide: width ${(oldWide.$3 - flat.$3).toStringAsFixed(1)} dB');
    await p.dispose();
  }, timeout: const Timeout(Duration(minutes: 2)));
}
