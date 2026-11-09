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

  static final Map<String, List<Color>> _palettes = {};

  /// Up to [n] distinct colours of the artwork, most prominent first, for the
  /// player's moving background. Empty when the image can't be read.
  static Future<List<Color>> paletteOf(String path, {int n = 4}) async {
    final hit = _palettes[path];
    if (hit != null) return hit;
    try {
      final bytes = await File(path).readAsBytes();
      final codec = await ui.instantiateImageCodec(bytes, targetWidth: 32, targetHeight: 32);
      final frame = await codec.getNextFrame();
      final data = await frame.image.toByteData(format: ui.ImageByteFormat.rawRgba);
      frame.image.dispose();
      if (data == null) return const [];
      return _palettes[path] = palette(data.buffer.asUint8List(), n: n);
    } catch (_) {
      return const [];
    }
  }

  /// [rgba]: raw pixels. Groups pixels by hue (12 slices, plus one for greys),
  /// weighs each by how much of the picture it covers and how vivid it is, and
  /// returns each top group's average colour. Exposed for tests.
  static List<Color> palette(List<int> rgba, {int n = 4}) {
    final sums = List.generate(13, (_) => [0.0, 0.0, 0.0, 0.0, 0.0]); // r g b count weight
    for (var i = 0; i + 3 < rgba.length; i += 4) {
      if (rgba[i + 3] < 128) continue;
      final hsl = HSLColor.fromColor(Color.fromARGB(255, rgba[i], rgba[i + 1], rgba[i + 2]));
      final grey = hsl.saturation < 0.18 || hsl.lightness < 0.08 || hsl.lightness > 0.94;
      final bin = grey ? 12 : (hsl.hue / 30).floor() % 12;
      final vivid = grey ? 0.15 : 0.35 + hsl.saturation * (1 - (hsl.lightness - 0.5).abs() * 1.4).clamp(0.0, 1.0);
      final b = sums[bin];
      b[0] += rgba[i];
      b[1] += rgba[i + 1];
      b[2] += rgba[i + 2];
      b[3] += 1;
      b[4] += vivid;
    }
    final order = [for (var i = 0; i < 13; i++) if (sums[i][3] > 0) i]..sort((a, b) => sums[b][4].compareTo(sums[a][4]));
    return [
      for (final i in order.take(n))
        Color.fromARGB(255, (sums[i][0] / sums[i][3]).round(), (sums[i][1] / sums[i][3]).round(), (sums[i][2] / sums[i][3]).round()),
    ];
  }

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
