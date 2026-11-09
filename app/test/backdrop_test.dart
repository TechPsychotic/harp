import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harp/theme/art_color.dart';
import 'package:harp/theme/spectrum_waves.dart';

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

  testWidgets('spectrum waves follow the bands, and hold still when paused', (tester) async {
    final bands = ValueNotifier<List<double>>(const []);
    final key = GlobalKey();
    Widget app(bool active) => MaterialApp(
          home: RepaintBoundary(
            key: key,
            child: Container(
              color: const Color(0xFF301018),
              width: 400,
              height: 800,
              child: SpectrumWaves(bands: bands, colors: const [Color(0xFFE0313F), Color(0xFF3050E0), Color(0xFFF0C040)], active: active),
            ),
          ),
        );
    await tester.pumpWidget(app(true));
    for (var i = 0; i < 20; i++) {
      // A bass-heavy moment: low bands high, highs lower.
      // A realistic shape: strong bass falling off towards the highs, with some texture.
      bands.value = [for (var b = 0; b < 24; b++) ((0.85 - b * 0.03) + 0.08 * (b % 3 == 0 ? 1 : -1)).clamp(0.0, 1.0) * (i.isEven ? 1 : 0.85)];
      await tester.pump(const Duration(milliseconds: 33));
    }
    expect(tester.takeException(), isNull);
    final out = Platform.environment['HARP_BACKDROP_PNG'];
    if (out != null) {
      await tester.runAsync(() async {
        final b = key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
        final img = await b.toImage();
        final data = await img.toByteData(format: ui.ImageByteFormat.png);
        File(out).writeAsBytesSync(data!.buffer.asUint8List());
      });
    }
    await tester.pumpWidget(app(false));
    await tester.pump();
    expect(tester.binding.hasScheduledFrame, isFalse, reason: 'nothing redraws while paused');
    bands.dispose();
  });
}
