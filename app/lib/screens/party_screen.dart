import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../native.dart';
import '../player/controller.dart';
import '../src/rust/api/engine.dart' as api;
import '../theme/glass.dart';
import '../widgets/song_actions.dart';

/// Listen together: host a party and share the link, or join one. Everyone
/// hears the same song at the same moment; anyone can play, pause, skip and
/// suggest songs. Peer to peer, no accounts.
class PartyScreen extends StatefulWidget {
  const PartyScreen({super.key, required this.player, this.invite});
  final PlayerController player;

  /// A party link that opened Harp: filled in, and offered to join.
  final String? invite;

  @override
  State<PartyScreen> createState() => _PartyScreenState();
}

class _PartyScreenState extends State<PartyScreen> {
  final _name = TextEditingController();
  final _link = TextEditingController();

  PlayerController get p => widget.player;

  @override
  void initState() {
    super.initState();
    api.settingGet(key: 'party_offset').then((v) => p.syncOffsetMs = int.tryParse(v ?? '') ?? 0);
    api.settingGet(key: 'party_name').then((v) {
      if (mounted) _name.text = v ?? (Platform.isAndroid ? 'My phone' : 'My computer');
    });
    p.party.note.addListener(_showNote);
    final inv = widget.invite;
    if (inv != null) {
      _link.text = inv;
      WidgetsBinding.instance.addPostFrameCallback((_) => _offerJoin(inv));
    }
  }

  Future<void> _offerJoin(String link) async {
    if (p.party.inParty || !mounted) return;
    final host = Uri.decodeComponent(RegExp(r'[?&]name=([^&]*)').firstMatch(link)?.group(1) ?? '');
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Join the party?'),
        content: Text(host.isEmpty ? 'Listen together with whoever sent this link.' : 'Listen together with $host.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Not now')),
          FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('Join')),
        ],
      ),
    );
    if (ok == true && mounted) await p.party.join(link, _nameValue);
  }

  @override
  void dispose() {
    p.party.note.removeListener(_showNote);
    _name.dispose();
    _link.dispose();
    super.dispose();
  }

  void _showNote() {
    final n = p.party.note.value;
    if (n != null && mounted) toast(context, n);
  }

  String get _nameValue {
    final n = _name.text.trim();
    api.settingSet(key: 'party_name', value: n);
    return n.isEmpty ? 'Someone' : n;
  }

  @override
  Widget build(BuildContext context) {
    final tok = context.tok;
    final accent = Theme.of(context).colorScheme.primary;
    return Scaffold(
      appBar: AppBar(title: const Text('Listen Together')),
      body: ListenableBuilder(
        listenable: p.party,
        builder: (context, _) {
          final party = p.party;
          return ListView(padding: const EdgeInsets.fromLTRB(16, 8, 16, 24), children: [
            if (!party.inParty) ...[
              Text('Play the same music at the same moment with friends, wherever they are. '
                  'Anyone in the party can play, pause, skip and suggest songs. Peer to peer: no accounts.',
                  style: TextStyle(color: tok.textDim)),
              const SizedBox(height: 16),
              TextField(controller: _name, decoration: const InputDecoration(labelText: 'Your name in the party')),
              const SizedBox(height: 20),
              FilledButton.icon(
                onPressed: party.busy ? null : () => party.host(_nameValue),
                icon: const Icon(Icons.celebration_rounded),
                label: const Text('Host a party'),
              ),
              const SizedBox(height: 24),
              const Text('Join a party', style: TextStyle(fontWeight: FontWeight.w700, fontSize: 16)),
              const SizedBox(height: 8),
              TextField(
                controller: _link,
                decoration: InputDecoration(
                  hintText: 'Paste the invite link (harp://party/…)',
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
              OutlinedButton.icon(
                onPressed: party.busy ? null : () => _link.text.trim().isEmpty ? null : party.join(_link.text.trim(), _nameValue),
                icon: const Icon(Icons.login_rounded),
                label: const Text('Join'),
              ),
              if (party.busy) const Padding(padding: EdgeInsets.all(16), child: Center(child: CircularProgressIndicator())),
            ] else ...[
              GlassCard(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Row(children: [
                    Icon(Icons.graphic_eq_rounded, color: accent),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        party.role == 'host' ? 'You’re hosting' : 'Listening with ${party.hostName.isEmpty ? 'the host' : party.hostName}',
                        style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 17),
                      ),
                    ),
                  ]),
                  const SizedBox(height: 6),
                  Text(
                    party.role == 'host'
                        ? 'What you play, everyone hears. Guests can play, pause, skip and suggest songs.'
                        : 'You hear what the host plays. Your play, pause, skip and seek steer the party; “Suggest to the party” is in every song’s menu.',
                    style: TextStyle(color: tok.textDim, fontSize: 13.5),
                  ),
                ]),
              ),
              if (party.role == 'host' && party.link != null) ...[
                const SizedBox(height: 16),
                const Text('Invite link', style: TextStyle(fontWeight: FontWeight.w700)),
                const SizedBox(height: 6),
                SelectableText(party.link!, style: TextStyle(color: tok.textDim, fontSize: 12.5)),
                const SizedBox(height: 8),
                Row(children: [
                  Expanded(
                    child: OutlinedButton.icon(
                      onPressed: () async {
                        await Clipboard.setData(ClipboardData(text: party.link!));
                        if (context.mounted) toast(context, 'Link copied');
                      },
                      icon: const Icon(Icons.copy_rounded),
                      label: const Text('Copy'),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: FilledButton.icon(
                      onPressed: () async {
                        final text = 'Listen with me on Harp: open Harp › Listen Together › Join and paste this:\n${party.link!}';
                        if (!await Native.share(text)) {
                          await Clipboard.setData(ClipboardData(text: party.link!));
                          if (context.mounted) toast(context, 'Link copied');
                        }
                      },
                      icon: const Icon(Icons.share_rounded),
                      label: const Text('Share'),
                    ),
                  ),
                ]),
              ],
              if (party.role == 'guest') ...[
                const SizedBox(height: 12),
                Text('Sync offset: ${p.syncOffsetMs > 0 ? '+' : ''}${p.syncOffsetMs} ms', style: const TextStyle(fontWeight: FontWeight.w600)),
                Text('If your sound runs behind the host’s (Bluetooth adds delay), move this right; if ahead, left.', style: TextStyle(color: tok.textDim, fontSize: 12.5)),
                StatefulBuilder(
                  builder: (context, set) => Slider(
                    value: p.syncOffsetMs.toDouble(),
                    min: -500,
                    max: 500,
                    divisions: 40,
                    label: '${p.syncOffsetMs} ms',
                    onChanged: (v) => set(() => p.syncOffsetMs = v.round()),
                    onChangeEnd: (v) => api.settingSet(key: 'party_offset', value: '${v.round()}'),
                  ),
                ),
              ],
              SectionHeader('In the party (${party.members.length})'),
              for (final (i, m) in party.members.indexed)
                ListTile(
                  leading: CircleAvatar(backgroundColor: accent.withValues(alpha: 0.2), child: Text(m.isEmpty ? '?' : m.characters.first.toUpperCase())),
                  title: Text(m),
                  subtitle: i == 0 ? const Text('Host') : null,
                ),
              const SizedBox(height: 16),
              OutlinedButton.icon(
                onPressed: party.leave,
                style: OutlinedButton.styleFrom(foregroundColor: Colors.redAccent),
                icon: const Icon(Icons.logout_rounded),
                label: Text(party.role == 'host' ? 'End party' : 'Leave'),
              ),
            ],
            if (party.error != null) Padding(padding: const EdgeInsets.only(top: 12), child: Text(party.error!, style: const TextStyle(color: Colors.redAccent))),
          ]);
        },
      ),
    );
  }
}
