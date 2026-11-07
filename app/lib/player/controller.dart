import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:media_kit/media_kit.dart';

import '../settings.dart';
import '../src/rust/api/engine.dart' as api;
import 'visualizer.dart';

/// Owns playback: the audio engine (media_kit/libmpv) and the Rust queue and
/// taste model.
///
/// Performance note: playback position changes ~10x a second, so it lives in
/// its own notifiers; `notifyListeners` fires only for real changes (song,
/// queue, play/pause, errors), so the whole UI is not rebuilt on every tick.
class PlayerController extends ChangeNotifier {
  PlayerController(this.settings) {
    viz = Visualizer()
      ..position = (() => position.value)
      ..enabled = settings.reactiveBackground && settings.fullEffects;
    unawaited(viz.attach(_player).then((_) => _applyAudioFx()));
    _subs.addAll([
      _player.stream.position.listen((p) => position.value = p),
      _player.stream.duration.listen((d) => duration.value = d),
      _player.stream.playing.listen((p) {
        playing = p;
        viz.playing = p;
        settings.animating = p && settings.fullEffects;
        notifyListeners();
      }),
      _player.stream.completed.listen((done) {
        if (done && !_loading) _advance(auto: true);
      }),
      _player.stream.error.listen(_onPlayerError),
    ]);
    String lastFx = '${settings.spatial}|${settings.reactiveBackground}|${settings.fullEffects}';
    settings.addListener(() {
      viz.enabled = settings.reactiveBackground && settings.fullEffects;
      final fx = '${settings.spatial}|${settings.reactiveBackground}|${settings.fullEffects}';
      if (fx != lastFx) {
        lastFx = fx;
        _applyAudioFx();
      }
    });
  }

  final AppSettings settings;
  final Player _player = Player();
  late final Visualizer viz;
  final List<StreamSubscription<dynamic>> _subs = [];

  final ValueNotifier<Duration> position = ValueNotifier(Duration.zero);
  final ValueNotifier<Duration> duration = ValueNotifier(Duration.zero);

  /// Bumped when likes, playlists or downloads change, so screens reload.
  final ValueNotifier<int> libraryRev = ValueNotifier(0);

  /// Song ids being downloaded right now.
  final ValueNotifier<Set<String>> downloading = ValueNotifier({});

  api.QueueView queue = const api.QueueView(user: [], autoplay: [], repeat: 'off');
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
  String? _mood; // genre / mood key autoplay should stay in
  Timer? _skipChipTimer;

  api.SongView? get current => queue.current;
  double get progress {
    final d = duration.value.inMilliseconds;
    return d == 0 ? 0 : (position.value.inMilliseconds / d).clamp(0, 1).toDouble();
  }

  bool get currentSuperliked => _currentSuperliked;
  String? get currentReason => current == null ? null : reasons[current!.id];

  /// The loudness analyzer only runs when the animated background is on.
  Future<void> _applyAudioFx() => viz.applyFilters(
        spatial: settings.spatial,
        analyze: settings.reactiveBackground && settings.fullEffects,
      );

  Future<void> init() async {
    queue = await api.queueState();
    resumeOffer = await api.lastPlayed();
    notifyListeners();
  }

  void _bump() => libraryRev.value++;

  // --- starting playback ---

  /// Tapping a song plays that song only.
  Future<void> playSong(api.SongView s, {String source = 'searched'}) {
    _mood = null;
    return _start(s, source);
  }

  Future<void> playSearchResult(api.SongView s) => playSong(s);

  /// Smart shuffle: pick by taste, optionally nudged by a genre, mood or artist.
  Future<void> smartShuffle({String? genre, String? artist}) async {
    if (settings.offline) return playDownloads(shuffle: true);
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
      _mood = genre;
      await _start(pick.song, 'recommended');
    } catch (e) {
      error = '$e';
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> playPlaylist(int id, {bool shuffle = false}) async {
    _mood = null;
    queue = await api.queuePlayPlaylist(id: id, shuffle: shuffle);
    _currentSource = 'queue';
    _currentSuperliked = false;
    await _load();
  }

  /// Plays everything downloaded (works offline).
  Future<void> playDownloads({bool shuffle = true}) async {
    final d = (await api.downloads()).map((e) => e.song).toList();
    if (d.isEmpty) {
      error = 'No downloads yet. Download songs to play them offline.';
      notifyListeners();
      return;
    }
    if (shuffle) d.shuffle();
    await _start(d.first, 'queue');
    for (final s in d.skip(1)) {
      queue = await api.queueAdd(s: s);
    }
    notifyListeners();
  }

  Future<void> resume() async {
    final s = resumeOffer;
    resumeOffer = null;
    if (s != null) await playSong(s);
  }

  Future<void> _start(api.SongView s, String source) async {
    await _finishCurrent(skipped: false);
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
    position.value = Duration.zero;
    duration.value = Duration.zero;
    viz.songSeed = s.id;
    notifyListeners();
    try {
      if (settings.offline && !s.downloaded) {
        throw 'Offline mode: this song is not downloaded.';
      }
      final st = await api.resolveStream(id: s.id, quality: settings.streamQuality);
      await _player.open(Media(st.url, httpHeaders: st.local ? null : {for (final (k, v) in st.headers) k: v}));
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
      if (settings.offline) {
        final inQueue = {queue.current?.id, ...queue.user.map((e) => e.id)};
        final d = (await api.downloads()).map((e) => e.song).where((s) => !inQueue.contains(s.id)).toList()..shuffle();
        queue = await api.queueSetAutoplay(songs: d.take(10).toList());
      } else {
        final q = await api.fillAutoplay(count: 10, genre: _mood);
        for (final s in q.autoplay) {
          reasons.putIfAbsent(s.id, () => 'Up next from your taste');
        }
        queue = q;
      }
      notifyListeners();
    } catch (_) {
      // Offline or yt-dlp broken: what is queued keeps playing.
    }
  }

  // --- controls ---

  Future<void> togglePlay() async {
    if (current == null) return;
    await _player.playOrPause();
  }

  Future<void> seekFraction(double f) => _player.seek(Duration(milliseconds: (duration.value.inMilliseconds * f).round()));

  Future<void> next() => _advance(auto: false);

  Future<void> previous() async {
    if (position.value.inSeconds > 4) {
      await _player.seek(Duration.zero);
      return;
    }
    await _finishCurrent(skipped: true, record: false);
    queue = await api.queuePrevious();
    await _load();
  }

  /// Jump to an item in the queue, skipping the songs before it.
  Future<void> jumpTo(String section, int index) async {
    await _finishCurrent(skipped: true);
    queue = await api.queueJump(section: section, index: index);
    _currentSource = section == 'auto' ? 'recommended' : 'queue';
    _currentSuperliked = false;
    await _load();
  }

  Future<void> _advance({required bool auto}) async {
    final before = queue;
    await _finishCurrent(skipped: !auto);
    final hadNext = before.user.isNotEmpty || before.autoplay.isNotEmpty || before.repeat != 'off';
    queue = hadNext ? await api.queueNext(autoAdvance: auto) : before;
    if (!hadNext) {
      if (settings.offline) {
        await _topUpAutoplay();
        queue = await api.queueNext(autoAdvance: auto);
      } else {
        final pick = await api.recommendNext(genre: _mood);
        if (pick != null) {
          reasons[pick.song.id] = pick.reason;
          queue = await api.queuePlayNow(s: pick.song);
        }
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
  Future<void> _finishCurrent({required bool skipped, bool record = true}) async {
    final s = queue.current;
    if (s == null || !record) return;
    final completion = duration.value.inMilliseconds == 0 ? 0.0 : progress;
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

  // --- likes, downloads ---

  Future<void> toggleLike([api.SongView? s]) async {
    final song = s ?? current;
    if (song == null) return;
    final liked = await api.toggleLike(s: song);
    queue = await api.queueState();
    if (!liked) _currentSuperliked = false;
    _bump();
    notifyListeners();
  }

  /// A superlike counts four times as much and seeds future picks first.
  Future<void> superlike() async {
    final s = current;
    if (s == null) return;
    _currentSuperliked = true;
    if (!s.liked) await api.toggleLike(s: s);
    queue = await api.queueState();
    _bump();
    notifyListeners();
  }

  Future<String?> download(api.SongView s) async {
    if (downloading.value.contains(s.id)) return null;
    downloading.value = {...downloading.value, s.id};
    try {
      await api.downloadSong(s: s, quality: settings.downloadQuality);
      queue = await api.queueState();
      return null;
    } catch (e) {
      return "Couldn't download: $e";
    } finally {
      downloading.value = {...downloading.value}..remove(s.id);
      _bump();
      notifyListeners();
    }
  }

  Future<void> removeDownload(api.SongView s) async {
    await api.deleteDownload(id: s.id);
    queue = await api.queueState();
    _bump();
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
    viz.dispose();
    _player.dispose();
    position.dispose();
    duration.dispose();
    libraryRev.dispose();
    downloading.dispose();
    super.dispose();
  }
}
