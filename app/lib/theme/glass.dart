import 'dart:math' as math;
import 'dart:ui';

import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart';

/// Corner radii. Kept modest: this look is glass, not pills.
class Radii {
  static const double card = 14;
  static const double tile = 12;
  static const double cover = 8;
  static const double control = 12;
  static const double sheet = 18;
  static const double chip = 10;
}

/// Colors that differ between the dark and light themes.
@immutable
class HarpTokens extends ThemeExtension<HarpTokens> {
  const HarpTokens({
    required this.ink,
    required this.text,
    required this.textDim,
    required this.textFaint,
    required this.glass,
    required this.glassEdge,
    required this.shadow,
    required this.orbAlpha,
  });

  final Color ink; // page background
  final Color text;
  final Color textDim;
  final Color textFaint;
  final Color glass; // tint of frosted surfaces
  final Color glassEdge;
  final Color shadow;
  final double orbAlpha;

  static const dark = HarpTokens(
    ink: Color(0xFF07070D),
    text: Color(0xFFF4F3FA),
    textDim: Color(0xB3F4F3FA),
    textFaint: Color(0x80F4F3FA),
    glass: Colors.white,
    glassEdge: Colors.white,
    shadow: Colors.black,
    orbAlpha: 0.42,
  );

  static const light = HarpTokens(
    ink: Color(0xFFF3F1FA),
    text: Color(0xFF14131C),
    textDim: Color(0xB314131C),
    textFaint: Color(0x8014131C),
    glass: Colors.white,
    glassEdge: Color(0xFF2B2A3A),
    shadow: Color(0xFF3A3560),
    orbAlpha: 0.30,
  );

  @override
  HarpTokens copyWith() => this;

  @override
  HarpTokens lerp(ThemeExtension<HarpTokens>? other, double t) => t < 0.5 ? this : (other as HarpTokens? ?? this);
}

extension HarpContext on BuildContext {
  HarpTokens get tok => Theme.of(this).extension<HarpTokens>() ?? HarpTokens.dark;
  bool get isDark => Theme.of(this).brightness == Brightness.dark;
}

class HarpColors {
  /// A stable accent hue for a song (no image decoding needed).
  static Color accentFor(String? seed) {
    if (seed == null || seed.isEmpty) return const Color(0xFFE5484D); // Harp red
    var h = 0;
    for (final c in seed.codeUnits) {
      h = (h * 31 + c) & 0x7fffffff;
    }
    return HSLColor.fromAHSL(1, (h % 360).toDouble(), 0.75, 0.58).toColor();
  }

  static const harpRedLight = Color(0xFFFF6B6B);
  static const harpRedDark = Color(0xFFB3001B);
}

ThemeData harpTheme(Color accent, Brightness b) {
  final tok = b == Brightness.dark ? HarpTokens.dark : HarpTokens.light;
  final scheme = ColorScheme.fromSeed(seedColor: accent, brightness: b, surface: tok.ink);
  return ThemeData(
    useMaterial3: true,
    brightness: b,
    colorScheme: scheme,
    scaffoldBackgroundColor: Colors.transparent,
    extensions: [tok],
    textTheme: (b == Brightness.dark ? ThemeData.dark() : ThemeData.light())
        .textTheme
        .apply(bodyColor: tok.text, displayColor: tok.text),
    splashFactory: InkSparkle.splashFactory,
    sliderTheme: SliderThemeData(
      trackHeight: 4,
      activeTrackColor: accent,
      inactiveTrackColor: tok.text.withValues(alpha: 0.16),
      thumbColor: b == Brightness.dark ? Colors.white : accent,
      overlayColor: accent.withValues(alpha: 0.18),
      thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
    ),
    dividerColor: Colors.transparent,
  );
}

/// Drifting colored orbs behind everything, with waves that rise with the music.
///
/// It repaints at ~20 fps and only while something is playing; when idle it
/// draws once and stops, so it costs nothing on a paused app.
class AuroraBackground extends StatefulWidget {
  const AuroraBackground({super.key, required this.accent, required this.child, this.level, this.active = false, this.flat = false});
  final Color accent;
  final Widget child;

  /// Loudness 0..1 of what is playing (see `Visualizer`).
  final ValueListenable<double>? level;

  /// Animate (a song is playing).
  final bool active;

  /// Plain background color only (fastest).
  final bool flat;

  @override
  State<AuroraBackground> createState() => _AuroraBackgroundState();
}

class _AuroraBackgroundState extends State<AuroraBackground> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(vsync: this, duration: const Duration(seconds: 60));
  final ValueNotifier<double> _t = ValueNotifier(0);
  double _lastPaint = 0;

  @override
  void initState() {
    super.initState();
    _c.addListener(() {
      // ~20 fps is plenty for slow-moving color and keeps weak GPUs cool.
      final v = _c.value * 60;
      if ((v - _lastPaint).abs() >= 0.05 || v < _lastPaint) {
        _lastPaint = v;
        _t.value = v;
      }
    });
    _sync();
  }

  @override
  void didUpdateWidget(AuroraBackground old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    final reduce = WidgetsBinding.instance.platformDispatcher.accessibilityFeatures.disableAnimations;
    if (widget.active && !reduce && !widget.flat) {
      if (!_c.isAnimating) _c.repeat();
    } else {
      _c.stop();
    }
  }

  @override
  void dispose() {
    _c.dispose();
    _t.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Stack(fit: StackFit.expand, children: [
      ColoredBox(color: tok.ink),
      if (!widget.flat)
        RepaintBoundary(
          child: CustomPaint(
            painter: _AuroraPainter(_t, widget.level, widget.accent, tok.orbAlpha, context.isDark),
          ),
        ),
      widget.child,
    ]);
  }
}

class _AuroraPainter extends CustomPainter {
  _AuroraPainter(this.t, this.level, this.accent, this.orbAlpha, this.dark)
      : super(repaint: Listenable.merge([t, ?level]));
  final ValueNotifier<double> t;
  final ValueListenable<double>? level;
  final Color accent;
  final double orbAlpha;
  final bool dark;

  @override
  void paint(Canvas canvas, Size size) {
    final time = t.value;
    final lv = (level?.value ?? 0).clamp(0.0, 1.0);
    final hsl = HSLColor.fromColor(accent);
    final orbs = <(Color, double, double, double)>[
      (accent, 0.2, 0.15, 0.55),
      (hsl.withHue((hsl.hue + 70) % 360).toColor(), 0.85, 0.3, 0.5),
      (hsl.withHue((hsl.hue + 200) % 360).toColor(), 0.4, 0.95, 0.6),
    ];
    final r = size.longestSide;
    for (var i = 0; i < orbs.length; i++) {
      final (color, x, y, rad) = orbs[i];
      final a = (time / 60 + i / orbs.length) * 2 * math.pi;
      final c = Offset(size.width * (x + 0.08 * math.cos(a)), size.height * (y + 0.08 * math.sin(a * 1.3)));
      // The orbs swell a little with the music.
      final radius = r * rad * (1 + 0.12 * lv);
      final paint = Paint()
        ..shader = RadialGradient(colors: [
          color.withValues(alpha: (orbAlpha + 0.18 * lv).clamp(0, 0.7)),
          color.withValues(alpha: 0),
        ]).createShader(Rect.fromCircle(center: c, radius: radius));
      canvas.drawCircle(c, radius, paint);
    }

    if (level == null) return;
    // Soft waves along the bottom whose height follows the loudness.
    final baseY = size.height * 0.86;
    for (var layer = 0; layer < 3; layer++) {
      final amp = (6 + 46 * lv) * (1 - layer * 0.25);
      final speed = 1.2 + layer * 0.7;
      final path = Path()..moveTo(0, size.height);
      const steps = 48;
      for (var i = 0; i <= steps; i++) {
        final x = size.width * i / steps;
        final ph = i / steps * (2.2 + layer) * math.pi + time * speed;
        final y = baseY + layer * 10 - amp * (0.55 * math.sin(ph) + 0.45 * math.sin(ph * 1.9 + layer));
        path.lineTo(x, y);
      }
      path
        ..lineTo(size.width, size.height)
        ..close();
      canvas.drawPath(
        path,
        Paint()..color = accent.withValues(alpha: (dark ? 0.16 : 0.12) - layer * 0.035),
      );
    }
  }

  @override
  bool shouldRepaint(_AuroraPainter old) => old.accent != accent || old.dark != dark || old.orbAlpha != orbAlpha;
}

/// A translucent panel with a light edge. **No blur**: cheap enough for lists.
class GlassCard extends StatelessWidget {
  const GlassCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(14),
    this.radius = Radii.card,
    this.tint,
    this.onTap,
    this.strength = 1,
  });
  final Widget child;
  final EdgeInsetsGeometry padding;
  final double radius;
  final Color? tint;
  final VoidCallback? onTap;
  final double strength;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final dark = context.isDark;
    final base = tint ?? tok.glass;
    final r = BorderRadius.circular(radius);
    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: r,
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [
            base.withValues(alpha: (dark ? 0.12 : 0.62) * strength),
            base.withValues(alpha: (dark ? 0.05 : 0.38) * strength),
          ],
        ),
        border: Border.all(color: tok.glassEdge.withValues(alpha: dark ? 0.16 : 0.10)),
      ),
      child: Material(
        type: MaterialType.transparency,
        child: InkWell(borderRadius: r, onTap: onTap, child: Padding(padding: padding, child: child)),
      ),
    );
  }
}

/// A real frosted surface (blurs what is behind it). Costly: use only a few,
/// large, mostly-static ones (the dock, sheets).
class GlassSurface extends StatelessWidget {
  const GlassSurface({super.key, required this.child, this.radius = 0, this.blur = 18, this.topBorderOnly = false});
  final Widget child;
  final double radius;
  final double blur;
  final bool topBorderOnly;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final dark = context.isDark;
    final r = BorderRadius.circular(radius);
    final blurred = blur > 0;
    final box = DecoratedBox(
      decoration: BoxDecoration(
        // Without blur the fill is more opaque so text stays readable.
        color: (dark ? const Color(0xFF14131E) : Colors.white).withValues(alpha: blurred ? (dark ? 0.62 : 0.72) : (dark ? 0.90 : 0.92)),
        border: topBorderOnly
            ? Border(top: BorderSide(color: tok.glassEdge.withValues(alpha: dark ? 0.14 : 0.10)))
            : Border.all(color: tok.glassEdge.withValues(alpha: dark ? 0.16 : 0.10)),
      ),
      child: child,
    );
    return RepaintBoundary(
      child: ClipRRect(
        borderRadius: r,
        child: blurred ? BackdropFilter(filter: ImageFilter.blur(sigmaX: blur, sigmaY: blur), child: box) : box,
      ),
    );
  }
}

/// A small square icon button.
class GlassIconButton extends StatelessWidget {
  const GlassIconButton({
    super.key,
    required this.icon,
    required this.onPressed,
    this.size = 44,
    this.filled = false,
    this.color,
    this.tooltip,
    this.round = false,
  });
  final IconData icon;
  final VoidCallback? onPressed;
  final double size;
  final bool filled;
  final Color? color;
  final String? tooltip;
  final bool round;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    final shape = round
        ? CircleBorder(side: BorderSide(color: tok.glassEdge.withValues(alpha: 0.2)))
        : RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Radii.control),
            side: BorderSide(color: tok.glassEdge.withValues(alpha: 0.18)),
          );
    final fg = onPressed == null ? tok.textFaint : (color ?? (filled ? Colors.white : tok.text));
    final btn = Material(
      color: filled ? accent : tok.glass.withValues(alpha: context.isDark ? 0.10 : 0.6),
      shape: shape,
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onPressed,
        child: SizedBox(width: size, height: size, child: Icon(icon, size: size * 0.5, color: fg)),
      ),
    );
    return tooltip == null ? btn : Tooltip(message: tooltip!, child: btn);
  }
}

/// A rounded-rectangle chip.
class GlassChip extends StatelessWidget {
  const GlassChip({super.key, required this.label, this.selected = false, this.onTap, this.icon});
  final String label;
  final bool selected;
  final VoidCallback? onTap;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    return Material(
      color: selected ? accent.withValues(alpha: 0.30) : tok.glass.withValues(alpha: context.isDark ? 0.09 : 0.6),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Radii.chip),
        side: BorderSide(color: tok.glassEdge.withValues(alpha: selected ? 0.35 : 0.14)),
      ),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            if (icon != null) ...[Icon(icon, size: 16), const SizedBox(width: 6)],
            Text(label, style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w500)),
          ]),
        ),
      ),
    );
  }
}
