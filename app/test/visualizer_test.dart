import 'package:flutter_test/flutter_test.dart';
import 'package:harp/player/visualizer.dart';
import 'package:harp/settings.dart';

void main() {
  audioChainTests();
  settingsTests();
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

void audioChainTests() {
  test('filter chain: spatial effect first, analyzer last, both optional', () {
    expect(Visualizer.filterChain(spatial: 'off', analyze: false), '');
    expect(Visualizer.filterChain(spatial: 'off', analyze: true), startsWith('@harp:lavfi=[astats'));
    final both = Visualizer.filterChain(spatial: 'wide', analyze: true);
    expect(both.indexOf('@fx:'), lessThan(both.indexOf('@harp:')), reason: 'the analyzer hears the processed audio');
    expect(Visualizer.filterChain(spatial: 'headphone', analyze: false), startsWith('@fx:lavfi=[crossfeed'));
    expect(Visualizer.spatialFilter('nonsense'), isNull);
    for (final m in ['wide', 'headphone', 'immersive']) {
      expect(Visualizer.spatialFilter(m), isNotNull);
    }
  });
}

void settingsTests() {
  test('effects level: auto follows the governor, explicit choices win', () {
    final s = AppSettings();
    expect(s.effectsPref, 'reduced', reason: 'the light background is the default');
    expect(s.effects, 'reduced');
    expect(s.fullEffects, isFalse);
    s.effectsPref = 'auto';
    expect(s.effects, 'full', reason: 'auto starts full and only steps down when frames are slow');
    s.effectsPref = 'full';
    expect(s.fullEffects, isTrue);
    s.effectsPref = 'off';
    expect(s.flatBackground, isTrue);
  });
}
