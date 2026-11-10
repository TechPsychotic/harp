import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import 'cover.dart';

/// The player's background: the song's cover, heavily blurred, filling the
/// screen. The same gaussian blur as a live `ImageFiltered`, but worked out
/// **once per song** into a picture (at a quarter of the size with a quarter
/// of the radius: a blur this wide looks identical), so animations only move a
/// picture around instead of re-blurring the whole window every frame. (That
/// per-frame blur is what made opening the player stutter on big windows.)
class BlurredArt extends StatefulWidget {
  const BlurredArt({super.key, required this.url, required this.seed, this.sigma = 60});
  final String url;
  final String seed;
  final double sigma;

  /// [src] covering [size], blurred by [sigma], worked out at 1/[scale] of the size.
  static Future<ui.Image> blur(ui.Image src, Size size, double sigma, double scale) async {
    final w = (size.width / scale).ceil().clamp(16, 4000), h = (size.height / scale).ceil().clamp(16, 4000);
    final rec = ui.PictureRecorder();
    final c = Canvas(rec);
    final out = Rect.fromLTWH(0, 0, w.toDouble(), h.toDouble());
    c.saveLayer(out, Paint()..imageFilter = ui.ImageFilter.blur(sigmaX: sigma / scale, sigmaY: sigma / scale, tileMode: TileMode.decal));
    paintImage(canvas: c, rect: out, image: src, fit: BoxFit.cover, filterQuality: FilterQuality.medium);
    c.restore();
    return rec.endRecording().toImage(w, h);
  }

  @override
  State<BlurredArt> createState() => _BlurredArtState();
}

class _BlurredArtState extends State<BlurredArt> {
  static final Map<String, ui.Image> _cache = {};
  static final List<String> _order = [];
  ui.Image? _img;
  String? _key;

  static const _scale = 4.0;

  Future<void> _make(String key, String path, Size size) async {
    try {
      final codec = await ui.instantiateImageCodec(await File(path).readAsBytes(), targetWidth: 128);
      final src = (await codec.getNextFrame()).image;
      final img = await BlurredArt.blur(src, size, widget.sigma, _scale);
      src.dispose();
      _cache[key] = img;
      _order.add(key);
      while (_order.length > 4) {
        _cache.remove(_order.removeAt(0))?.dispose();
      }
      if (mounted && _key == key) setState(() => _img = img);
    } catch (_) {}
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, c) {
        final path = ThumbCache.peek(widget.url);
        if (path == null) return const SizedBox.shrink();
        // Sizes rounded, so resizing a window doesn't redo it every pixel.
        final size = Size((c.maxWidth / 100).ceil() * 100.0, (c.maxHeight / 100).ceil() * 100.0);
        final key = '$path|${size.width}x${size.height}|${widget.sigma}';
        if (key != _key) {
          _key = key;
          _img = _cache[key];
          if (_img == null) _make(key, path, size);
        }
        final img = _img;
        if (img == null) {
          // The first time for this song: the live blur, for the moment it takes.
          return ImageFiltered(
            imageFilter: ui.ImageFilter.blur(sigmaX: widget.sigma, sigmaY: widget.sigma, tileMode: TileMode.decal),
            child: FittedBox(fit: BoxFit.cover, child: Cover(url: widget.url, seed: widget.seed, size: 200, radius: 0)),
          );
        }
        return RawImage(image: img, fit: BoxFit.cover, width: c.maxWidth, height: c.maxHeight, filterQuality: FilterQuality.medium);
      });
}
