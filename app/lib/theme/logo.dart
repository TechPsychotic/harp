import 'package:flutter/material.dart';

import 'glass.dart';

/// Harp's mark: a small cursive "h" with a strike through its stem, in white on a red gradient tile.
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
    // A cursive "h" with a strike through its stem (like an h-bar). Coordinates are
    // in a 1024 box, the same curves scripts/make-icons.sh draws for the app icons.
    final k = s / 1024;
    Offset o(double x, double y) => Offset(x * k, y * k);
    final stem = Path()
      ..moveTo(340 * k, 320 * k)
      ..cubicTo(380 * k, 240 * k, 450 * k, 210 * k, 470 * k, 260 * k)
      ..cubicTo(490 * k, 320 * k, 410 * k, 600 * k, 372 * k, 790 * k);
    final arch = Path()
      ..moveTo(410 * k, 598 * k)
      ..cubicTo(445 * k, 505 * k, 590 * k, 420 * k, 620 * k, 560 * k)
      ..cubicTo(640 * k, 660 * k, 590 * k, 740 * k, 610 * k, 780 * k)
      ..cubicTo(625 * k, 810 * k, 680 * k, 780 * k, 720 * k, 730 * k);
    final pen = Paint()
      ..color = Colors.white
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..strokeWidth = 62 * k;
    canvas.drawPath(stem, pen);
    canvas.drawPath(arch, pen);
    canvas.drawLine(o(372, 415), o(560, 415), pen..strokeWidth = 40 * k);
  }

  @override
  bool shouldRepaint(HarpLogoPainter old) => false;
}
