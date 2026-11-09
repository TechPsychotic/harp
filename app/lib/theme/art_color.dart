import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';

/// The main colour of a piece of artwork, the way music apps tint their screens.
///
/// The cover is decoded at 24×24 pixels (cheap) and the most vivid colour wins:
/// each pixel counts by how saturated it is and how far it is from black or white.
class ArtColor {
  static final Map<String, Color?> _cache = {};

  static Color? peek(String path) => _cache[path];

  static Future<Color?> of(String path) async {
    if (_cache.containsKey(path)) return _cache[path];
    try {
      final bytes = await File(path).readAsBytes();
      final codec = await ui.instantiateImageCodec(bytes, targetWidth: 24, targetHeight: 24);
      final frame = await codec.getNextFrame();
      final data = await frame.image.toByteData(format: ui.ImageByteFormat.rawRgba);
      frame.image.dispose();
      if (data == null) return _cache[path] = null;
      return _cache[path] = dominant(data.buffer.asUint8List());
    } catch (_) {
      return _cache[path] = null;
    }
  }

  /// [rgba]: raw pixels. Exposed for tests.
  static Color dominant(List<int> rgba) {
    double r = 0, g = 0, b = 0, w = 0;
    double ar = 0, ag = 0, ab = 0;
    var n = 0;
    for (var i = 0; i + 3 < rgba.length; i += 4) {
      if (rgba[i + 3] < 128) continue;
      final c = Color.fromARGB(255, rgba[i], rgba[i + 1], rgba[i + 2]);
      final hsl = HSLColor.fromColor(c);
      ar += rgba[i];
      ag += rgba[i + 1];
      ab += rgba[i + 2];
      n++;
      final vivid = hsl.saturation * (1 - (hsl.lightness - 0.5).abs() * 1.6).clamp(0.0, 1.0);
      final weight = vivid * vivid;
      r += rgba[i] * weight;
      g += rgba[i + 1] * weight;
      b += rgba[i + 2] * weight;
      w += weight;
    }
    if (n == 0) return const Color(0xFF808080);
    // A grey or black-and-white cover: use its average colour.
    if (w < n * 0.01) return Color.fromARGB(255, (ar / n).round(), (ag / n).round(), (ab / n).round());
    return Color.fromARGB(255, (r / w).round(), (g / w).round(), (b / w).round());
  }
}
