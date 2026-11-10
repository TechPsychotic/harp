import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harp/widgets/blurred_art.dart';

/// A picture with sharp, colourful detail (what a cover has).
Future<ui.Image> _pattern() {
  final rec = ui.PictureRecorder();
  final c = Canvas(rec);
  for (var y = 0; y < 8; y++) {
    for (var x = 0; x < 8; x++) {
      c.drawRect(Rect.fromLTWH(x * 32.0, y * 32.0, 32, 32), Paint()..color = HSLColor.fromAHSL(1, (x * 45 + y * 20) % 360.0, 0.8, (x + y).isEven ? 0.35 : 0.65).toColor());
    }
  }
  return rec.endRecording().toImage(256, 256);
}

Future<ui.Image> _upscale(ui.Image img, int w, int h) {
  final rec = ui.PictureRecorder();
  paintImage(canvas: Canvas(rec), rect: Rect.fromLTWH(0, 0, w.toDouble(), h.toDouble()), image: img, fit: BoxFit.fill, filterQuality: FilterQuality.medium);
  return rec.endRecording().toImage(w, h);
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  test('the quick blur looks like the full one', () async {
    const size = Size(800, 1200);
    final src = await _pattern();
    final full = await BlurredArt.blur(src, size, 60, 1); // what the old live blur produced
    final quick = await _upscale(await BlurredArt.blur(src, size, 60, 4), 800, 1200);
    final a = (await full.toByteData())!.buffer.asUint8List(), b = (await quick.toByteData())!.buffer.asUint8List();
    var diff = 0.0;
    for (var i = 0; i < a.length; i++) {
      diff += (a[i] - b[i]).abs();
    }
    final mean = diff / a.length;
    final out = Platform.environment['HARP_OUT'];
    if (out != null) {
      for (final (name, img) in [('full', full), ('quick', quick)]) {
        File('$out-$name.png').writeAsBytesSync((await img.toByteData(format: ui.ImageByteFormat.png))!.buffer.asUint8List());
      }
    }
    expect(mean, lessThan(2.0), reason: 'average difference per colour channel, out of 255');
  });
}
