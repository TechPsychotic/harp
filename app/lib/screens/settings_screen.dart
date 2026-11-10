import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/services.dart';

import '../native.dart';
import '../update.dart';
import '../version.dart';
import 'package:url_launcher/url_launcher.dart';
import '../player/engine_probe.dart';
import 'spotify_screen.dart';
import 'sync_screen.dart';
import '../player/controller.dart';
import '../settings.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/cover.dart';
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
  int _cacheBytes = 0;
  int _downloadBytes = 0;
  String _ytdlp = '…';

  AppSettings get s => widget.player.settings;

  Timer? _sizes;

  @override
  void initState() {
    super.initState();
    _refresh();
    // Sizes follow downloads, removals and the play cache as they change.
    widget.player.libraryRev.addListener(_refresh);
    widget.player.downloading.addListener(_refresh);
    _sizes = Timer.periodic(const Duration(seconds: 30), (_) => _refresh());
    api.ytdlpVersion().then((v) => mounted ? setState(() => _ytdlp = v) : null).catchError((_) {
      if (mounted) setState(() => _ytdlp = 'unavailable');
    });
  }

  @override
  void dispose() {
    widget.player.libraryRev.removeListener(_refresh);
    widget.player.downloading.removeListener(_refresh);
    _sizes?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    final t = await api.thumbnailCacheBytes();
    final d = (await api.downloads()).fold<int>(0, (a, e) => a + e.bytes.toInt());
    final c = await api.cacheBytes();
    if (mounted) setState(() { _thumbBytes = t.toInt(); _downloadBytes = d; _cacheBytes = c.toInt(); });
  }

  Future<void> _probe(BuildContext context) async {
    final p = widget.player;
    final song = p.current ?? p.resumeOffer;
    if (song == null) {
      toast(context, 'Play a song first, then run the test');
      return;
    }
    if (p.playing) await p.togglePlay();
    final lines = ValueNotifier<List<String>>(['Testing with “${song.title}”…']);
    var done = false;
    if (!context.mounted) return;
    unawaited(showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (ctx) => AlertDialog(
        title: const Text('New audio engine'),
        content: SizedBox(
          width: 420,
          child: ValueListenableBuilder<List<String>>(
            valueListenable: lines,
            builder: (_, l, _) => SingleChildScrollView(child: SelectableText(l.join('\n'), style: const TextStyle(fontSize: 13, height: 1.4))),
          ),
        ),
        actions: [
          ValueListenableBuilder<List<String>>(
            valueListenable: lines,
            builder: (_, l, _) => TextButton(
              onPressed: done ? () => Clipboard.setData(ClipboardData(text: '${Platform.operatingSystem}\n${l.join('\n')}')).then((_) { if (ctx.mounted) toast(ctx, 'Copied'); }) : null,
              child: const Text('Copy'),
            ),
          ),
          ValueListenableBuilder<List<String>>(
            valueListenable: lines,
            builder: (_, _, _) => TextButton(onPressed: done ? () => Navigator.pop(ctx) : null, child: const Text('Close')),
          ),
        ],
      ),
    ));
    await EngineProbe.run(song.id, quality: p.settings.streamQuality, log: (s) => lines.value = [...lines.value, s]);
    done = true;
    lines.value = [...lines.value, '', 'Copy this and send it to me.'];
  }

  Future<void> _backup(BuildContext context) async {
    try {
      final bytes = await api.libraryBackup();
      final d = DateTime.now();
      final name = 'harp-library-${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}.harp';
      final where = await FilePicker.saveFile(fileName: name, bytes: bytes, dialogTitle: 'Save your Harp library');
      if (context.mounted) toast(context, where == null ? 'Not saved' : 'Library saved');
    } catch (e) {
      if (context.mounted) toast(context, "Couldn't back up: $e");
    }
  }

  Future<void> _restore(BuildContext context) async {
    try {
      final files = await FilePicker.pickFiles(dialogTitle: 'Choose a Harp backup');
      if (files.isEmpty || files.first.path == null) return;
      final bytes = await File(files.first.path!).readAsBytes();
      final info = await api.libraryRestore(bytes: bytes);
      widget.player.libraryRev.value++;
      widget.player.savedRev.value++;
      if (!context.mounted) return;
      final nothing = info.liked == 0 && info.playlistsNew == 0 && info.playlistsGrown == 0 && info.plays == 0;
      await showDialog<void>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: const Text('Backup added'),
          content: Text(nothing
              ? 'Everything in this backup is already here.'
              : 'Added ${info.liked} liked songs, ${info.playlistsNew} new playlists'
                  '${info.playlistsGrown > 0 ? ', more songs in ${info.playlistsGrown} playlists you already had' : ''}'
                  ' and ${info.plays} plays from your history. Nothing you had was changed.'),
          actions: [TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('OK'))],
        ),
      );
    } catch (e) {
      if (context.mounted) toast(context, "Couldn't restore: $e");
    }
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
                  child: Row(children: [
                    for (final (mode, icon, label) in [
                      (ThemeMode.system, Icons.brightness_auto_rounded, 'Auto'),
                      (ThemeMode.dark, Icons.dark_mode_rounded, 'Dark'),
                      (ThemeMode.light, Icons.light_mode_rounded, 'Light'),
                    ])
                      Expanded(
                        child: Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 3),
                          child: _ThemeChoice(icon: icon, label: label, selected: s.themeMode == mode, onTap: () => s.setTheme(mode)),
                        ),
                      ),
                  ]),
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
                  subtitle: Text('The player background moves with the music, following its live spectrum. Turn off to save battery.', style: TextStyle(color: tok.textDim)),
                  value: s.reactiveBackground,
                  onChanged: s.setReactiveBackground,
                ),
              ]),
              _Section('Playback', [
                _QualityTile(title: 'Streaming quality', value: s.streamQuality, onChanged: s.setStreamQuality),
                ListTile(
                  enabled: PlayerController.fxSupported,
                  title: const Text('Spatial audio'),
                  subtitle: Text(
                    '${spatialLabel(s.spatial)} · a virtual stereo effect for headphones and speakers. Not Dolby Atmos.',
                    style: TextStyle(color: tok.textDim),
                  ),
                  trailing: !PlayerController.fxSupported ? null : PopupMenuButton<String>(
                    icon: const Icon(Icons.expand_more_rounded),
                    initialValue: s.spatial,
                    onSelected: s.setSpatial,
                    itemBuilder: (_) => [for (final m in ['off', 'wide', 'headphone', 'immersive']) PopupMenuItem(value: m, child: Text(spatialLabel(m)))],
                  ),
                ),
                ListTile(
                  enabled: PlayerController.fxSupported,
                  title: const Text('Equalizer'),
                  subtitle: Text(
                    eqLabel(s.eq),
                    style: TextStyle(color: tok.textDim),
                  ),
                  trailing: !PlayerController.fxSupported ? null : PopupMenuButton<String>(
                    icon: const Icon(Icons.expand_more_rounded),
                    initialValue: s.eq,
                    onSelected: (v) {
                      if (v == 'custom') {
                        s.setEq('custom');
                        showCustomEq(context, s);
                      } else {
                        s.setEq(v);
                      }
                    },
                    itemBuilder: (_) => [
                      for (final m in ['flat', 'bass', 'vocal', 'treble', 'loudness']) PopupMenuItem(value: m, child: Text(eqLabel(m))),
                      const PopupMenuItem(value: 'custom', child: Text('Custom…')),
                    ],
                  ),
                ),
                SwitchListTile(
                  title: const Text('Even volume'),
                  subtitle: Text(
                    'Every song at the same loudness (EBU R128), so you don’t reach for the volume between songs.',
                    style: TextStyle(color: tok.textDim),
                  ),
                  value: s.normalize,
                  onChanged: PlayerController.fxSupported ? s.setNormalize : null,
                ),
                ListTile(
                  title: const Text('Headphone correction'),
                  subtitle: Text(s.hpName.isEmpty ? 'Off. Pick your headphones to hear them neutral (measured profiles from AutoEq).' : s.hpName, style: TextStyle(color: tok.textDim)),
                  trailing: s.hpName.isEmpty
                      ? const Icon(Icons.chevron_right_rounded)
                      : IconButton(tooltip: 'Turn off', icon: const Icon(Icons.close_rounded), onPressed: () => s.setHeadphone('', 0, const [])),
                  onTap: () => showHeadphonePicker(context, s),
                ),
                ListTile(
                  title: const Text('Crossfade'),
                  subtitle: Text(s.crossfade == 0 ? 'Off: songs follow each other with no gap' : '${s.crossfade} s: each song fades into the next', style: TextStyle(color: tok.textDim)),
                  trailing: PopupMenuButton<int>(
                    icon: const Icon(Icons.expand_more_rounded),
                    initialValue: s.crossfade,
                    onSelected: s.setCrossfade,
                    itemBuilder: (_) => [for (final v in const [0, 3, 6, 9, 12]) PopupMenuItem(value: v, child: Text(v == 0 ? 'Off (gapless)' : '$v seconds'))],
                  ),
                ),
                ListTile(
                  title: const Text('Playback speed'),
                  subtitle: Text(s.speed == 1 ? 'Normal' : '${s.speed}× · the pitch stays the same', style: TextStyle(color: tok.textDim)),
                  trailing: PopupMenuButton<double>(
                    icon: const Icon(Icons.expand_more_rounded),
                    initialValue: s.speed,
                    onSelected: s.setSpeed,
                    itemBuilder: (_) => [for (final v in const [0.75, 1.0, 1.25, 1.5, 1.75, 2.0]) PopupMenuItem(value: v, child: Text(v == 1 ? 'Normal' : '$v×'))],
                  ),
                ),
              ]),
              _Section('Play cache', [
                ListTile(
                  title: const Text('Fetch songs ahead'),
                  subtitle: Text(
                    s.cacheMb == 0
                        ? 'Off: songs stream only'
                        : 'Up to ${s.cacheMb} MB. The next songs load in the background so skipping is instant and playback survives a weak connection or a sleeping screen.',
                    style: TextStyle(color: tok.textDim),
                  ),
                  trailing: PopupMenuButton<int>(
                    icon: const Icon(Icons.expand_more_rounded),
                    initialValue: s.cacheMb,
                    onSelected: s.setCacheMb,
                    itemBuilder: (_) => [for (final mb in [0, 256, 512, 1024, 2048]) PopupMenuItem(value: mb, child: Text(mb == 0 ? 'Off' : '$mb MB'))],
                  ),
                ),
                SwitchListTile(
                  title: const Text('Fetch ahead on mobile data'),
                  subtitle: Text('Off: songs are fetched ahead only on Wi-Fi.', style: TextStyle(color: tok.textDim)),
                  value: s.prefetchOnMobile,
                  onChanged: s.setPrefetchOnMobile,
                ),
                ListTile(
                  title: const Text('Cached songs'),
                  subtitle: Text(_mb(_cacheBytes), style: TextStyle(color: tok.textDim)),
                  trailing: TextButton(
                    onPressed: () async {
                      await api.clearCache();
                      _refresh();
                      if (context.mounted) toast(context, 'Play cache cleared');
                    },
                    child: const Text('Clear'),
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
                      ThumbCache.forgetAll();
                      _refresh();
                      if (context.mounted) toast(context, 'Cover cache cleared');
                    },
                    child: const Text('Clear'),
                  ),
                ),
              ]),
              _Section('Backup', [
                ListTile(
                  leading: const Icon(Icons.devices_rounded),
                  title: const Text('Sync with another device'),
                  subtitle: Text('Your phone and computer share likes, playlists, history and follows. Nothing is lost.', style: TextStyle(color: tok.textDim)),
                  trailing: const Icon(Icons.chevron_right_rounded),
                  onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => SyncScreen(player: widget.player))),
                ),
                SwitchListTile(
                  title: const Text('Keep my devices in sync'),
                  subtitle: Text('Paired devices sync by themselves (at start and every 10 minutes).', style: TextStyle(color: tok.textDim)),
                  value: s.autoSync,
                  onChanged: s.setAutoSync,
                ),
                SwitchListTile(
                  title: const Text('Keep saved playlists up to date'),
                  subtitle: Text('Playlists saved from a friend’s link follow their changes.', style: TextStyle(color: tok.textDim)),
                  value: s.autoPlaylists,
                  onChanged: s.setAutoPlaylists,
                ),
                ListTile(
                  leading: const Icon(Icons.save_alt_rounded),
                  title: const Text('Back up your library'),
                  subtitle: Text('Likes, playlists, history, taste and settings in one file. Keep it safe, or move it to another device.', style: TextStyle(color: tok.textDim)),
                  onTap: () => _backup(context),
                ),
                ListTile(
                  leading: const Icon(Icons.settings_backup_restore_rounded),
                  title: const Text('Restore from a backup'),
                  subtitle: Text('Adds the backup to what’s here: nothing you have now is replaced or lost.', style: TextStyle(color: tok.textDim)),
                  onTap: () => _restore(context),
                ),
              ]),
              _Section('Spotify', [
                ListTile(
                  leading: const Icon(Icons.account_circle_rounded),
                  title: const Text('Spotify account'),
                  subtitle: Text('Read every playlist and Liked Songs to bring them over', style: TextStyle(color: tok.textDim)),
                  trailing: const Icon(Icons.chevron_right_rounded),
                  onTap: () => Navigator.of(context).push(MaterialPageRoute(builder: (_) => const SpotifyScreen())),
                ),
              ]),
              _Section('Performance', [
                SwitchListTile(
                  title: const Text('Show frame times'),
                  subtitle: Text('A small readout of how smooth the app is. If something feels slow, the numbers help find out why.', style: TextStyle(color: tok.textDim)),
                  value: s.perfOverlay,
                  onChanged: s.setPerfOverlay,
                ),
              ]),
              _Section('About', [
                ListTile(title: const Text('Harp $appVersion'), subtitle: Text('No accounts, no servers. Your taste stays on this device.', style: TextStyle(color: tok.textDim))),
                ValueListenableBuilder(
                  valueListenable: Updates.available,
                  builder: (context, up, _) => up == null
                      ? ListTile(
                          leading: const Icon(Icons.system_update_rounded),
                          title: const Text('Check for updates'),
                          onTap: () async {
                            await Updates.check(force: true);
                            if (context.mounted && Updates.available.value == null) toast(context, 'You have the latest Harp');
                          },
                        )
                      : ListTile(
                          leading: Icon(Icons.new_releases_rounded, color: Theme.of(context).colorScheme.primary),
                          title: Text('Update available: ${up.tag}'),
                          subtitle: Text('Opens the release page to download it.', style: TextStyle(color: tok.textDim)),
                          onTap: () => launchUrl(Uri.parse(up.url), mode: LaunchMode.externalApplication),
                        ),
                ),
                ListTile(title: const Text('yt-dlp'), subtitle: Text(_ytdlp, style: TextStyle(color: tok.textDim))),
                ListTile(
                  leading: const Icon(Icons.science_rounded),
                  title: const Text('Test the audio engine'),
                  subtitle: Text('About 30 seconds: plays the current song (or your last one) through each effect and reports what works. Pauses normal playback.', style: TextStyle(color: tok.textDim)),
                  onTap: () => _probe(context),
                ),
                if (Platform.isAndroid)
                  ListTile(
                    title: const Text('Last crash report'),
                    subtitle: Text('If Harp closed on its own, this says why. Nothing is sent anywhere.', style: TextStyle(color: tok.textDim)),
                    trailing: const Icon(Icons.chevron_right_rounded),
                    onTap: () async {
                      final r = await Native.crashReport();
                      if (!context.mounted) return;
                      await showDialog<void>(
                        context: context,
                        builder: (ctx) {
                          final nav = Navigator.of(ctx);
                          return AlertDialog(
                          title: const Text('Last crash report'),
                          content: SingleChildScrollView(child: SelectableText(r.isEmpty ? 'No crash recorded.' : r, style: const TextStyle(fontSize: 11.5, fontFamily: 'monospace'))),
                          actions: [
                            if (r.isNotEmpty) TextButton(onPressed: () => Clipboard.setData(ClipboardData(text: r)).then((_) => nav.pop()), child: const Text('Copy')),
                            if (r.isNotEmpty) TextButton(onPressed: () => Native.clearCrash().then((_) => nav.pop()), child: const Text('Clear')),
                            TextButton(onPressed: () => nav.pop(), child: const Text('Close')),
                          ],
                        );
                        },
                      );
                    },
                  ),
              ]),
            ],
          ),
    );
    if (widget.embedded) {
      return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        const PageTitle('Settings'),
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
        Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 6), child: Text(title.toUpperCase(), style: TextStyle(fontSize: 12.5, fontWeight: FontWeight.w600, letterSpacing: 0.4, color: context.tok.textDim))),
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

class _ThemeChoice extends StatelessWidget {
  const _ThemeChoice({required this.icon, required this.label, required this.selected, required this.onTap});
  final IconData icon;
  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    return Material(
      color: selected ? accent.withValues(alpha: 0.28) : tok.glass.withValues(alpha: context.isDark ? 0.08 : 0.6),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Radii.control),
        side: BorderSide(color: tok.glassEdge.withValues(alpha: selected ? 0.4 : 0.14)),
      ),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 12),
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            Icon(icon, size: 22, color: selected ? tok.text : tok.textDim),
            const SizedBox(height: 4),
            Text(label, style: TextStyle(fontSize: 13, fontWeight: selected ? FontWeight.w700 : FontWeight.w500)),
          ]),
        ),
      ),
    );
  }
}


/// The custom equalizer: ten bands, ±12 dB, applied live as you drag.
void showCustomEq(BuildContext context, AppSettings s) {
  showModalBottomSheet<void>(
    context: context,
    useRootNavigator: true,
    isScrollControlled: true,
    builder: (ctx) => SafeArea(
      child: ListenableBuilder(
        listenable: s,
        builder: (context, _) {
          final tok = context.tok;
          String hz(int f) => f >= 1000 ? '${f ~/ 1000}k' : '$f';
          return Padding(
            padding: const EdgeInsets.fromLTRB(12, 16, 12, 12),
            child: Column(mainAxisSize: MainAxisSize.min, children: [
              Row(children: [
                const Expanded(child: Text('Custom Equalizer', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700))),
                TextButton(onPressed: s.resetCustomEq, child: const Text('Reset')),
              ]),
              Text('Drag a band up or down. Changes apply as you go.', style: TextStyle(color: tok.textDim, fontSize: 13)),
              const SizedBox(height: 8),
              SizedBox(
                height: 230,
                child: Row(children: [
                  for (var i = 0; i < AppSettings.eqBands.length; i++)
                    Expanded(
                      child: Column(children: [
                        Text('${s.customEq[i] > 0 ? '+' : ''}${s.customEq[i].toStringAsFixed(0)}', style: TextStyle(fontSize: 11, color: tok.textDim)),
                        Expanded(
                          child: RotatedBox(
                            quarterTurns: 3,
                            child: Slider(value: s.customEq[i], min: -12, max: 12, divisions: 48, onChanged: (v) => s.setCustomBand(i, v)),
                          ),
                        ),
                        Text(hz(AppSettings.eqBands[i]), style: const TextStyle(fontSize: 11)),
                      ]),
                    ),
                ]),
              ),
            ]),
          );
        },
      ),
    ),
  );
}


/// Search ~9,000 measured headphones and earbuds (AutoEq) and apply one.
void showHeadphonePicker(BuildContext context, AppSettings s) {
  final q = TextEditingController();
  List<api.AutoEqProfileView> hits = const [];
  bool busy = false;
  String? err;
  showModalBottomSheet<void>(
    context: context,
    useRootNavigator: true,
    isScrollControlled: true,
    builder: (ctx) => StatefulBuilder(builder: (ctx, set) {
      final tok = ctx.tok;
      Future<void> run() async {
        set(() { busy = true; err = null; });
        try {
          hits = await api.autoeqSearch(query: q.text);
        } catch (e) {
          err = 'Couldn’t load the list: $e';
        }
        set(() => busy = false);
      }
      return SafeArea(
        child: Padding(
          padding: EdgeInsets.fromLTRB(16, 16, 16, MediaQuery.viewInsetsOf(ctx).bottom + 8),
          child: SizedBox(
            height: MediaQuery.sizeOf(ctx).height * 0.7,
            child: Column(children: [
              const Text('Your headphones', style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
              const SizedBox(height: 4),
              Text('Measured corrections from AutoEq: your model, tuned to sound neutral. Applied before your equalizer.', style: TextStyle(color: tok.textDim, fontSize: 13)),
              const SizedBox(height: 8),
              TextField(
                controller: q,
                autofocus: true,
                decoration: const InputDecoration(hintText: 'Model, e.g. Sony WH-1000XM4, AirPods Pro', prefixIcon: Icon(Icons.search_rounded)),
                onSubmitted: (_) => run(),
              ),
              if (busy) const LinearProgressIndicator(minHeight: 2),
              if (err != null) Padding(padding: const EdgeInsets.all(8), child: Text(err!, style: const TextStyle(color: Colors.redAccent))),
              Expanded(
                child: ListView(children: [
                  for (final h in hits)
                    ListTile(
                      leading: const Icon(Icons.headphones_rounded),
                      title: Text(h.name),
                      subtitle: Text('measured by ${h.source}', style: TextStyle(color: tok.textDim, fontSize: 12.5)),
                      onTap: () async {
                        set(() => busy = true);
                        try {
                          final c = await api.autoeqLoad(name: h.name, source: h.source, path: h.path);
                          await s.setHeadphone('${c.name} (${h.source.split(' on ').first})', c.preampDb, [for (final f in c.filters) (f.kind, f.freq, f.gainDb, f.q)]);
                          if (ctx.mounted) Navigator.pop(ctx);
                        } catch (e) {
                          set(() { busy = false; err = 'Couldn’t load it: $e'; });
                        }
                      },
                    ),
                ]),
              ),
            ]),
          ),
        ),
      );
    }),
  );
}
