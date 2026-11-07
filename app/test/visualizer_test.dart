import 'package:flutter_test/flutter_test.dart';
import 'package:harp/player/visualizer.dart';

void main() {
  test('reads RMS level from mpv metadata text in several shapes', () {
    expect(Visualizer.parseRmsDb('{"lavfi.astats.Overall.RMS_level":"-23.40","lavfi.astats.Overall.Peak_level":"-9.1"}'), -23.4);
    expect(Visualizer.parseRmsDb("lavfi.astats.Overall.RMS_level=-31.25"), -31.25);
    expect(Visualizer.parseRmsDb('"lavfi.astats.Overall.RMS_level": "-inf"'), -90);
    expect(Visualizer.parseRmsDb('nothing here'), isNull);
  });

  test('dB maps to a 0..1 level', () {
    expect(Visualizer.dbToLevel(-90), 0);
    expect(Visualizer.dbToLevel(0), 1);
    expect(Visualizer.dbToLevel(-32.5), closeTo(0.5, 0.001));
  });

  test('the stand-in pulse is steady per song, bounded, and differs between songs', () {
    for (var t = 0.0; t < 30; t += 0.37) {
      final v = Visualizer.synthetic('abc', t);
      expect(v, inInclusiveRange(0, 1.2));
      expect(Visualizer.synthetic('abc', t), v);
    }
    final a = [for (var t = 0.0; t < 5; t += 0.2) Visualizer.synthetic('abc', t)];
    final b = [for (var t = 0.0; t < 5; t += 0.2) Visualizer.synthetic('zzz', t)];
    expect(a, isNot(b));
  });
}
