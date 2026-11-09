// ignore_for_file: avoid_print
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  test('opening the next song: paused unless play: true', () async {
    MpvAudioKit.ensureInitialized();
    final p = Player();
    var playing = false;
    p.stream.playing.listen((v) => playing = v);
    final tone = Platform.environment['HARP_TONE']!;
    await p.open(Media(tone));
    await p.play();
    await Future<void>.delayed(const Duration(seconds: 1));
    print('first song playing=$playing');
    await p.open(Media(tone));
    await Future<void>.delayed(const Duration(seconds: 1));
    print('next song, play: null -> playing=$playing');
    await p.open(Media(tone), play: true);
    await Future<void>.delayed(const Duration(seconds: 1));
    print('next song, play: true -> playing=$playing');
    expect(playing, isTrue);
    await p.dispose();
  });
}
