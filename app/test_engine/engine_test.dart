// Plays a tone through mpv_audio_kit with every effect chain the app can make.
// Run by hand: HARP_TONE=/path/tone.ogg flutter test test_engine/engine_test.dart
// ignore_for_file: avoid_print
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harp/player/visualizer.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  final tone = Platform.environment['HARP_TONE'];
  test('every chain the app makes keeps playback going', () async {
    MpvAudioKit.ensureInitialized();
    final p = Player();
    var pos = Duration.zero;
    final errs = <String>[];
    p.stream.position.listen((d) => pos = d);
    p.stream.error.listen((e) => errs.add(e.message));
    await p.open(Media(tone!));
    await p.play();
    for (var i = 0; i < 40 && pos < const Duration(seconds: 1); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    final bad = <String>[];
    for (final eq in ['flat', 'bass', 'vocal', 'treble', 'loudness']) {
      for (final sp in ['off', 'wide', 'headphone', 'immersive']) {
        for (final norm in [false, true]) {
          final name = '$eq/$sp/${norm ? 'norm' : '-'}';
          if (pos > const Duration(seconds: 20)) {
            await p.seek(const Duration(seconds: 1));
            await Future<void>.delayed(const Duration(milliseconds: 400));
          }
          errs.clear();
          final before = pos;
          try {
            await p.setAudioEffects(Visualizer.chain(spatial: sp, eq: eq, normalize: norm)).timeout(const Duration(seconds: 5));
            await p.setVolumeGain(-Visualizer.eqHeadroomDb(eq));
          } catch (e) {
            bad.add('$name threw $e');
            continue;
          }
          await Future<void>.delayed(const Duration(milliseconds: 700));
          if (pos - before < const Duration(milliseconds: 400)) bad.add('$name stalled ${errs.take(1)}');
        }
      }
    }
    // A real headphone correction (AutoEq, AirPods Pro) with everything else on.
    const hp = [
      ('low_shelf', 105.0, 2.6, 0.7), ('peak', 514.0, -4.4, 0.68), ('peak', 8903.0, 6.0, 1.66), ('peak', 183.0, 2.0, 0.77),
      ('peak', 4613.0, 3.4, 2.46), ('high_shelf', 10000.0, -0.5, 0.7), ('peak', 1517.0, -1.0, 2.33),
    ];
    final before = pos;
    try {
      await p.setAudioEffects(Visualizer.chain(spatial: 'wide', eq: 'bass', normalize: true, headphone: hp)).timeout(const Duration(seconds: 5));
      await Future<void>.delayed(const Duration(milliseconds: 900));
      if (pos - before < const Duration(milliseconds: 500)) bad.add('headphone correction stalled');
    } catch (e) {
      bad.add('headphone correction threw $e');
    }
    print('chains tried: 40, problems: ${bad.length}');
    bad.forEach(print);
    await p.dispose();
    expect(bad, isEmpty);
  }, timeout: const Timeout(Duration(minutes: 3)));
}
