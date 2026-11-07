import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harp/theme/glass.dart';
import 'package:harp/theme/logo.dart';
import 'package:harp/widgets/cover.dart';
import 'package:harp/widgets/song_tile.dart';

Widget _app(Brightness b, Widget child) => MaterialApp(
      theme: harpTheme(HarpColors.accentFor('x'), b),
      home: AuroraBackground(accent: HarpColors.accentFor('x'), child: Scaffold(body: child)),
    );

void main() {
  test('accent is stable per song and differs between songs', () {
    expect(HarpColors.accentFor('abc'), HarpColors.accentFor('abc'));
    expect(HarpColors.accentFor('abc'), isNot(HarpColors.accentFor('abd')));
  });

  test('duration formatting', () {
    expect(fmtDuration(248), '4:08');
    expect(fmtDuration(null), '');
  });

  test('corners are modest', () {
    expect(Radii.card, lessThanOrEqualTo(16));
    expect(Radii.sheet, lessThanOrEqualTo(20));
  });

  for (final b in Brightness.values) {
    testWidgets('glass widgets render and respond in ${b.name} theme', (tester) async {
      var taps = 0;
      await tester.pumpWidget(_app(
        b,
        Column(children: [
          GlassCard(onTap: () => taps++, child: const Text('card')),
          GlassChip(label: 'Jazz', onTap: () => taps++),
          GlassIconButton(icon: Icons.play_arrow_rounded, onPressed: () => taps++),
          const Cover(url: null, seed: 'x'),
          const HarpLogo(size: 48),
          const GlassSurface(child: SizedBox(height: 20, width: 20)),
        ]),
      ));
      await tester.tap(find.text('card'));
      await tester.tap(find.text('Jazz'));
      await tester.tap(find.byIcon(Icons.play_arrow_rounded));
      expect(taps, 3);
      expect(find.byIcon(Icons.music_note_rounded), findsOneWidget);
      final ctx = tester.element(find.text('card'));
      expect(ctx.isDark, b == Brightness.dark);
      // Text stays readable: dark text on light, light text on dark.
      expect(ctx.tok.text.computeLuminance() > 0.5, b == Brightness.dark);
    });
  }

  testWidgets('background animates only when active and survives a level source', (tester) async {
    final level = ValueNotifier<double>(0);
    await tester.pumpWidget(MaterialApp(
      theme: harpTheme(HarpColors.accentFor('x'), Brightness.dark),
      home: AuroraBackground(accent: HarpColors.accentFor('x'), active: true, level: level, child: const SizedBox.expand()),
    ));
    level.value = 0.8;
    await tester.pump(const Duration(milliseconds: 200));
    expect(tester.takeException(), isNull);
    // Switching to inactive stops the ticker without errors.
    await tester.pumpWidget(MaterialApp(
      theme: harpTheme(HarpColors.accentFor('x'), Brightness.dark),
      home: AuroraBackground(accent: HarpColors.accentFor('x'), level: level, child: const SizedBox.expand()),
    ));
    await tester.pump(const Duration(seconds: 1));
    expect(tester.takeException(), isNull);
  });
}
