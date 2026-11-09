import 'dart:math' as math;
import 'dart:ui';

import 'package:flutter/cupertino.dart' show CupertinoPageTransitionsBuilder;
import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart';

/// Harp's design system: a music app first. Flat backgrounds, artwork-led colour,
/// plain list rows, big type. (The file keeps its old name; the "glass" is now
/// used only for the floating player and sheets.)

/// Corner radii.
class Radii {
  static const double card = 12;
  static const double tile = 10;
  static const double cover = 6;
  static const double control = 12;
  static const double sheet = 18;
  static const double chip = 8;
}

/// Colours that differ between the dark and light themes.
@immutable
class HarpTokens extends ThemeExtension<HarpTokens> {
  const HarpTokens({
    required this.ink,
    required this.surface,
    required this.raised,
    required this.text,
    required this.textDim,
    required this.textFaint,
    required this.separator,
    required this.glass,
    required this.glassEdge,
    required this.shadow,
    required this.orbAlpha,
  });

  final Color ink; // page background
  final Color surface; // grouped sections, search field
  final Color raised; // floating player, sheets
  final Color text;
  final Color textDim;
  final Color textFaint;
  final Color separator;
  final Color glass;
  final Color glassEdge;
  final Color shadow;
  final double orbAlpha;

  static const dark = HarpTokens(
    ink: Color(0xFF0A0A0C),
    surface: Color(0xFF1C1C1F),
    raised: Color(0xFF242428),
    text: Color(0xFFF5F5F7),
    textDim: Color(0xFFA1A1A6),
    textFaint: Color(0xFF6E6E73),
    separator: Color(0x1FFFFFFF),
    glass: Colors.white,
    glassEdge: Colors.white,
    shadow: Colors.black,
    orbAlpha: 0.35,
  );

  static const light = HarpTokens(
    ink: Color(0xFFFFFFFF),
    surface: Color(0xFFF2F2F7),
    raised: Color(0xFFFFFFFF),
    text: Color(0xFF1C1C1E),
    textDim: Color(0xFF6C6C70),
    textFaint: Color(0xFFAEAEB2),
    separator: Color(0x1F000000),
    glass: Colors.white,
    glassEdge: Color(0xFF3A3A3C),
    shadow: Color(0xFF000000),
    orbAlpha: 0.22,
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
  /// A stable fallback colour for a song when its artwork colour is not known yet.
  static Color accentFor(String? seed) {
    if (seed == null || seed.isEmpty) return harpRed;
    var h = 0;
    for (final c in seed.codeUnits) {
      h = (h * 31 + c) & 0x7fffffff;
    }
    return HSLColor.fromAHSL(1, (h % 360).toDouble(), 0.7, 0.55).toColor();
  }

  static const harpRed = Color(0xFFFA2D48); // like a music app's signature red
  static const harpRedLight = Color(0xFFFF6B6B);
  static const harpRedDark = Color(0xFFB3001B);

  /// Keeps an artwork colour readable as an accent on this background.
  static Color usableAccent(Color c, Brightness b) {
    final hsl = HSLColor.fromColor(c);
    final s = hsl.saturation < 0.25 ? 0.0 : hsl.saturation.clamp(0.45, 0.9);
    final l = b == Brightness.dark ? hsl.lightness.clamp(0.55, 0.72) : hsl.lightness.clamp(0.36, 0.5);
    return hsl.withSaturation(s.toDouble()).withLightness(l.toDouble()).toColor();
  }
}

ThemeData harpTheme(Color accent, Brightness b) {
  final tok = b == Brightness.dark ? HarpTokens.dark : HarpTokens.light;
  final a = HarpColors.usableAccent(accent, b);
  final scheme = ColorScheme.fromSeed(seedColor: a, brightness: b, surface: tok.ink).copyWith(primary: a);
  final base = b == Brightness.dark ? ThemeData.dark() : ThemeData.light();
  return ThemeData(
    useMaterial3: true,
    brightness: b,
    colorScheme: scheme,
    scaffoldBackgroundColor: tok.ink,
    canvasColor: tok.ink,
    extensions: [tok],
    textTheme: base.textTheme.apply(bodyColor: tok.text, displayColor: tok.text),
    splashFactory: InkSparkle.splashFactory,
    sliderTheme: SliderThemeData(
      trackHeight: 3,
      activeTrackColor: tok.text,
      inactiveTrackColor: tok.text.withValues(alpha: 0.18),
      thumbColor: tok.text,
      overlayColor: tok.text.withValues(alpha: 0.1),
      thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 5),
    ),
    dividerTheme: DividerThemeData(color: tok.separator, thickness: 0.6, space: 0.6),
    filledButtonTheme: FilledButtonThemeData(style: FilledButton.styleFrom(
      backgroundColor: a,
      foregroundColor: Colors.white,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Radii.control)),
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 13),
      textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
    )),
    outlinedButtonTheme: OutlinedButtonThemeData(style: OutlinedButton.styleFrom(
      backgroundColor: tok.surface,
      foregroundColor: a,
      side: BorderSide.none,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Radii.control)),
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 13),
      textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
    )),
    textButtonTheme: TextButtonThemeData(style: TextButton.styleFrom(
      foregroundColor: a,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Radii.control)),
      textStyle: const TextStyle(fontWeight: FontWeight.w600),
    )),
    iconButtonTheme: IconButtonThemeData(style: IconButton.styleFrom(foregroundColor: tok.text)),
    appBarTheme: AppBarTheme(
      backgroundColor: tok.ink,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: false,
      foregroundColor: a,
      titleTextStyle: TextStyle(color: tok.text, fontSize: 17, fontWeight: FontWeight.w700),
    ),
    popupMenuTheme: PopupMenuThemeData(
      color: tok.raised,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Radii.tile)),
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: tok.raised,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Radii.sheet)),
    ),
    bottomSheetTheme: BottomSheetThemeData(backgroundColor: tok.raised, surfaceTintColor: Colors.transparent, showDragHandle: false),
    snackBarTheme: SnackBarThemeData(
      behavior: SnackBarBehavior.floating,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Radii.control)),
    ),
    inputDecorationTheme: InputDecorationTheme(
      hintStyle: TextStyle(color: tok.textFaint),
      filled: false,
      border: const UnderlineInputBorder(),
    ),
    switchTheme: SwitchThemeData(
      thumbColor: const WidgetStatePropertyAll(Colors.white),
      trackColor: WidgetStateProperty.resolveWith((s) => s.contains(WidgetState.selected) ? const Color(0xFF34C759) : tok.text.withValues(alpha: 0.16)),
      trackOutlineColor: const WidgetStatePropertyAll(Colors.transparent),
    ),
    listTileTheme: ListTileThemeData(contentPadding: const EdgeInsets.symmetric(horizontal: 16), iconColor: a),
    progressIndicatorTheme: ProgressIndicatorThemeData(color: a),
    pageTransitionsTheme: const PageTransitionsTheme(builders: {
      TargetPlatform.android: PredictiveBackPageTransitionsBuilder(),
      TargetPlatform.linux: FadeUpwardsPageTransitionsBuilder(),
      TargetPlatform.windows: FadeUpwardsPageTransitionsBuilder(),
      TargetPlatform.macOS: CupertinoPageTransitionsBuilder(),
      TargetPlatform.iOS: CupertinoPageTransitionsBuilder(),
    }),
  );
}

/// The page background: plain, with a soft glow of the song's colour at the top.
/// Static (no animation), so it costs nothing per frame. The old animated aurora
/// lives on only behind Now Playing (see [ReactiveWaves]).
class AuroraBackground extends StatelessWidget {
  const AuroraBackground({super.key, required this.accent, required this.child, this.level, this.active = false, this.flat = false});
  final Color accent;
  final Widget child;
  final ValueListenable<double>? level; // kept for compatibility; unused here
  final bool active; // kept for compatibility; unused here
  final bool flat;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: tok.ink,
        gradient: flat
            ? null
            : LinearGradient(
                begin: Alignment.topCenter,
                end: const Alignment(0, -0.2),
                colors: [accent.withValues(alpha: context.isDark ? 0.22 : 0.10), tok.ink],
              ),
      ),
      child: child,
    );
  }
}

/// Soft waves along the bottom of Now Playing that rise with the music.
class ReactiveWaves extends StatefulWidget {
  const ReactiveWaves({super.key, required this.color, required this.level, required this.active});
  final Color color;
  final ValueListenable<double> level;
  final bool active;

  @override
  State<ReactiveWaves> createState() => _ReactiveWavesState();
}

class _ReactiveWavesState extends State<ReactiveWaves> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(vsync: this, duration: const Duration(seconds: 60));
  final ValueNotifier<double> _t = ValueNotifier(0);
  double _last = 0;

  @override
  void initState() {
    super.initState();
    _c.addListener(() {
      final v = _c.value * 60;
      if ((v - _last).abs() >= 0.05 || v < _last) {
        _last = v;
        _t.value = v;
      }
    });
    _sync();
  }

  @override
  void didUpdateWidget(ReactiveWaves old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() => widget.active ? (_c.isAnimating ? null : _c.repeat()) : _c.stop();

  @override
  void dispose() {
    _c.dispose();
    _t.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      RepaintBoundary(child: CustomPaint(painter: _WavesPainter(_t, widget.level, widget.color), size: Size.infinite));
}

class _WavesPainter extends CustomPainter {
  _WavesPainter(this.t, this.level, this.color) : super(repaint: Listenable.merge([t, level]));
  final ValueNotifier<double> t;
  final ValueListenable<double> level;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final lv = level.value.clamp(0.0, 1.0);
    final baseY = size.height * 0.84;
    for (var layer = 0; layer < 3; layer++) {
      final amp = (6 + 46 * lv) * (1 - layer * 0.25);
      final path = Path()..moveTo(0, size.height);
      const steps = 40;
      for (var i = 0; i <= steps; i++) {
        final x = size.width * i / steps;
        final ph = i / steps * (2.2 + layer) * math.pi + t.value * (1.2 + layer * 0.7);
        path.lineTo(x, baseY + layer * 10 - amp * (0.55 * math.sin(ph) + 0.45 * math.sin(ph * 1.9 + layer)));
      }
      path
        ..lineTo(size.width, size.height)
        ..close();
      canvas.drawPath(path, Paint()..color = color.withValues(alpha: 0.16 - layer * 0.04));
    }
  }

  @override
  bool shouldRepaint(_WavesPainter old) => old.color != color;
}

/// A grouped, filled section (settings groups, info boxes). No blur.
class GlassCard extends StatelessWidget {
  const GlassCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(14),
    this.radius = Radii.card,
    this.tint,
    this.onTap,
    this.onLongPress,
    this.strength = 1,
  });
  final Widget child;
  final EdgeInsetsGeometry padding;
  final double radius;
  final Color? tint;
  final VoidCallback? onTap;
  final VoidCallback? onLongPress;
  final double strength;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final r = BorderRadius.circular(radius);
    final color = tint == null ? tok.surface : Color.alphaBlend(tint!.withValues(alpha: context.isDark ? 0.22 : 0.14), tok.surface);
    return Material(
      color: color,
      borderRadius: r,
      clipBehavior: Clip.antiAlias,
      child: InkWell(onTap: onTap, onLongPress: onLongPress, child: Padding(padding: padding, child: child)),
    );
  }
}

/// A raised surface (the floating player, sheets). Blurs only when [blur] > 0.
class GlassSurface extends StatelessWidget {
  const GlassSurface({super.key, required this.child, this.radius = 0, this.blur = 0, this.topBorderOnly = false});
  final Widget child;
  final double radius;
  final double blur;
  final bool topBorderOnly;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final r = BorderRadius.circular(radius);
    final blurred = blur > 0;
    final box = DecoratedBox(
      decoration: BoxDecoration(
        color: tok.raised.withValues(alpha: blurred ? 0.82 : 1),
        border: topBorderOnly ? Border(top: BorderSide(color: tok.separator)) : null,
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

/// A plain icon button (music apps use bare glyphs). [filled] draws a solid circle.
class GlassIconButton extends StatelessWidget {
  const GlassIconButton({
    super.key,
    required this.icon,
    required this.onPressed,
    this.size = 44,
    this.filled = false,
    this.color,
    this.tooltip,
    this.round = true,
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
    final fg = onPressed == null ? tok.textFaint : (color ?? (filled ? tok.ink : tok.text));
    final btn = Material(
      color: filled ? tok.text : Colors.transparent,
      shape: const CircleBorder(),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onPressed,
        child: SizedBox(width: size, height: size, child: Icon(icon, size: size * (filled ? 0.5 : 0.62), color: fg)),
      ),
    );
    return tooltip == null ? btn : Tooltip(message: tooltip!, child: btn);
  }
}

/// A filter chip.
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
      color: selected ? accent : tok.surface,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Radii.chip)),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            if (icon != null) ...[Icon(icon, size: 16, color: selected ? Colors.white : tok.text), const SizedBox(width: 6)],
            Text(label, style: TextStyle(fontSize: 14, fontWeight: FontWeight.w600, color: selected ? Colors.white : tok.text)),
          ]),
        ),
      ),
    );
  }
}

/// The big title at the top of every main screen.
class PageTitle extends StatelessWidget {
  const PageTitle(this.title, {super.key, this.leading, this.actions = const [], this.subtitle});
  final String title;
  final String? subtitle;
  final Widget? leading;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 8, 6),
      child: Row(children: [
        if (leading != null) ...[leading!, const SizedBox(width: 12)],
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
            if (subtitle != null)
              Text(subtitle!.toUpperCase(), style: TextStyle(fontSize: 12, fontWeight: FontWeight.w600, letterSpacing: 0.6, color: context.tok.textDim)),
            Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 32, fontWeight: FontWeight.w800, height: 1.15, letterSpacing: -0.6)),
          ]),
        ),
        ...actions,
      ]),
    );
  }
}

/// A section heading, optionally with an action on the right ("See all").
class SectionHeader extends StatelessWidget {
  const SectionHeader(this.title, {super.key, this.subtitle, this.action, this.onAction});
  final String title;
  final String? subtitle;
  final String? action;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 18, 8, 10),
      child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title, style: const TextStyle(fontSize: 22, fontWeight: FontWeight.w700, letterSpacing: -0.3)),
            if (subtitle != null) Text(subtitle!, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: tok.textDim)),
          ]),
        ),
        if (action != null) TextButton(onPressed: onAction, child: Text(action!)),
      ]),
    );
  }
}

/// What a list shows when it has nothing to show.
class EmptyState extends StatelessWidget {
  const EmptyState({super.key, required this.icon, required this.text});
  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          Icon(icon, size: 44, color: tok.textFaint),
          const SizedBox(height: 12),
          Text(text, textAlign: TextAlign.center, style: TextStyle(color: tok.textDim, fontSize: 15)),
        ]),
      ),
    );
  }
}
