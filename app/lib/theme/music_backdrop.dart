import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

/// The Now Playing background: the artwork's own colours as large, soft
/// clouds of light that drift and blend into each other, like Apple Music's
/// animated backgrounds, and react to the music:
///
/// * the clouds swell and brighten with the loudness, and drift faster when
///   the music has more energy;
/// * a sudden jump in loudness (a beat, a drop) sends a soft ring of light out
///   from behind the artwork.
///
/// When [active] is false it holds still (paused, or effects turned down), so
/// it costs nothing per frame. New songs fade from the old colours to the new.
class MusicBackdrop extends StatefulWidget {
  const MusicBackdrop({super.key, required this.colors, required this.level, required this.active, this.origin = const Alignment(0, -0.3)});

  /// Colours from the artwork, most prominent first (at least one).
  final List<Color> colors;

  /// Loudness, 0–1.
  final ValueListenable<double> level;
  final bool active;

  /// Where beat rings start (behind the artwork).
  final Alignment origin;

  @override
  State<MusicBackdrop> createState() => _MusicBackdropState();
}

class _Ring {
  _Ring(this.born, this.strength);
  final double born;
  final double strength;
}

/// What the painter reads each frame.
class _Frame extends ChangeNotifier {
  double phase = 0; // drift, advanced faster when the music is louder
  double time = 0; // seconds, for rings
  double level = 0; // smoothed loudness
  double mix = 1; // 0 = old colours, 1 = new
  List<Color> from = const [];
  List<Color> to = const [];
  final List<_Ring> rings = [];
  void tick() => notifyListeners();
}

class _MusicBackdropState extends State<MusicBackdrop> with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_onTick);
  final _f = _Frame();
  Duration _last = Duration.zero;
  double _avg = 0;
  double _lastBeat = -10;

  @override
  void initState() {
    super.initState();
    _f.to = _f.from = _norm(widget.colors);
    _sync();
  }

  @override
  void didUpdateWidget(MusicBackdrop old) {
    super.didUpdateWidget(old);
    if (!listEquals(old.colors, widget.colors)) {
      _f.from = _blend(_f.from, _f.to, _f.mix);
      _f.to = _norm(widget.colors);
      _f.mix = 0;
      if (!_ticker.isActive) {
        // Paused: no animation, so just switch.
        _f.mix = 1;
        _f.tick();
      }
    }
    _sync();
  }

  void _sync() {
    if (widget.active && !_ticker.isActive) {
      _last = Duration.zero;
      _ticker.start();
    } else if (!widget.active && _ticker.isActive) {
      _ticker.stop();
      _f.level = 0;
      _f.rings.clear();
      _f.mix = 1;
      _f.tick();
    }
  }

  void _onTick(Duration elapsed) {
    final dt = _last == Duration.zero ? 1 / 60 : ((elapsed - _last).inMicroseconds / 1e6).clamp(0.0, 0.1);
    _last = elapsed;
    final raw = widget.level.value.clamp(0.0, 1.0);
    final f = _f;
    f.time += dt;
    f.level += (raw - f.level) * (raw > f.level ? 0.35 : 0.08);
    _avg += (raw - _avg) * 0.03;
    f.phase += dt * (0.18 + 0.75 * f.level);
    if (f.mix < 1) f.mix = (f.mix + dt / 1.4).clamp(0.0, 1.0);
    // A beat: loudness jumps well above its recent average.
    if (raw - _avg > 0.16 && f.time - _lastBeat > 0.32) {
      _lastBeat = f.time;
      f.rings.add(_Ring(f.time, ((raw - _avg) * 3).clamp(0.4, 1.0)));
      if (f.rings.length > 4) f.rings.removeAt(0);
    }
    f.rings.removeWhere((r) => f.time - r.born > _ringLife);
    f.tick();
  }

  @override
  void dispose() {
    _ticker.dispose();
    _f.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RepaintBoundary(
        child: CustomPaint(painter: _BackdropPainter(_f, widget.origin), size: Size.infinite, isComplex: true, willChange: widget.active),
      );
}

const _ringLife = 1.8;

/// Five colours, each tuned to glow on a dark background.
List<Color> _norm(List<Color> c) {
  final base = c.isEmpty ? const [Color(0xFFE0313F)] : c;
  return [
    for (var i = 0; i < 5; i++)
      () {
        final hsl = HSLColor.fromColor(base[i % base.length]);
        // Repeated colours are nudged round the colour wheel so the clouds differ.
        final shift = i >= base.length ? (i - base.length + 1) * 18.0 : 0.0;
        final greyish = hsl.saturation < 0.15;
        return hsl
            .withHue((hsl.hue + shift) % 360)
            .withSaturation(greyish ? hsl.saturation : (hsl.saturation * 1.15).clamp(0.45, 0.95))
            .withLightness(hsl.lightness.clamp(0.32, 0.58))
            .toColor();
      }(),
  ];
}

List<Color> _blend(List<Color> a, List<Color> b, double t) =>
    a.isEmpty ? b : [for (var i = 0; i < b.length; i++) Color.lerp(a[i % a.length], b[i], t)!];

class _BackdropPainter extends CustomPainter {
  _BackdropPainter(this.f, this.origin) : super(repaint: f);
  final _Frame f;
  final Alignment origin;

  // Each cloud's path: speeds and offsets on two axes (a slow Lissajous drift).
  static const _paths = [
    (0.31, 0.23, 0.0, 1.3, 0.30, 0.22),
    (0.19, 0.29, 2.1, 0.4, 0.36, 0.30),
    (0.25, 0.17, 4.2, 2.6, 0.28, 0.34),
    (0.14, 0.21, 1.1, 3.9, 0.40, 0.26),
    (0.22, 0.33, 5.3, 0.9, 0.32, 0.38),
  ];

  @override
  void paint(Canvas canvas, Size size) {
    final colors = _blend(f.from, f.to, f.mix);
    final lv = f.level;
    final rect = Offset.zero & size;
    final long = size.longestSide;
    final deep = HSLColor.fromColor(colors.first).withLightness(0.07).withSaturation(0.5).toColor();
    canvas.drawRect(rect, Paint()..color = deep);

    for (var i = 0; i < _paths.length; i++) {
      final (sx, sy, ox, oy, ax, ay) = _paths[i];
      final p = f.phase;
      final c = Offset(
        size.width * (0.5 + ax * math.sin(p * sx * 2 + ox) + 0.08 * math.sin(p * 0.7 + i)),
        size.height * (0.42 + ay * math.cos(p * sy * 2 + oy)),
      );
      final r = long * (0.42 + 0.06 * math.sin(p * 0.9 + i * 1.7)) * (1 + 0.28 * lv * (i.isEven ? 1 : 0.7));
      final a = (0.55 + 0.35 * lv) * (i == 0 ? 1.0 : 0.85);
      canvas.drawCircle(
        c,
        r,
        Paint()
          ..blendMode = BlendMode.screen
          ..shader = RadialGradient(
            colors: [colors[i].withValues(alpha: a), colors[i].withValues(alpha: a * 0.35), colors[i].withValues(alpha: 0)],
            stops: const [0, 0.45, 1],
          ).createShader(Rect.fromCircle(center: c, radius: r)),
      );
    }

    // Beat rings from behind the artwork.
    final o = origin.alongSize(size);
    for (final ring in f.rings) {
      final t = ((f.time - ring.born) / _ringLife).clamp(0.0, 1.0);
      final ease = 1 - math.pow(1 - t, 3).toDouble();
      final fade = math.pow(1 - t, 2).toDouble();
      final radius = long * (0.12 + 0.7 * ease);
      canvas.drawCircle(
        o,
        radius,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = long * (0.035 * (1 - t) + 0.008)
          ..blendMode = BlendMode.screen
          ..color = Color.lerp(Colors.white, colors[1], 0.35)!.withValues(alpha: 0.28 * fade * ring.strength)
          ..maskFilter = MaskFilter.blur(BlurStyle.normal, long * 0.018),
      );
    }

    // Darker towards the bottom, so the controls always read clearly.
    canvas.drawRect(
      rect,
      Paint()
        ..shader = const LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [Color(0x33000000), Color(0x00000000), Color(0x99000000)],
          stops: [0, 0.35, 1],
        ).createShader(rect),
    );
  }

  @override
  bool shouldRepaint(_BackdropPainter old) => old.f != f || old.origin != origin;
}
