import 'package:flutter/material.dart';

import 'glass.dart';

/// Harp's mark: a small harp-shaped "H" in white on a red gradient tile.
class HarpLogo extends StatelessWidget {
  const HarpLogo({super.key, this.size = 40});
  final double size;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: size,
      height: size,
      child: CustomPaint(painter: HarpLogoPainter()),
    );
  }
}

/// Also used to draw the app icons (see `scripts/make-icons.sh`, same geometry).
class HarpLogoPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final s = size.width;
    final tile = RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(s * 0.24));
    canvas.drawRRect(
      tile,
      Paint()
        ..shader = const LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [HarpColors.harpRedLight, HarpColors.harpRedDark],
        ).createShader(Offset.zero & size),
    );
    final white = Paint()..color = Colors.white;
    // Two pillars...
    for (final x in [0.289, 0.633]) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(Rect.fromLTWH(s * x, s * 0.230, s * 0.078, s * 0.540), Radius.circular(s * 0.039)),
        white,
      );
    }
    // ...and five thin strings across, like a harp's.
    for (final y in [0.344, 0.421, 0.493, 0.565, 0.643]) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(Rect.fromLTWH(s * 0.367, s * y, s * 0.266, s * 0.0137), Radius.circular(s * 0.007)),
        white,
      );
    }
  }

  @override
  bool shouldRepaint(HarpLogoPainter old) => false;
}
