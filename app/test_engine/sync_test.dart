// A "guest" player 800 ms behind a "host" catches up by playing slightly
// faster (the rule the app uses), checked every 2 s like host updates.
// HARP_A=… flutter test test_engine/sync_test.dart
// ignore_for_file: avoid_print
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  test('rate nudging closes the gap', () async {
    MpvAudioKit.ensureInitialized();
    final host = Player(), guest = Player();
    var ph = Duration.zero, pg = Duration.zero;
    host.stream.position.listen((d) => ph = d);
    guest.stream.position.listen((d) => pg = d);
    await host.setVolume(0);
    await guest.setVolume(0);
    await host.open(Media(Platform.environment['HARP_A']!), play: true);
    await Future<void>.delayed(const Duration(milliseconds: 1800));
    await guest.open(Media(Platform.environment['HARP_A']!), play: true);
    await Future<void>.delayed(const Duration(milliseconds: 600));
    await guest.seek(ph - const Duration(milliseconds: 800), exact: true);
    await Future<void>.delayed(const Duration(milliseconds: 400));
    for (var round = 0; round < 6; round++) {
      final behind = (ph - pg).inMilliseconds;
      if (behind.abs() > 400) {
        print('round $round: guest behind by $behind ms → exact seek');
        await guest.seek(ph, exact: true);
        await guest.setRate(1);
      } else {
        final rate = behind.abs() < 40 ? 1.0 : (1 + (behind / 2000).clamp(-0.08, 0.08));
        print('round $round: guest behind by $behind ms → rate ${rate.toStringAsFixed(3)}');
        await guest.setRate(rate);
      }
      await Future<void>.delayed(const Duration(seconds: 2));
    }
    final last = (ph - pg).inMilliseconds;
    print('after 12 s: $last ms apart');
    expect(last.abs(), lessThan(80));
    await host.dispose();
    await guest.dispose();
  }, timeout: const Timeout(Duration(minutes: 2)));
}
