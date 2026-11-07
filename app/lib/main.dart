import 'package:flutter/material.dart';
import 'package:media_kit/media_kit.dart';

import 'boot.dart';
import 'player/controller.dart';
import 'screens/shell.dart';
import 'settings.dart';
import 'src/rust/frb_generated.dart';
import 'theme/glass.dart';
import 'theme/logo.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  MediaKit.ensureInitialized();
  await RustLib.init();
  runApp(const HarpApp());
}

class HarpApp extends StatefulWidget {
  const HarpApp({super.key});

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
      final p = PlayerController(_settings);
      await p.init();
      if (mounted) setState(() => _player = p);
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
