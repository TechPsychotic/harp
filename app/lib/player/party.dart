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

  /// A guest's say: "controller" (steers and plays songs for everyone) or
  /// "listener" (listens, can suggest).
  String myRole = 'controller';

  /// Host: everyone, with their roles.
  List<party.PartyMemberView> roster = const [];
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
      // Our real position every 2 s (guests fine-tune their speed to it, so
      // it has to be the true one, not an estimate).
      _beat = Timer.periodic(const Duration(seconds: 2), (_) {
        if (player.playing) _publish();
      });
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
      _applyRole('controller');
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

  /// Host: make a guest a controller or a listener.
  Future<void> setRole(party.PartyMemberView m, String r) async {
    await party.partySetRole(id: m.id, role: r);
  }

  /// Guest: what our role lets us do.
  void _applyRole(String r) {
    myRole = r;
    if (r == 'controller') {
      player.steer = (action, ms) => unawaited(party.partyControl(action: action, positionMs: BigInt.from(ms)).catchError((_) {}));
      player.steerPlay = (s) {
        unawaited(party.partyPlay(s: s).catchError((_) {}));
        note.value = 'Playing “${s.title}” for everyone';
      };
    } else {
      // Listeners don't steer: controls do nothing here (the host's music plays on).
      player.steer = (action, ms) => note.value = 'You’re listening: only the host and controllers can change the music';
      player.steerPlay = (s) {
        unawaited(party.partyRequest(s: s).catchError((_) {}));
        note.value = 'Suggested “${s.title}” to the host';
      };
    }
  }

  void _reset() {
    role = 'none';
    link = null;
    members = const [];
    hostName = '';
    player.steer = null;
    player.steerPlay = null;
    player.partyUpNext.value = const [];
    roster = const [];
    myRole = 'controller';
    player.onTransport = null;
    unawaited(player.endFollow());
    _beat?.cancel();
    _beat = null;
    notifyListeners();
  }

  void _publish() {
    if (role != 'host') return;
    unawaited(party.partyPublish(
      state: party.PartyStateView(song: player.current, playing: player.playing, positionMs: BigInt.from(player.position.value.inMilliseconds), upNext: player.upcoming),
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
          // The host's next songs: shown here and fetched ahead, so a song
          // change starts on this device as quickly as on the host's.
          final ids = s.upNext.map((x) => x.id).join(',');
          if (ids != player.partyUpNext.value.map((x) => x.id).join(',')) {
            player.partyUpNext.value = s.upNext;
            unawaited(player.prefetchFor(s.upNext));
          }
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
      case 'play':
        // A controller started a song: it plays for everyone.
        final s = e.song;
        if (s != null && role == 'host') {
          final st = player.steerPlay;
          player.steerPlay = null;
          await player.playSong(s, source: 'party');
          player.steerPlay = st;
          note.value = '${e.name} is playing “${s.title}”';
        }
      case 'roster':
        roster = e.roster;
      case 'role':
        if (role == 'guest') {
          _applyRole(e.action);
          note.value = e.action == 'controller' ? 'You can now steer the music' : 'You’re now a listener';
        }
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
