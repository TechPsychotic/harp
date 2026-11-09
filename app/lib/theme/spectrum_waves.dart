import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

/// Waves along the bottom of Now Playing, shaped by the music's live spectrum:
/// bass lifts the middle, treble the sides, so a kick drum swells the centre
/// and cymbals ripple the edges. Three layers in the cover's colours drift
/// slowly over each other, each with a soft bright crest.
///
/// When [active] is false (paused, or effects turned down) the waves hold
/// still at a low, calm shape and nothing is redrawn.
class SpectrumWaves extends StatefulWidget {
  const SpectrumWaves({super.key, required this.bands, required this.colors, required this.active});

  /// Spectrum bands, low to high, 0..1 (may be empty).
  final ValueListenable<List<double>> bands;

  /// Colours from the artwork (at least one).
  final List<Color> colors;
  final bool active;

  @override
  State<SpectrumWaves> createState() => _SpectrumWavesState();
}

class _SpectrumWavesState extends State<SpectrumWaves> with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_tick);
  final _phase = ValueNotifier<double>(0);
  Duration _last = Duration.zero;

  @override
  void initState() {
    super.initState();
    _sync();
  }

  @override
  void didUpdateWidget(SpectrumWaves old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    if (widget.active && !_ticker.isActive) {
      _last = Duration.zero;
      _ticker.start();
    } else if (!widget.active && _ticker.isActive) {
      _ticker.stop();
    }
  }

  void _tick(Duration t) {
    // 30 frames a second: smooth for slow waves, half the work of 60.
    if (_last != Duration.zero && t - _last < const Duration(milliseconds: 30)) return;
    final dt = _last == Duration.zero ? 0.033 : (t - _last).inMicroseconds / 1e6;
    _last = t;
    _phase.value += dt;
  }

  @override
  void dispose() {
    _ticker.dispose();
    _phase.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RepaintBoundary(
        child: CustomPaint(
          painter: _WavesPainter(phase: _phase, bands: widget.bands, colors: widget.colors, active: widget.active),
          size: Size.infinite,
        ),
      );
}

class _WavesPainter extends CustomPainter {
  _WavesPainter({required this.phase, required this.bands, required this.colors, required this.active}) : super(repaint: phase);
  final ValueNotifier<double> phase;
  final ValueListenable<List<double>> bands;
  final List<Color> colors;
  final bool active;

  /// The spectrum at position x (0..1 across the screen): mirrored, so the
  /// lowest band sits in the middle and the highest at both edges.
  /// Neighbouring bands blended a few times, so the waves roll instead of stepping.
  static List<double> smooth(List<double> b) {
    var v = b;
    for (var k = 0; k < 3; k++) {
      v = [
        for (var i = 0; i < v.length; i++) 0.25 * v[math.max(0, i - 1)] + 0.5 * v[i] + 0.25 * v[math.min(v.length - 1, i + 1)],
      ];
    }
    return v;
  }

  static double at(List<double> b, double x) {
    if (b.isEmpty) return 0;
    final d = (x - 0.5).abs() * 2; // 0 middle .. 1 edge
    final f = d * (b.length - 1);
    final i = f.floor().clamp(0, b.length - 1);
    final j = math.min(i + 1, b.length - 1);
    final t = f - i;
    // Highs are naturally quieter: lift them a little so the edges move too.
    return (b[i] * (1 - t) + b[j] * t) * (1 + 0.6 * d);
  }

  Color _layerColor(int i) {
    final c = colors.isEmpty ? Colors.white : colors[i % colors.length];
    final hsl = HSLColor.fromColor(c);
    return hsl.withLightness(hsl.lightness.clamp(0.55, 0.75)).withSaturation(hsl.saturation.clamp(0.0, 0.8)).toColor();
  }

  @override
  void paint(Canvas canvas, Size size) {
    final b = active ? smooth(bands.value) : const <double>[];
    final t = phase.value;
    final w = size.width, h = size.height;
    const n = 48;
    for (var layer = 2; layer >= 0; layer--) {
      // Back layers sit higher and reach further, front ones lower: depth.
      final base = h * (0.86 - layer * 0.045);
      final reach = h * 0.20 * (1 + layer * 0.2); // how high the music can lift it
      final pts = <Offset>[];
      for (var i = 0; i <= n; i++) {
        final x = i / n;
        final s = at(b, x);
        // A slow swell that drifts sideways, different per layer, plus the music.
        final drift = math.sin(x * (2.2 + layer * 0.9) * math.pi + t * (0.6 + layer * 0.35) + layer * 1.7);
        final y = base - h * 0.012 * (1 + drift) - reach * s * (0.8 + 0.2 * drift);
        pts.add(Offset(x * w, y));
      }
      // A smooth curve through the points (midpoint quadratics).
      final crest = Path()..moveTo(pts.first.dx, pts.first.dy);
      for (var i = 1; i < pts.length; i++) {
        final m = Offset((pts[i - 1].dx + pts[i].dx) / 2, (pts[i - 1].dy + pts[i].dy) / 2);
        crest.quadraticBezierTo(pts[i - 1].dx, pts[i - 1].dy, m.dx, m.dy);
      }
      crest.lineTo(pts.last.dx, pts.last.dy);
      final fill = Path.from(crest)
        ..lineTo(w, h)
        ..lineTo(0, h)
        ..close();
      final c = _layerColor(layer);
      final top = pts.map((p) => p.dy).reduce(math.min);
      canvas.drawPath(
        fill,
        Paint()
          ..shader = LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [c.withValues(alpha: 0.34 - layer * 0.08), c.withValues(alpha: 0.10)],
          ).createShader(Rect.fromLTRB(0, top, w, h)),
      );
      // The bright crest line.
      canvas.drawPath(
        crest,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = layer == 0 ? 2 : 1.2
          ..color = Color.lerp(c, Colors.white, 0.4)!.withValues(alpha: 0.55 - layer * 0.15),
      );
    }
  }

  @override
  bool shouldRepaint(_WavesPainter old) => old.colors != colors || old.active != active || old.bands != bands;
}
