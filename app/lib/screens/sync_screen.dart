import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../native.dart';
import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../src/rust/api/party.dart' as net;
import '../theme/glass.dart';
import '../widgets/song_actions.dart';

/// Your devices: pair them once with a link, and they keep each other's
/// library (likes, playlists, history, follows) in sync from then on.
/// Nothing is replaced or lost. Peer to peer, no accounts.
class SyncScreen extends StatefulWidget {
  const SyncScreen({super.key, required this.player, this.invite});
  final PlayerController player;

  /// A sync link that opened Harp: filled in, ready to pair.
  final String? invite;

  @override
  State<SyncScreen> createState() => _SyncScreenState();
}

class _SyncScreenState extends State<SyncScreen> {
  final _link = TextEditingController();
  String? _offer;
  bool _busy = false;
  String? _result;
  String? _error;
  List<net.DeviceView> _devices = [];

  PlayerController get p => widget.player;

  @override
  void initState() {
    super.initState();
    if (widget.invite != null) _link.text = widget.invite!;
    _loadDevices();
  }

  @override
  void dispose() {
    if (_offer != null) net.syncCancel();
    _link.dispose();
    super.dispose();
  }

  Future<void> _loadDevices() async {
    final d = await net.syncDevices();
    if (mounted) setState(() => _devices = d);
  }

  Future<String> _name() async => await api.settingGet(key: 'party_name') ?? (Platform.isAndroid ? 'My phone' : 'My computer');

  String _summary(api.MergeInfoView m) {
    if (m.liked == 0 && m.playlistsNew == 0 && m.playlistsGrown == 0 && m.plays == 0) {
      return 'Paired. Already in sync: nothing new on the other device.';
    }
    return 'Paired. Added ${m.liked} liked songs, ${m.playlistsNew} new playlists'
        '${m.playlistsGrown > 0 ? ', more songs in ${m.playlistsGrown} playlists' : ''}'
        ' and ${m.plays} plays from the other device. Nothing here was changed or lost.';
  }

  void _after(api.MergeInfoView m) {
    p.libraryRev.value++;
    p.savedRev.value++;
    if (mounted) setState(() => _result = _summary(m));
    _loadDevices();
  }

  Future<void> _start() async {
    setState(() { _busy = true; _error = null; _result = null; });
    try {
      final link = await net.syncOffer(name: await _name());
      if (!mounted) return;
      setState(() => _offer = link);
      _after(await net.syncWait());
    } catch (e) {
      if (mounted && !'$e'.contains('cancelled')) setState(() => _error = 'Pairing didn’t finish: $e');
    } finally {
      if (mounted) setState(() { _busy = false; _offer = null; });
    }
  }

  Future<void> _join() async {
    final link = _link.text.trim();
    if (link.isEmpty) return;
    setState(() { _busy = true; _error = null; _result = null; });
    try {
      _after(await net.syncJoin(link: link, name: await _name()));
    } catch (e) {
      if (mounted) setState(() => _error = 'Pairing didn’t finish: $e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _syncNow() async {
    setState(() { _busy = true; _error = null; _result = null; });
    try {
      final lines = await net.syncNow();
      p.libraryRev.value++;
      p.savedRev.value++;
      if (mounted) setState(() => _result = lines.isEmpty ? 'No paired devices yet.' : lines.join('\n'));
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final s = p.settings;
    return Scaffold(
      appBar: AppBar(title: const Text('Sync Your Devices')),
      body: ListenableBuilder(
        listenable: s,
        builder: (context, _) => ListView(padding: const EdgeInsets.fromLTRB(16, 8, 16, 24), children: [
          Text('Pair your phone and computer once; after that they share liked songs, playlists, play history and the artists '
              'you follow, and each keeps everything it had. Peer to peer: no accounts.',
              style: TextStyle(color: tok.textDim)),
          const SizedBox(height: 8),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Keep my devices in sync'),
            subtitle: Text('Syncs with your paired devices when Harp opens and every 10 minutes while both are on. Off: only when you tap Sync now.',
                style: TextStyle(color: tok.textDim, fontSize: 13)),
            value: s.autoSync,
            onChanged: s.setAutoSync,
          ),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Keep saved playlists up to date'),
            subtitle: Text('Playlists you saved from a friend’s link follow their changes. Off: they stay as they are.',
                style: TextStyle(color: tok.textDim, fontSize: 13)),
            value: s.autoPlaylists,
            onChanged: s.setAutoPlaylists,
          ),
          if (_devices.isNotEmpty) ...[
            const SectionHeader('Paired devices'),
            for (final d in _devices)
              ListTile(
                contentPadding: EdgeInsets.zero,
                leading: const Icon(Icons.devices_rounded),
                title: Text(d.name),
                trailing: TextButton(
                  onPressed: () async {
                    await net.syncUnpair(id: d.id);
                    _loadDevices();
                  },
                  child: const Text('Forget'),
                ),
              ),
            FilledButton.icon(onPressed: _busy ? null : _syncNow, icon: const Icon(Icons.sync_rounded), label: const Text('Sync now')),
          ],
          SectionHeader(_devices.isEmpty ? 'Pair a device' : 'Pair another device'),
          if (_offer == null) ...[
            OutlinedButton.icon(onPressed: _busy ? null : _start, icon: const Icon(Icons.qr_code_2_rounded), label: const Text('Show a pairing link on this device')),
            const SizedBox(height: 16),
            const Text('Or paste the link from your other device', style: TextStyle(fontWeight: FontWeight.w700)),
            const SizedBox(height: 8),
            TextField(
              controller: _link,
              decoration: InputDecoration(
                hintText: 'harp://sync/…',
                suffixIcon: IconButton(
                  tooltip: 'Paste',
                  icon: const Icon(Icons.content_paste_rounded),
                  onPressed: () async {
                    final d = await Clipboard.getData(Clipboard.kTextPlain);
                    if (d?.text != null) _link.text = d!.text!.trim();
                  },
                ),
              ),
            ),
            const SizedBox(height: 8),
            OutlinedButton.icon(onPressed: _busy ? null : _join, icon: const Icon(Icons.link_rounded), label: const Text('Pair and sync')),
          ] else
            GlassCard(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                const Text('Waiting for your other device…', style: TextStyle(fontWeight: FontWeight.w700, fontSize: 16)),
                const SizedBox(height: 6),
                Text('On the other device: Settings › Backup › Sync with another device › paste this link. It works once.',
                    style: TextStyle(color: tok.textDim, fontSize: 13.5)),
                const SizedBox(height: 10),
                SelectableText(_offer!, style: TextStyle(color: tok.textDim, fontSize: 12.5)),
                const SizedBox(height: 10),
                Row(children: [
                  Expanded(
                    child: OutlinedButton.icon(
                      onPressed: () async {
                        await Clipboard.setData(ClipboardData(text: _offer!));
                        if (context.mounted) toast(context, 'Link copied');
                      },
                      icon: const Icon(Icons.copy_rounded),
                      label: const Text('Copy'),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: OutlinedButton.icon(
                      onPressed: () async {
                        if (!await Native.share(_offer!)) {
                          await Clipboard.setData(ClipboardData(text: _offer!));
                          if (context.mounted) toast(context, 'Link copied');
                        }
                      },
                      icon: const Icon(Icons.share_rounded),
                      label: const Text('Share'),
                    ),
                  ),
                ]),
                const SizedBox(height: 8),
                const LinearProgressIndicator(minHeight: 2),
                TextButton(onPressed: net.syncCancel, child: const Text('Cancel')),
              ]),
            ),
          if (_busy && _offer == null) const Padding(padding: EdgeInsets.all(16), child: Center(child: CircularProgressIndicator())),
          if (_result != null) Padding(padding: const EdgeInsets.only(top: 16), child: GlassCard(child: Text(_result!))),
          if (_error != null) Padding(padding: const EdgeInsets.only(top: 12), child: Text(_error!, style: const TextStyle(color: Colors.redAccent))),
        ]),
      ),
    );
  }
}
