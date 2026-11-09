import 'package:flutter_test/flutter_test.dart';
import 'package:harp/player/visualizer.dart';
import 'package:harp/settings.dart';

void main() {
  audioChainTests();
  settingsTests();
  test('spectrum bands become one level, the low end counting more', () {
    expect(Visualizer.levelFromBands(const []), 0);
    expect(Visualizer.levelFromBands(List.filled(64, 0.0)), 0);
    expect(Visualizer.levelFromBands(List.filled(64, 1.0)), 1);
    final bassy = [for (var i = 0; i < 64; i++) i < 16 ? 0.8 : 0.1];
    final airy = [for (var i = 0; i < 64; i++) i < 16 ? 0.1 : 0.8];
    expect(Visualizer.levelFromBands(bassy), greaterThan(Visualizer.levelFromBands(airy) * 0.9));
    expect(Visualizer.levelFromBands(List.filled(64, 0.5)), inInclusiveRange(0.6, 0.7));
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
  test('effects chain: levelling, then tone, then space; each optional', () {
    expect(Visualizer.chain(spatial: 'off'), isEmpty);
    final all = Visualizer.chain(spatial: 'wide', eq: 'bass', normalize: true);
    final kinds = all.map((e) => e.runtimeType.toString()).toList();
    expect(kinds.first, 'LoudnormEffect');
    expect(kinds.indexOf('EqualizerEffect'), lessThan(kinds.indexOf('StereowidenEffect')));
    expect(kinds.last, 'StereowidenEffect');
    expect(Visualizer.spatialEffects('headphone').single.runtimeType.toString(), 'CrossfeedEffect');
    expect(Visualizer.spatialEffects('nonsense'), isEmpty);
    for (final m in ['wide', 'headphone', 'immersive']) {
      expect(Visualizer.spatialEffects(m), isNotEmpty);
    }
    for (final p in ['bass', 'vocal', 'treble', 'loudness']) {
      expect(Visualizer.eqEffects(p), everyElement(isA<Object>().having((e) => e.runtimeType.toString(), 'type', 'EqualizerEffect')));
      expect(Visualizer.eqHeadroomDb(p), greaterThan(0), reason: '$p leaves headroom');
    }
    expect(Visualizer.eqHeadroomDb('flat'), 0);
    expect(Visualizer.eqEffects('flat'), isEmpty);
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
