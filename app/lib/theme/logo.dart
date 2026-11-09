import 'dart:math' as math;

import 'package:flutter/foundation.dart';
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
///
/// [draw] (0–1) traces the pen strokes in; [pluck] (0–1) and [phase] make the
/// strike line vibrate like a plucked harp string.
class HarpLogoPainter extends CustomPainter {
  HarpLogoPainter({this.draw = 1, this.pluck = 0, this.phase = 0, this.glow = 0});
  final double draw;
  final double pluck;
  final double phase;
  final double glow;

  /// The first [t] of a path, for the stroke-by-stroke draw-in.
  static Path _part(Path p, double t) {
    if (t >= 1) return p;
    final out = Path();
    if (t <= 0) return out;
    for (final m in p.computeMetrics()) {
      out.addPath(m.extractPath(0, m.length * t), Offset.zero);
    }
    return out;
  }

  @override
  void paint(Canvas canvas, Size size) {
    final s = size.width;
    if (glow > 0) {
      canvas.drawRRect(
        RRect.fromRectAndRadius((Offset.zero & size).inflate(s * 0.04 * glow), Radius.circular(s * 0.28)),
        Paint()
          ..color = HarpColors.harpRed.withValues(alpha: 0.45 * glow)
          ..maskFilter = MaskFilter.blur(BlurStyle.normal, s * 0.18),
      );
    }
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
    // The stem, then the arch, then the strike: each takes its share of [draw].
    double seg(double a, double b) => ((draw - a) / (b - a)).clamp(0.0, 1.0);
    canvas.drawPath(_part(stem, seg(0, 0.45)), pen);
    canvas.drawPath(_part(arch, seg(0.4, 0.85)), pen);
    final strike = seg(0.8, 1);
    if (strike > 0) {
      final a = o(372, 415), b = o(372 + 188 * strike, 415);
      final bend = pluck * 46 * k * math.sin(phase * math.pi * 2);
      final line = Path()
        ..moveTo(a.dx, a.dy)
        ..quadraticBezierTo((a.dx + b.dx) / 2, a.dy + bend, b.dx, b.dy);
      canvas.drawPath(line, pen..strokeWidth = 40 * k);
    }
  }

  @override
  bool shouldRepaint(HarpLogoPainter old) => old.draw != draw || old.pluck != pluck || old.phase != phase || old.glow != glow;
}

/// The logo on the home page. It writes itself in when it first shows, and
/// while music plays its strike line quivers like a plucked string, harder on
/// louder moments. Still when nothing plays or effects are off.
class AnimatedHarpLogo extends StatefulWidget {
  const AnimatedHarpLogo({super.key, this.size = 36, required this.playing, this.level, this.animate = true});
  final double size;
  final bool playing;

  /// Loudness, 0–1 (from the visualizer).
  final ValueListenable<double>? level;
  final bool animate;

  @override
  State<AnimatedHarpLogo> createState() => _AnimatedHarpLogoState();
}

class _AnimatedHarpLogoState extends State<AnimatedHarpLogo> with TickerProviderStateMixin {
  late final AnimationController _draw = AnimationController(vsync: this, duration: const Duration(milliseconds: 1400));
  late final AnimationController _string = AnimationController(vsync: this, duration: const Duration(milliseconds: 180));
  double _amp = 0;

  @override
  void initState() {
    super.initState();
    if (widget.animate) {
      _draw.forward();
    } else {
      _draw.value = 1;
    }
    _sync();
  }

  @override
  void didUpdateWidget(AnimatedHarpLogo old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    if (widget.playing && widget.animate) {
      if (!_string.isAnimating) _string.repeat();
    } else {
      _string.stop();
      _amp = 0; // rest straight, not frozen mid-quiver
    }
  }

  @override
  void dispose() {
    _draw.dispose();
    _string.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return RepaintBoundary(
      child: SizedBox(
        width: widget.size,
        height: widget.size,
        child: AnimatedBuilder(
          animation: Listenable.merge([_draw, _string]),
          builder: (_, _) {
            final on = widget.playing && widget.animate;
            final lvl = on ? (widget.level?.value ?? 0.4).clamp(0.0, 1.0) : 0.0;
            // Ease towards the loudness so the string settles instead of snapping.
            _amp += ((on ? 0.25 + 0.75 * lvl : 0) - _amp) * 0.2;
            return CustomPaint(
              painter: HarpLogoPainter(
                draw: Curves.easeInOutCubic.transform(_draw.value),
                pluck: _amp,
                phase: _string.value,
                glow: _amp * 0.8,
              ),
            );
          },
        ),
      ),
    );
  }
}
