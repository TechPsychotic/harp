// Harp's player on the 3D output: plays a file through mpv_audio_kit with the device the app
// uses for 3D audio. Run by hand while 3D audio runs (see scripts note in UPDATES.md):
//   HARP_TONE=/path/left_only.wav flutter test test_engine/spatial_route_test.dart
// ignore_for_file: avoid_print
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  final tone = Platform.environment['HARP_TONE'];
  test('the player plays on pulse/harp_spatial', () async {
    MpvAudioKit.ensureInitialized();
    final p = Player();
    var pos = Duration.zero;
    p.stream.position.listen((d) => pos = d);
    await Future<void>.delayed(const Duration(milliseconds: 500));
    print('devices: ${p.state.audioDevices.map((d) => d.name).toList()}');
    await p.setAudioDevice(const Device(name: 'pulse/harp_spatial', description: 'Harp 3D'));
    await p.open(Media(tone!));
    await p.play();
    for (var i = 0; i < 80 && pos < const Duration(seconds: 2); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    print('device now: ${p.state.audioDevice.name}, position ${pos.inMilliseconds} ms');
    expect(pos, greaterThan(const Duration(seconds: 1)));
    await Future<void>.delayed(const Duration(seconds: 3));
    await p.dispose();
  }, timeout: const Timeout(Duration(seconds: 40)));
}
