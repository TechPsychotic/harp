// Two engine players at once, volumes crossing, then the first closed.
// HARP_A=… HARP_B=… flutter test test_engine/crossfade_test.dart
// ignore_for_file: avoid_print
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  test('crossfade between two players', () async {
    MpvAudioKit.ensureInitialized();
    final a = Player(), b = Player();
    var pa = Duration.zero, pb = Duration.zero;
    a.stream.position.listen((d) => pa = d);
    b.stream.position.listen((d) => pb = d);
    await a.open(Media(Platform.environment['HARP_A']!), play: true);
    await Future<void>.delayed(const Duration(seconds: 2));
    await b.setVolume(0);
    await b.open(Media(Platform.environment['HARP_B']!), play: true);
    for (var i = 1; i <= 20; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
      await b.setVolume(100 * i / 20);
      await a.setVolume(100 * (1 - i / 20));
    }
    final aAt = pa, bAt = pb;
    print('after the crossing: a at ${aAt.inMilliseconds} ms, b at ${bAt.inMilliseconds} ms');
    await a.dispose();
    await Future<void>.delayed(const Duration(seconds: 2));
    print('2 s after closing a: b at ${pb.inMilliseconds} ms, volume ${b.state.volume}');
    expect(bAt.inMilliseconds, greaterThan(1000), reason: 'both played at once');
    expect(pb - bAt, greaterThan(const Duration(milliseconds: 1500)), reason: 'b keeps playing after a closes');
    await b.dispose();
  });
}
