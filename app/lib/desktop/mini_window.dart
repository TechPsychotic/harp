import 'dart:io';

import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

/// The mini player on computers: the window shrinks to a small strip with the
/// cover and controls, and asks to stay on top. (On GNOME with Wayland, apps
/// can't put themselves on top: right-click the title bar › Always on Top.)
class MiniWindow {
  static final ValueNotifier<bool> on = ValueNotifier(false);
  static bool get supported => Platform.isLinux || Platform.isWindows || Platform.isMacOS;
  static Size? _before;

  static Future<void> enter() async {
    if (!supported || on.value) return;
    try {
      final w = WindowManager.instance.getCurrent();
      if (w == null) return;
      _before = w.size.toSize();
      w.minimumSize = const Size(320, 112).toNative();
      w.setSize(const Size(420, 128).toNative(), false);
      w.isAlwaysOnTop = true;
      on.value = true;
    } catch (_) {}
  }

  static Future<void> exit() async {
    if (!on.value) return;
    on.value = false;
    try {
      final w = WindowManager.instance.getCurrent();
      if (w == null) return;
      w.isAlwaysOnTop = false;
      w.minimumSize = const Size(360, 560).toNative();
      w.setSize((_before ?? const Size(1100, 760)).toNative(), false);
    } catch (_) {}
  }
}
