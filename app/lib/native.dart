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

  /// What the last crash left behind (empty if there was none, or off Android).
  static Future<String> crashReport() async {
    if (!Platform.isAndroid) return '';
    try {
      return await _c.invokeMethod<String>('crashReport') ?? '';
    } catch (_) {
      return '';
    }
  }

  static Future<void> clearCrash() async {
    if (!Platform.isAndroid) return;
    try {
      await _c.invokeMethod('clearCrash');
    } catch (_) {}
  }

  /// True on mobile data (or any metered network). Desktop: never.
  static Future<bool> isMetered() async {
    if (!Platform.isAndroid) return false;
    try {
      return await _c.invokeMethod<bool>('isMetered') ?? false;
    } catch (_) {
      return false;
    }
  }

  /// Opens the share sheet with [text]. Returns false where there is none
  /// (desktop), so the caller can copy it instead.
  static Future<bool> share(String text) async {
    if (!Platform.isAndroid) return false;
    try {
      return await _c.invokeMethod<bool>('share', {'text': text}) ?? false;
    } catch (_) {
      return false;
    }
  }

  /// Android: asks to read your music files (true elsewhere).
  static Future<bool> requestAudioAccess() async {
    if (!Platform.isAndroid) return true;
    try {
      return await _c.invokeMethod<bool>('requestAudioAccess') ?? false;
    } catch (_) {
      return false;
    }
  }

  static Future<void> requestNotifications() async {
    if (!Platform.isAndroid) return;
    try {
      await _c.invokeMethod('requestNotifications');
    } catch (_) {}
  }
}
