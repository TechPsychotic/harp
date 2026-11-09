import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harp/theme/art_color.dart';
import 'package:harp/theme/music_backdrop.dart';

void main() {
  test('palette finds the main colours of a cover, biggest first', () {
    // 60% red, 30% blue, 10% white pixels.
    final px = <int>[];
    void add(int n, int r, int g, int b) {
      for (var i = 0; i < n; i++) {
        px.addAll([r, g, b, 255]);
      }
    }
    add(60, 220, 30, 40);
    add(30, 30, 60, 210);
    add(10, 250, 250, 250);
    final p = ArtColor.palette(px);
    expect(p.length, 3);
    expect(p[0], const Color(0xFFDC1E28));
    expect(p[1], const Color(0xFF1E3CD2));
  });

  testWidgets('backdrop animates while active, rings on beats, and holds still when paused', (tester) async {
    final level = ValueNotifier<double>(0.1);
    final key = GlobalKey();
    Widget app(bool active, List<Color> colors) => MaterialApp(
          home: RepaintBoundary(key: key, child: SizedBox(width: 400, height: 800, child: MusicBackdrop(colors: colors, level: level, active: active))),
        );
    await tester.pumpWidget(app(true, const [Color(0xFFE0313F), Color(0xFF3050E0), Color(0xFFF0C040)]));
    for (var i = 0; i < 30; i++) {
      level.value = i % 10 == 0 ? 0.95 : 0.15; // beats
      await tester.pump(const Duration(milliseconds: 33));
    }
    expect(tester.takeException(), isNull);
    // Save a frame to look at (not compared: just for checking by eye).
    final out = Platform.environment['HARP_BACKDROP_PNG'];
    if (out != null) {
      await tester.runAsync(() async {
        final b = key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
        final img = await b.toImage();
        final data = await img.toByteData(format: ui.ImageByteFormat.png);
        File(out).writeAsBytesSync(data!.buffer.asUint8List());
      });
    }
    // A new song's colours, then pause: no ticking left behind.
    await tester.pumpWidget(app(true, const [Color(0xFF20C080)]));
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pumpWidget(app(false, const [Color(0xFF20C080)]));
    await tester.pump(); // the last, still frame
    expect(tester.binding.hasScheduledFrame, isFalse);
    level.dispose();
  });
}
