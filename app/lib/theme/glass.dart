import 'dart:math' as math;
import 'dart:ui';

import 'package:flutter/material.dart';

/// Harp's glassmorphic look: a dark aurora background, frosted translucent
/// panels with a light edge, and one accent that follows the playing song.
class HarpColors {
  static const ink = Color(0xFF07070D);
  static const text = Color(0xFFF4F3FA);
  static const textDim = Color(0xB3F4F3FA);
  static const textFaint = Color(0x80F4F3FA);

  /// A stable accent hue for a song (no image decoding needed).
  static Color accentFor(String? seed) {
    if (seed == null || seed.isEmpty) return const Color(0xFF8B7CFF);
    var h = 0;
    for (final c in seed.codeUnits) {
      h = (h * 31 + c) & 0x7fffffff;
    }
    return HSLColor.fromAHSL(1, (h % 360).toDouble(), 0.75, 0.62).toColor();
  }
}

ThemeData harpTheme(Color accent) {
  final scheme = ColorScheme.fromSeed(
    seedColor: accent,
    brightness: Brightness.dark,
    surface: HarpColors.ink,
  );
  return ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    scaffoldBackgroundColor: Colors.transparent,
    fontFamily: null,
    textTheme: ThemeData.dark().textTheme.apply(
          bodyColor: HarpColors.text,
          displayColor: HarpColors.text,
        ),
    splashFactory: InkSparkle.splashFactory,
    sliderTheme: SliderThemeData(
      trackHeight: 4,
      activeTrackColor: accent,
      inactiveTrackColor: Colors.white.withValues(alpha: 0.16),
      thumbColor: Colors.white,
      overlayColor: accent.withValues(alpha: 0.18),
      thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
    ),
  );
}

/// Slowly drifting colored orbs behind everything.
class AuroraBackground extends StatefulWidget {
  const AuroraBackground({super.key, required this.accent, required this.child});
  final Color accent;
  final Widget child;

  @override
  State<AuroraBackground> createState() => _AuroraBackgroundState();
}

class _AuroraBackgroundState extends State<AuroraBackground> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(vsync: this, duration: const Duration(seconds: 40))..repeat();

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final reduce = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    return Stack(fit: StackFit.expand, children: [
      const ColoredBox(color: HarpColors.ink),
      RepaintBoundary(
        child: AnimatedBuilder(
          animation: _c,
          builder: (_, _) => CustomPaint(painter: _AuroraPainter(reduce ? 0 : _c.value, widget.accent)),
        ),
      ),
      widget.child,
    ]);
  }
}

class _AuroraPainter extends CustomPainter {
  _AuroraPainter(this.t, this.accent);
  final double t;
  final Color accent;

  @override
  void paint(Canvas canvas, Size size) {
    final hsl = HSLColor.fromColor(accent);
    final orbs = <(Color, double, double, double)>[
      (accent, 0.2, 0.15, 0.55),
      (hsl.withHue((hsl.hue + 70) % 360).toColor(), 0.85, 0.3, 0.5),
      (hsl.withHue((hsl.hue + 200) % 360).toColor(), 0.4, 0.95, 0.6),
    ];
    final r = size.longestSide;
    for (var i = 0; i < orbs.length; i++) {
      final (color, x, y, rad) = orbs[i];
      final a = (t + i / orbs.length) * 2 * math.pi;
      final c = Offset(
        size.width * (x + 0.08 * math.cos(a)),
        size.height * (y + 0.08 * math.sin(a * 1.3)),
      );
      final paint = Paint()
        ..shader = RadialGradient(colors: [color.withValues(alpha: 0.42), color.withValues(alpha: 0)])
            .createShader(Rect.fromCircle(center: c, radius: r * rad));
      canvas.drawCircle(c, r * rad, paint);
    }
  }

  @override
  bool shouldRepaint(_AuroraPainter old) => old.t != t || old.accent != accent;
}

/// A frosted panel.
class GlassCard extends StatelessWidget {
  const GlassCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(16),
    this.radius = 24,
    this.blur = 24,
    this.tint,
    this.onTap,
  });
  final Widget child;
  final EdgeInsetsGeometry padding;
  final double radius;
  final double blur;
  final Color? tint;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final r = BorderRadius.circular(radius);
    return ClipRRect(
      borderRadius: r,
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: blur, sigmaY: blur),
        child: DecoratedBox(
          decoration: BoxDecoration(
            borderRadius: r,
            gradient: LinearGradient(
              begin: Alignment.topLeft,
              end: Alignment.bottomRight,
              colors: [
                (tint ?? Colors.white).withValues(alpha: 0.13),
                (tint ?? Colors.white).withValues(alpha: 0.05),
              ],
            ),
            border: Border.all(color: Colors.white.withValues(alpha: 0.18), width: 1),
            boxShadow: [BoxShadow(color: Colors.black.withValues(alpha: 0.25), blurRadius: 30, offset: const Offset(0, 12))],
          ),
          child: Material(
            type: MaterialType.transparency,
            child: InkWell(
              borderRadius: r,
              onTap: onTap,
              child: Padding(padding: padding, child: child),
            ),
          ),
        ),
      ),
    );
  }
}

/// A round frosted icon button.
class GlassIconButton extends StatelessWidget {
  const GlassIconButton({
    super.key,
    required this.icon,
    required this.onPressed,
    this.size = 44,
    this.filled = false,
    this.color,
    this.tooltip,
  });
  final IconData icon;
  final VoidCallback? onPressed;
  final double size;
  final bool filled;
  final Color? color;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.primary;
    final btn = ClipOval(
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 14, sigmaY: 14),
        child: Material(
          color: filled ? accent.withValues(alpha: 0.9) : Colors.white.withValues(alpha: 0.1),
          shape: CircleBorder(side: BorderSide(color: Colors.white.withValues(alpha: 0.2))),
          child: InkWell(
            onTap: onPressed,
            child: SizedBox(
              width: size,
              height: size,
              child: Icon(icon, size: size * 0.5, color: onPressed == null ? HarpColors.textFaint : (color ?? Colors.white)),
            ),
          ),
        ),
      ),
    );
    return tooltip == null ? btn : Tooltip(message: tooltip!, child: btn);
  }
}

/// A frosted pill chip.
class GlassChip extends StatelessWidget {
  const GlassChip({super.key, required this.label, this.selected = false, this.onTap, this.icon});
  final String label;
  final bool selected;
  final VoidCallback? onTap;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.primary;
    return ClipRRect(
      borderRadius: BorderRadius.circular(40),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 12, sigmaY: 12),
        child: Material(
          color: selected ? accent.withValues(alpha: 0.35) : Colors.white.withValues(alpha: 0.09),
          shape: StadiumBorder(side: BorderSide(color: Colors.white.withValues(alpha: selected ? 0.4 : 0.16))),
          child: InkWell(
            onTap: onTap,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                if (icon != null) ...[Icon(icon, size: 16), const SizedBox(width: 6)],
                Text(label, style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w500)),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}
