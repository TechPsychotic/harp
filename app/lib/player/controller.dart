import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:media_kit/media_kit.dart';

import '../src/rust/api/engine.dart' as api;

/// Owns playback: the audio engine (media_kit/libmpv) and the Rust queue and
/// taste model. The UI listens to this.
class PlayerController extends ChangeNotifier {
  PlayerController() {
    _subs.addAll([
      _player.stream.position.listen((p) {
        position = p;
        notifyListeners();
      }),
      _player.stream.duration.listen((d) {
        duration = d;
        notifyListeners();
      }),
      _player.stream.playing.listen((p) {
        playing = p;
        notifyListeners();
      }),
      _player.stream.completed.listen((done) {
        if (done && !_loading) _onCompleted();
      }),
      _player.stream.error.listen(_onPlayerError),
    ]);
  }

  final Player _player = Player();
  final List<StreamSubscription<dynamic>> _subs = [];

  api.QueueView queue = const api.QueueView(user: [], autoplay: [], repeat: 'off');
  Duration position = Duration.zero;
  Duration duration = Duration.zero;
  bool playing = false;
  bool loading = false;
  String? error;

  /// Why each recommended song was picked.
  final Map<String, String> reasons = {};

  /// Set after a quick skip so the UI can offer "why?" chips.
  api.SongView? skippedRecently;

  /// Offered on launch if something was playing last time.
  api.SongView? resumeOffer;

  bool _loading = false;
  bool _retried = false;
  bool _currentSuperliked = false;
  String _currentSource = 'recommended';
  Timer? _skipChipTimer;

  api.SongView? get current => queue.current;
  bool get currentLiked => current?.liked ?? false;
  bool get currentSuperliked => _currentSuperliked;
  double get progress =>
      duration.inMilliseconds == 0 ? 0 : (position.inMilliseconds / duration.inMilliseconds).clamp(0, 1).toDouble();
  String? get currentReason => current == null ? null : reasons[current!.id];

  Future<void> init() async {
    queue = await api.queueState();
    resumeOffer = await api.lastPlayed();
    notifyListeners();
  }

  // --- starting playback ---

  /// Tapping a search result plays that song only.
  Future<void> playSearchResult(api.SongView s) => _start(s, 'searched');

  /// Smart shuffle: pick by taste, optionally nudged by a genre or artist.
  Future<void> smartShuffle({String? genre, String? artist}) async {
    loading = true;
    error = null;
    notifyListeners();
    try {
      final pick = await api.recommendNext(genre: genre, artist: artist);
      if (pick == null) {
        error = 'Nothing found. Check your connection.';
        return;
      }
      reasons[pick.song.id] = pick.reason;
      await _start(pick.song, 'recommended');
    } catch (e) {
      error = '$e';
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> resume() async {
    final s = resumeOffer;
    resumeOffer = null;
    if (s != null) await _start(s, 'searched');
  }

  Future<void> _start(api.SongView s, String source) async {
    await _finishCurrent(skipped: false, record: true);
    _currentSource = source;
    _currentSuperliked = false;
    skippedRecently = null;
    queue = await api.queuePlayNow(s: s);
    resumeOffer = null;
    await _load();
  }

  Future<void> _load() async {
    final s = queue.current;
    if (s == null) return;
    _loading = true;
    loading = true;
    error = null;
    position = Duration.zero;
    duration = Duration.zero;
    notifyListeners();
    try {
      final st = await api.resolveStream(id: s.id);
      await _player.open(Media(st.url, httpHeaders: {for (final (k, v) in st.headers) k: v}));
      _retried = false;
      unawaited(api.saveLastPlayed(s: s));
      unawaited(_topUpAutoplay());
    } catch (e) {
      error = "Couldn't play this song: $e";
    } finally {
      _loading = false;
      loading = false;
      notifyListeners();
    }
  }

  Future<void> _topUpAutoplay() async {
    if (queue.autoplay.length >= 3) return;
    try {
      final q = await api.fillAutoplay(count: 8);
      // Remember why: the Rust side only returns songs, so mark them generically.
      for (final s in q.autoplay) {
        reasons.putIfAbsent(s.id, () => 'Up next from your taste');
      }
      queue = q;
      notifyListeners();
    } catch (_) {
      // Offline or yt-dlp broken: playback of what is queued continues.
    }
  }

  // --- controls ---

  Future<void> togglePlay() async {
    if (current == null) return;
    await _player.playOrPause();
  }

  Future<void> seekFraction(double f) =>
      _player.seek(Duration(milliseconds: (duration.inMilliseconds * f).round()));

  Future<void> next() => _advance(auto: false);

  Future<void> previous() async {
    // Past the first few seconds, "previous" restarts the song.
    if (position.inSeconds > 4) {
      await _player.seek(Duration.zero);
      return;
    }
    await _finishCurrent(skipped: true, record: false);
    queue = await api.queuePrevious();
    await _load();
  }

  Future<void> _onCompleted() => _advance(auto: true);

  Future<void> _advance({required bool auto}) async {
    final before = queue;
    await _finishCurrent(skipped: !auto, record: true);
    final hadNext = before.user.isNotEmpty || before.autoplay.isNotEmpty || before.repeat != 'off';
    queue = hadNext ? await api.queueNext(autoAdvance: auto) : before;
    if (!hadNext) {
      // Nothing queued: let taste pick something.
      final pick = await api.recommendNext();
      if (pick != null) {
        reasons[pick.song.id] = pick.reason;
        queue = await api.queuePlayNow(s: pick.song);
      }
    }
    _currentSource = _sourceOf(queue.current, before);
    _currentSuperliked = false;
    await _load();
  }

  String _sourceOf(api.SongView? s, api.QueueView before) {
    if (s == null) return 'recommended';
    if (before.autoplay.any((x) => x.id == s.id)) return 'recommended';
    if (before.user.any((x) => x.id == s.id)) return 'queue';
    return reasons.containsKey(s.id) ? 'recommended' : 'queue';
  }

  /// Records how much of the current song was heard.
  Future<void> _finishCurrent({required bool skipped, required bool record}) async {
    final s = queue.current;
    if (s == null || !record) return;
    final completion = duration.inMilliseconds == 0 ? 0.0 : progress;
    try {
      await api.recordPlay(s: s, completion: completion, superliked: _currentSuperliked, source: _currentSource);
    } catch (_) {}
    if (skipped && completion < 0.5) {
      skippedRecently = s;
      _skipChipTimer?.cancel();
      _skipChipTimer = Timer(const Duration(seconds: 12), () {
        skippedRecently = null;
        notifyListeners();
      });
    }
  }

  Future<void> tagSkip(String reason) async {
    final s = skippedRecently;
    skippedRecently = null;
    notifyListeners();
    if (s != null) await api.skipReason(s: s, reason: reason);
  }

  Future<void> toggleLike([api.SongView? s]) async {
    final song = s ?? current;
    if (song == null) return;
    final liked = await api.toggleLike(s: song);
    queue = await api.queueState();
    notifyListeners();
    if (!liked) _currentSuperliked = false;
  }

  /// A superlike counts four times as much and seeds future picks first.
  Future<void> superlike() async {
    final s = current;
    if (s == null) return;
    _currentSuperliked = true;
    if (!s.liked) await api.toggleLike(s: s);
    queue = await api.queueState();
    notifyListeners();
  }

  // --- queue edits ---

  Future<void> playNext(api.SongView s) async {
    queue = await api.queuePlayNext(s: s);
    notifyListeners();
  }

  Future<void> addToQueue(api.SongView s) async {
    queue = await api.queueAdd(s: s);
    notifyListeners();
  }

  Future<void> removeFromQueue(int i) async {
    queue = await api.queueRemove(index: i);
    notifyListeners();
  }

  Future<void> shuffleUpcoming() async {
    queue = await api.queueShuffle();
    notifyListeners();
  }

  Future<void> cycleRepeat() async {
    final next = switch (queue.repeat) { 'off' => 'all', 'all' => 'one', _ => 'off' };
    queue = await api.queueSetRepeat(mode: next);
    notifyListeners();
  }

  void _onPlayerError(String message) {
    if (_loading) return;
    final s = current;
    if (s == null) return;
    // Expired or rejected stream URL: resolve again once.
    if (!_retried) {
      _retried = true;
      api.forgetStream(id: s.id).then((_) => _load());
      return;
    }
    error = "Playback failed: $message";
    notifyListeners();
  }

  @override
  void dispose() {
    for (final s in _subs) {
      s.cancel();
    }
    _skipChipTimer?.cancel();
    _player.dispose();
    super.dispose();
  }
}
