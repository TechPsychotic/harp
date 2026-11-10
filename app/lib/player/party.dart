import 'dart:async';

import 'package:flutter/foundation.dart';

import '../src/rust/api/engine.dart' as api;
import '../src/rust/api/party.dart' as party;
import 'controller.dart';

/// Listening together (roadmap M7). Host a party and share the link, or join
/// one. The host's player is the truth; guests follow it, and their own play,
/// pause, skip and seek steer the host. Peer to peer over iroh (see the
/// `harp-party` crate): no accounts and no servers of our own.
class PartyController extends ChangeNotifier {
  PartyController(this.player) {
    _events = party.partyEvents().listen(_onEvent, onError: (_) {});
  }

  final PlayerController player;
  StreamSubscription<party.PartyEventView>? _events;
  Timer? _beat;

  /// "none", "host" or "guest".
  String role = 'none';
  String? link;
  String hostName = '';
  List<String> members = const [];
  bool busy = false;
  String? error;

  /// Short notes for the screen ("Ana joined", "Ana suggests …").
  final ValueNotifier<String?> note = ValueNotifier(null);

  bool get inParty => role != 'none';

  Future<void> host(String name) async {
    busy = true;
    error = null;
    notifyListeners();
    try {
      link = await party.partyHost(name: name);
      role = 'host';
      player.onTransport = _publish;
      _publish();
      // Belt and braces: the network side repeats the state while playing;
      // this also catches buffering and drift on our side.
      _beat = Timer.periodic(const Duration(seconds: 15), (_) => _publish());
    } catch (e) {
      error = 'Couldn’t start the party: $e';
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  Future<void> join(String inviteLink, String name) async {
    busy = true;
    error = null;
    notifyListeners();
    try {
      await party.partyJoin(link: inviteLink, name: name);
      role = 'guest';
      player.steer = (action, ms) => unawaited(party.partyControl(action: action, positionMs: BigInt.from(ms)).catchError((_) {}));
    } catch (e) {
      error = 'Couldn’t join: $e';
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  Future<void> leave() async {
    await party.partyLeave();
    _reset();
  }

  /// Guest: suggest a song to the host.
  Future<void> suggest(api.SongView s) => party.partyRequest(s: s);

  void _reset() {
    role = 'none';
    link = null;
    members = const [];
    hostName = '';
    player.steer = null;
    player.onTransport = null;
    _beat?.cancel();
    _beat = null;
    notifyListeners();
  }

  void _publish() {
    if (role != 'host') return;
    unawaited(party.partyPublish(
      state: party.PartyStateView(song: player.current, playing: player.playing, positionMs: BigInt.from(player.position.value.inMilliseconds)),
    ).catchError((_) {}));
  }

  Future<void> _onEvent(party.PartyEventView e) async {
    switch (e.kind) {
      case 'members':
        members = e.members;
      case 'guest_joined':
        note.value = '${e.name} joined';
        _publish();
      case 'guest_left':
        note.value = '${e.name} left';
      case 'connected':
        hostName = e.name;
        note.value = 'Listening with ${e.name}';
      case 'state':
        final s = e.state;
        if (s != null && role == 'guest') {
          await player.followHost(song: s.song, playing: s.playing, position: Duration(milliseconds: s.positionMs.toInt()), host: hostName);
        }
      case 'control':
        // A guest pressed something: act here (we're the host); everyone follows.
        final steer = player.steer;
        player.steer = null;
        switch (e.action) {
          case 'play':
            if (!player.playing) await player.togglePlay();
          case 'pause':
            if (player.playing) await player.togglePlay();
          case 'next':
            await player.next();
          case 'previous':
            await player.previous();
          case 'seek':
            await player.seekTo(Duration(milliseconds: e.positionMs.toInt()));
        }
        player.steer = steer;
      case 'request':
        final s = e.song;
        if (s != null) {
          await player.playNext(s);
          note.value = '${e.name} added “${s.title}” to play next';
        }
      case 'ended':
        note.value = e.reason;
        _reset();
        return;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _events?.cancel();
    _beat?.cancel();
    note.dispose();
    super.dispose();
  }
}
