import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

/// Soft waves along the bottom of Now Playing that rise with the music (its
/// real loudness, from the engine's spectrum) and roll gently.
///
/// When [active] is false (paused, or effects turned down) the waves hold
/// still at a low, calm shape and nothing is redrawn.
class SpectrumWaves extends StatefulWidget {
  const SpectrumWaves({super.key, required this.bands, required this.level, required this.color, required this.active});

  /// Spectrum bands, low to high, 0..1 (may be empty).
  final ValueListenable<List<double>> bands;

  /// Loudness, 0..1.
  final ValueListenable<double> level;

  /// The waves' colour (white on dark, a deep tint of the cover on light).
  final Color color;
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
          painter: _WavesPainter(phase: _phase, bands: widget.bands, level: widget.level, color: widget.color, active: widget.active),
          size: Size.infinite,
        ),
      );
}

class _WavesPainter extends CustomPainter {
  _WavesPainter({required this.phase, required this.bands, required this.level, required this.color, required this.active})
      : super(repaint: phase);
  final ValueNotifier<double> phase;
  final ValueListenable<List<double>> bands;
  final ValueListenable<double> level;
  final Color color;
  final bool active;

  /// Neighbouring bands blended a few times, so the shape rolls instead of stepping.
  static List<double> smooth(List<double> b) {
    var v = b;
    for (var k = 0; k < 3; k++) {
      v = [
        for (var i = 0; i < v.length; i++) 0.25 * v[math.max(0, i - 1)] + 0.5 * v[i] + 0.25 * v[math.min(v.length - 1, i + 1)],
      ];
    }
    return v;
  }

  /// The spectrum at x (0..1 across): mirrored, lowest band in the middle.
  static double at(List<double> b, double x) {
    if (b.isEmpty) return 0;
    final d = (x - 0.5).abs() * 2;
    final f = d * (b.length - 1);
    final i = f.floor().clamp(0, b.length - 1);
    final j = math.min(i + 1, b.length - 1);
    final t = f - i;
    return (b[i] * (1 - t) + b[j] * t) * (1 + 0.6 * d);
  }

  /// The original soft waves (three translucent layers of rolling sine
  /// shapes), now lifted by the song's real loudness, with the spectrum
  /// adding a little extra where the energy is.
  @override
  void paint(Canvas canvas, Size size) {
    final lv = active ? level.value.clamp(0.0, 1.0) : 0.0;
    final b = active ? smooth(bands.value) : const <double>[];
    final t = phase.value;
    final baseY = size.height * 0.84;
    for (var layer = 0; layer < 3; layer++) {
      final amp = (6 + 46 * lv) * (1 - layer * 0.25);
      final path = Path()..moveTo(0, size.height);
      const steps = 40;
      for (var i = 0; i <= steps; i++) {
        final x = i / steps;
        final ph = x * (2.2 + layer) * math.pi + t * (1.2 + layer * 0.7);
        final shape = 0.75 + 0.5 * at(b, x); // the spectrum's touch: subtle
        path.lineTo(size.width * x, baseY + layer * 10 - amp * shape * (0.55 * math.sin(ph) + 0.45 * math.sin(ph * 1.9 + layer)));
      }
      path
        ..lineTo(size.width, size.height)
        ..close();
      canvas.drawPath(path, Paint()..color = color.withValues(alpha: 0.16 - layer * 0.04));
    }
  }

  @override
  bool shouldRepaint(_WavesPainter old) => old.color != color || old.active != active || old.bands != bands || old.level != level;
}
