import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:harp/theme/art_color.dart';
import 'package:harp/theme/glass.dart';

List<int> pixels(List<(int, int, int)> colors, {int each = 10}) => [
      for (final (r, g, b) in colors)
        for (var i = 0; i < each; i++) ...[r, g, b, 255]
    ];

void main() {
  test('the vivid colour wins over black, white and grey', () {
    final c = ArtColor.dominant(pixels([(0, 0, 0), (255, 255, 255), (128, 128, 128), (220, 30, 40)]));
    final hsl = HSLColor.fromColor(c);
    expect(hsl.hue, anyOf(lessThan(15), greaterThan(345)), reason: 'red');
    expect(hsl.saturation, greaterThan(0.6));
  });

  test('a black-and-white cover falls back to its average', () {
    final c = ArtColor.dominant(pixels([(10, 10, 10), (240, 240, 240)]));
    expect(HSLColor.fromColor(c).saturation, lessThan(0.05));
  });

  test('transparent pixels are ignored; an empty image is grey', () {
    expect(ArtColor.dominant([0, 0, 0, 0]), const Color(0xFF808080));
  });

  test('accents stay readable on both themes', () {
    final dark = HSLColor.fromColor(HarpColors.usableAccent(const Color(0xFF101010), Brightness.dark));
    expect(dark.lightness, greaterThanOrEqualTo(0.53), reason: "clamped to 0.55, less 8-bit rounding");
    final light = HSLColor.fromColor(HarpColors.usableAccent(const Color(0xFFFFE0E0), Brightness.light));
    expect(light.lightness, lessThanOrEqualTo(0.52), reason: "clamped to 0.5, plus 8-bit rounding");
  });
}
