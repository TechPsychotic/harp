import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harp/theme/glass.dart';
import 'package:harp/widgets/cover.dart';
import 'package:harp/widgets/song_tile.dart';

void main() {
  test('accent is stable per song and differs between songs', () {
    expect(HarpColors.accentFor('abc'), HarpColors.accentFor('abc'));
    expect(HarpColors.accentFor('abc'), isNot(HarpColors.accentFor('abd')));
    expect(HarpColors.accentFor(null), isA<Color>());
  });

  test('duration formatting', () {
    expect(fmtDuration(248), '4:08');
    expect(fmtDuration(null), '');
  });

  testWidgets('glass widgets render and respond', (tester) async {
    var taps = 0;
    await tester.pumpWidget(MaterialApp(
      theme: harpTheme(HarpColors.accentFor('x')),
      home: AuroraBackground(
        accent: HarpColors.accentFor('x'),
        child: Scaffold(
          body: Column(children: [
            GlassCard(onTap: () => taps++, child: const Text('card')),
            GlassChip(label: 'Jazz', onTap: () => taps++),
            GlassIconButton(icon: Icons.play_arrow_rounded, onPressed: () => taps++),
            const Cover(url: null, seed: 'x'),
          ]),
        ),
      ),
    ));
    await tester.tap(find.text('card'));
    await tester.tap(find.text('Jazz'));
    await tester.tap(find.byIcon(Icons.play_arrow_rounded));
    expect(taps, 3);
    expect(find.byIcon(Icons.music_note_rounded), findsOneWidget);
  });
}
