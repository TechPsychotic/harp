import 'package:flutter/material.dart';

import '../player/controller.dart';
import '../settings.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_actions.dart';
import 'playlist.dart' show AuroraScaffold;

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key, required this.player, this.embedded = false});
  final PlayerController player;

  /// Shown as a tab of the main window (no back button, no own background).
  final bool embedded;

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  int _thumbBytes = 0;
  int _downloadBytes = 0;
  String _ytdlp = '…';

  AppSettings get s => widget.player.settings;

  @override
  void initState() {
    super.initState();
    _refresh();
    api.ytdlpVersion().then((v) => mounted ? setState(() => _ytdlp = v) : null).catchError((_) {
      if (mounted) setState(() => _ytdlp = 'unavailable');
    });
  }

  Future<void> _refresh() async {
    final t = await api.thumbnailCacheBytes();
    final d = (await api.downloads()).fold<int>(0, (a, e) => a + e.bytes.toInt());
    if (mounted) setState(() { _thumbBytes = t.toInt(); _downloadBytes = d; });
  }

  String _mb(int b) => '${(b / 1048576).toStringAsFixed(1)} MB';

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final list = ListenableBuilder(
          listenable: s,
          builder: (context, _) => ListView(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 40),
            children: [
              _Section('Appearance', [
                Padding(
                  padding: const EdgeInsets.all(12),
                  child: SegmentedButton<ThemeMode>(
                    segments: const [
                      ButtonSegment(value: ThemeMode.system, icon: Icon(Icons.brightness_auto_rounded), label: Text('System')),
                      ButtonSegment(value: ThemeMode.dark, icon: Icon(Icons.dark_mode_rounded), label: Text('Dark')),
                      ButtonSegment(value: ThemeMode.light, icon: Icon(Icons.light_mode_rounded), label: Text('Light')),
                    ],
                    selected: {s.themeMode},
                    onSelectionChanged: (v) => s.setTheme(v.first),
                  ),
                ),
                ListTile(
                  title: const Text('Background effects'),
                  subtitle: Text(effectsLabel(s.effectsPref) + (s.effectsPref == 'auto' && s.effects != 'full' ? '  (now: ${s.effects})' : ''), style: TextStyle(color: tok.textDim)),
                  trailing: PopupMenuButton<String>(
                    icon: const Icon(Icons.expand_more_rounded),
                    initialValue: s.effectsPref,
                    onSelected: s.setEffects,
                    itemBuilder: (_) => [for (final e in ['auto', 'full', 'reduced', 'off']) PopupMenuItem(value: e, child: Text(effectsLabel(e)))],
                  ),
                ),
                SwitchListTile(
                  title: const Text('Music-reactive background'),
                  subtitle: Text('The background moves with the loudness of what plays. Turn off to save battery.', style: TextStyle(color: tok.textDim)),
                  value: s.reactiveBackground,
                  onChanged: s.setReactiveBackground,
                ),
              ]),
              _Section('Playback', [
                _QualityTile(title: 'Streaming quality', value: s.streamQuality, onChanged: s.setStreamQuality),
                ListTile(
                  title: const Text('Spatial audio'),
                  subtitle: Text('${spatialLabel(s.spatial)} · a virtual stereo effect for headphones and speakers. Not Dolby Atmos.', style: TextStyle(color: tok.textDim)),
                  trailing: PopupMenuButton<String>(
                    icon: const Icon(Icons.expand_more_rounded),
                    initialValue: s.spatial,
                    onSelected: s.setSpatial,
                    itemBuilder: (_) => [for (final m in ['off', 'wide', 'headphone', 'immersive']) PopupMenuItem(value: m, child: Text(spatialLabel(m)))],
                  ),
                ),
              ]),
              _Section('Offline', [
                SwitchListTile(
                  title: const Text('Offline mode'),
                  subtitle: Text('Play only downloaded songs and never use the network.', style: TextStyle(color: tok.textDim)),
                  value: s.offline,
                  onChanged: s.setOffline,
                ),
                _QualityTile(title: 'Download quality', value: s.downloadQuality, onChanged: s.setDownloadQuality),
                ListTile(
                  title: const Text('Downloaded songs'),
                  subtitle: Text(_mb(_downloadBytes), style: TextStyle(color: tok.textDim)),
                  trailing: TextButton(
                    onPressed: () async {
                      for (final d in await api.downloads()) {
                        await api.deleteDownload(id: d.song.id);
                      }
                      widget.player.libraryRev.value++;
                      _refresh();
                    },
                    child: const Text('Delete all'),
                  ),
                ),
              ]),
              _Section('Storage', [
                ListTile(
                  title: const Text('Cover art cache'),
                  subtitle: Text(_mb(_thumbBytes), style: TextStyle(color: tok.textDim)),
                  trailing: TextButton(
                    onPressed: () async {
                      await api.clearThumbnailCache();
                      _refresh();
                      if (context.mounted) toast(context, 'Cover cache cleared');
                    },
                    child: const Text('Clear'),
                  ),
                ),
              ]),
              _Section('About', [
                ListTile(title: const Text('Harp'), subtitle: Text('No accounts, no servers. Your taste stays on this device.', style: TextStyle(color: tok.textDim))),
                ListTile(title: const Text('yt-dlp'), subtitle: Text(_ytdlp, style: TextStyle(color: tok.textDim))),
              ]),
            ],
          ),
    );
    if (widget.embedded) {
      return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Padding(padding: const EdgeInsets.fromLTRB(16, 20, 16, 4), child: Text('Settings', style: Theme.of(context).textTheme.headlineSmall?.copyWith(fontWeight: FontWeight.w700))),
        Expanded(child: list),
      ]);
    }
    return AuroraScaffold(
      player: widget.player,
      child: Scaffold(appBar: AppBar(backgroundColor: Colors.transparent, title: const Text('Settings')), body: list),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section(this.title, this.children);
  final String title;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: 16),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Padding(padding: const EdgeInsets.fromLTRB(4, 0, 4, 8), child: Text(title, style: TextStyle(fontWeight: FontWeight.w700, color: context.tok.textDim))),
        GlassCard(padding: EdgeInsets.zero, child: Column(children: children)),
      ]),
    );
  }
}

class _QualityTile extends StatelessWidget {
  const _QualityTile({required this.title, required this.value, required this.onChanged});
  final String title;
  final String value;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      title: Text(title),
      subtitle: Text(qualityLabel(value), style: TextStyle(color: context.tok.textDim)),
      trailing: PopupMenuButton<String>(
        icon: const Icon(Icons.expand_more_rounded),
        initialValue: value,
        onSelected: onChanged,
        itemBuilder: (_) => [for (final q in ['low', 'medium', 'high']) PopupMenuItem(value: q, child: Text(qualityLabel(q)))],
      ),
    );
  }
}
