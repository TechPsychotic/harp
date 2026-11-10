// What the engine does at a song boundary with a resolver-backed next entry.
// HARP_A=/a.ogg HARP_B=/b.ogg flutter test test_engine/gapless_test.dart
// ignore_for_file: avoid_print
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

void main() {
  test('gapless into a resolved entry', () async {
    MpvAudioKit.ensureInitialized();
    final p = Player();
    final t0 = DateTime.now();
    String ts() => '${DateTime.now().difference(t0).inMilliseconds}ms';
    p.stream.playlist.listen((pl) => print('${ts()} playlist index=${pl.index} items=${pl.items.map((m) => m.extras?['harpId'] ?? 'x').toList()}'));
    p.stream.completed.listen((c) => print('${ts()} completed=$c'));
    p.stream.playing.listen((v) => print('${ts()} playing=$v'));
    await p.setGapless(Gapless.weak);
    await p.setPrefetchPlaylist(true);
    var resolved = 0;
    await p.setSourceResolver((r) {
      final id = r.media.extras?['harpId'];
      if (id == 'b' && r.uri.startsWith('harp:')) {
        resolved++;
        print('${ts()} resolver: b (retry=${r.isRetry})');
        return Platform.environment['HARP_B'];
      }
      return null;
    });
    await p.open(Media(Platform.environment['HARP_A']!, extras: const {'harpId': 'a'}), play: true);
    await p.add(Media(Platform.environment['HARP_B']!, extras: const {'harpId': 'b'}));
    await Future<void>.delayed(const Duration(seconds: 7));
    print('${ts()} state index=${p.state.playlist.index} pos=${p.state.position.inMilliseconds} resolved=$resolved');
    await p.remove(0);
    await Future<void>.delayed(const Duration(milliseconds: 300));
    print('${ts()} after remove(0): index=${p.state.playlist.index} playing=${p.state.playing}');
    expect(p.state.playlist.index, 0);
    expect(p.state.playing, isTrue, reason: 'still playing after the boundary');
    await p.dispose();
  });
}
