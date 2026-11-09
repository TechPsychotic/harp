// Plays a tone through mpv_audio_kit with each effect (desktop). Run by hand:
// HARP_TONE=/path/tone.ogg flutter test test_engine/engine_test.dart
import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  final tone = Platform.environment['HARP_TONE'];
  test('effects keep playback going', () async {
    MpvAudioKit.ensureInitialized();
    final p = Player();
    var pos = Duration.zero;
    var fft = 0;
    final errs = <String>[];
    p.stream.position.listen((d) => pos = d);
    p.stream.error.listen((e) => errs.add(e.message));
    p.stream.fft.listen((_) => fft++);
    await p.open(Media(tone!));
    await p.play();
    for (var i = 0; i < 40 && pos < const Duration(seconds: 1); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    print('start pos=$pos mpv=${await p.getRawProperty('mpv-version')}');
    for (final (name, fx) in <(String, List<AudioEffect>)>[
      ('eq', [const EqualizerEffect(frequency: 60, gain: 6, width: 1, widthType: EqualizerWidthType.q)]),
      ('widen', [const StereowidenEffect()]),
      ('crossfeed', [const CrossfeedEffect()]),
      ('dynaudnorm', [const DynaudnormEffect()]),
      ('loudnorm', [const LoudnormEffect(i: -16, tp: -1.5, lra: 11)]),
      ('sofalizer', [const RawAudioEffect('lavfi-sofalizer=sofa=/nonexistent.sofa')]),
    ]) {
      errs.clear();
      final before = pos;
      String? thrown;
      try {
        await p.setAudioEffects(fx).timeout(const Duration(seconds: 5));
      } catch (e) {
        thrown = '$e';
      }
      await Future<void>.delayed(const Duration(milliseconds: 2000));
      print('$name moved=${(pos - before).inMilliseconds}ms thrown=$thrown errs=${errs.take(2)}');
      if (thrown != null || pos - before < const Duration(milliseconds: 1000)) {
        try { await p.setAudioEffects(const []); } catch (_) {}
        await Future<void>.delayed(const Duration(milliseconds: 800));
      }
    }
    print('fft frames=$fft');
    await p.dispose();
  }, timeout: const Timeout(Duration(minutes: 2)));
}
