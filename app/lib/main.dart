import 'dart:async';
import 'dart:io';

import 'package:audio_service/audio_service.dart';
import 'package:audio_service_mpris/audio_service_mpris.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart';

import 'boot.dart';
import 'native.dart';
import 'player/media_session.dart';
import 'player/controller.dart';
import 'screens/shell.dart';
import 'settings.dart';
import 'links.dart';
import 'update.dart';
import 'src/rust/frb_generated.dart';
import 'theme/glass.dart';
import 'theme/logo.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  Links.listen(args);
  // Draw behind the status and navigation bars (edge to edge); screens use SafeArea.
  SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  SystemChrome.setSystemUIOverlayStyle(const SystemUiOverlayStyle(
    statusBarColor: Colors.transparent,
    systemNavigationBarColor: Colors.transparent,
    systemNavigationBarContrastEnforced: false,
  ));
  MpvAudioKit.ensureInitialized();
  await RustLib.init();
  if (Platform.isLinux) {
    // Shows Harp in the desktop's media controls (GNOME, KDE) and answers media keys.
    AudioServiceMpris.init(
      dBusName: 'harp',
      identity: 'Harp',
      desktopEntry: 'dev.harp.harp',
      canGoNext: true,
      canGoPrevious: true,
      canPlay: true,
      canPause: true,
      canControl: true,
    );
  }
  final session = await AudioService.init(
    builder: HarpAudioHandler.new,
    config: const AudioServiceConfig(
      androidNotificationChannelId: 'dev.harp.harp.playback',
      androidNotificationChannelName: 'Playback',
      androidNotificationIcon: 'drawable/ic_stat_harp',
      // Keep the media service (and so playback) alive while paused or with the screen off.
      androidStopForegroundOnPause: false,
      androidNotificationOngoing: false,
    ),
  );
  Native.requestNotifications();
  runApp(HarpApp(session: session));
}

class HarpApp extends StatefulWidget {
  const HarpApp({super.key, required this.session});
  final HarpAudioHandler session;

  @override
  State<HarpApp> createState() => _HarpAppState();
}

class _HarpAppState extends State<HarpApp> {
  final AppSettings _settings = AppSettings();
  PlayerController? _player;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _boot();
  }

  Future<void> _boot() async {
    setState(() => _error = null);
    try {
      await bootEngine();
      await _settings.load();
      _settings.watchPerformance();
      final p = PlayerController(_settings, widget.session);
      await p.init();
      if (mounted) setState(() => _player = p);
      await Links.ready(p);
      unawaited(Updates.check());
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  @override
  void dispose() {
    _player?.dispose();
    _settings.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final accent = HarpColors.accentFor(null);
    return ListenableBuilder(
      listenable: _settings,
      builder: (context, _) => MaterialApp(
        navigatorKey: Links.navigator,
        title: 'Harp',
        debugShowCheckedModeBanner: false,
        themeMode: _settings.themeMode,
        theme: harpTheme(accent, Brightness.light),
        darkTheme: harpTheme(accent, Brightness.dark),
        home: _player != null
            ? Shell(player: _player!)
            : Builder(
                builder: (context) => AuroraBackground(
                  accent: accent,
                  child: Scaffold(
                    body: Center(
                      child: _error == null
                          ? const Column(mainAxisSize: MainAxisSize.min, children: [
                              HarpLogo(size: 72),
                              SizedBox(height: 18),
                              Text('Getting ready…'),
                              SizedBox(height: 14),
                              SizedBox(width: 160, child: LinearProgressIndicator()),
                            ])
                          : Padding(
                              padding: const EdgeInsets.all(24),
                              child: GlassCard(
                                child: Column(mainAxisSize: MainAxisSize.min, children: [
                                  const Text("Harp couldn't start", style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
                                  const SizedBox(height: 8),
                                  Text('$_error', textAlign: TextAlign.center, style: TextStyle(color: context.tok.textDim)),
                                  const SizedBox(height: 14),
                                  FilledButton(onPressed: _boot, child: const Text('Try again')),
                                ]),
                              ),
                            ),
                    ),
                  ),
                ),
              ),
      ),
    );
  }
}
