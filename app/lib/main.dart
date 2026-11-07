import 'package:flutter/material.dart';
import 'package:media_kit/media_kit.dart';

import 'boot.dart';
import 'player/controller.dart';
import 'screens/shell.dart';
import 'src/rust/frb_generated.dart';
import 'theme/glass.dart';

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
      final p = PlayerController();
      await p.init();
      if (mounted) setState(() => _player = p);
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  @override
  void dispose() {
    _player?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final accent = HarpColors.accentFor(null);
    return MaterialApp(
      title: 'Harp',
      debugShowCheckedModeBanner: false,
      theme: harpTheme(accent),
      home: _player != null
          ? Shell(player: _player!)
          : AuroraBackground(
              accent: accent,
              child: Scaffold(
                body: Center(
                  child: _error == null
                      ? const Column(mainAxisSize: MainAxisSize.min, children: [
                          Icon(Icons.graphic_eq_rounded, size: 56),
                          SizedBox(height: 16),
                          Text('Getting yt-dlp ready…', style: TextStyle(color: HarpColors.textDim)),
                          SizedBox(height: 16),
                          SizedBox(width: 160, child: LinearProgressIndicator()),
                        ])
                      : Padding(
                          padding: const EdgeInsets.all(24),
                          child: GlassCard(
                            child: Column(mainAxisSize: MainAxisSize.min, children: [
                              const Text("Harp couldn't start", style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
                              const SizedBox(height: 8),
                              Text('$_error', textAlign: TextAlign.center, style: const TextStyle(color: HarpColors.textDim)),
                              const SizedBox(height: 14),
                              FilledButton(onPressed: _boot, child: const Text('Try again')),
                            ]),
                          ),
                        ),
                ),
              ),
            ),
    );
  }
}
