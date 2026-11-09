import 'dart:io';

import 'package:flutter/services.dart';

/// Small Android-only helpers (wake lock so playback survives a long screen-off,
/// notification permission for the media notification). No-ops elsewhere.
class Native {
  static const _c = MethodChannel('dev.harp/native');

  static Future<void> wakeLock(bool on) async {
    if (!Platform.isAndroid) return;
    try {
      await _c.invokeMethod('wakeLock', {'on': on});
    } catch (_) {}
  }

  static Future<void> requestNotifications() async {
    if (!Platform.isAndroid) return;
    try {
      await _c.invokeMethod('requestNotifications');
    } catch (_) {}
  }
}
