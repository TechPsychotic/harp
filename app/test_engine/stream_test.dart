// A real YouTube stream through the engine with the header handling.
// HARP_STREAM=/path/stream.json flutter test test_engine/stream_test.dart
// ignore_for_file: avoid_print
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harp/player/stream_headers.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  test('a real stream plays with browser headers', () async {
    final j = jsonDecode(File(Platform.environment['HARP_STREAM']!).readAsStringSync()) as Map<String, dynamic>;
    final raw = [for (final e in (j['headers'] as Map<String, dynamic>).entries) (e.key, e.value as String)];
    MpvAudioKit.ensureInitialized();
    final p = Player();
    var pos = Duration.zero;
    p.stream.position.listen((d) => pos = d);
    // Before: the raw headers are refused.
    try {
      await p.open(Media(j['url'] as String, httpHeaders: {for (final (k, v) in raw) k: v}), play: true);
      print('raw headers: accepted');
    } catch (e) {
      print('raw headers: refused (${'$e'.substring(0, 60)}…)');
    }
    final h = engineHeaders(raw, local: false);
    await p.setRawProperty('user-agent', h.userAgent!);
    await p.open(Media(j['url'] as String, httpHeaders: h.headers), play: true);
    for (var i = 0; i < 100 && pos < const Duration(seconds: 2); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    print('fixed headers: position ${pos.inMilliseconds} ms, user-agent=${(await p.getRawProperty('user-agent'))?.substring(0, 30)}…');
    expect(pos, greaterThan(const Duration(seconds: 1)));
    await p.dispose();
  }, timeout: const Timeout(Duration(minutes: 1)));
}
