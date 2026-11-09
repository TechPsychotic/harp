import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart' show Color;
import 'package:flutter/foundation.dart';
import 'package:media_kit/media_kit.dart';

import '../native.dart';
import '../settings.dart';
import '../theme/art_color.dart';
import '../theme/glass.dart';
import '../src/rust/api/engine.dart' as api;
import '../widgets/cover.dart';
import 'media_session.dart';
import 'visualizer.dart';

/// Owns playback: the audio engine (media_kit/libmpv), the Rust queue and taste
/// model, and the operating system's media session.
///
/// Ordering rules (these fix songs starting out of order):
/// * every load gets a number; a load that is no longer the newest is abandoned
///   before it can start playing;
/// * rapid skips are coalesced: with five skips queued only the last song is
///   loaded, the ones in between are stepped over silently;
/// * an end-of-file event only counts when the song really reached its end.
///
/// Performance note: the playback position changes ~10x a second, so it lives
/// in its own notifiers and `notifyListeners` fires only for real changes.
class PlayerController extends ChangeNotifier {
  PlayerController(this.settings, this.session) {
    viz = Visualizer()
      ..position = (() => position.value)
      ..enabled = settings.reactiveBackground && settings.fullEffects;
    unawaited(viz.attach(_player, native: fxSupported).then((_) => _applyAudioFx()));
    _subs.addAll([
      _player.stream.position.listen((p) => position.value = p),
      _player.stream.duration.listen((d) => duration.value = d),
      _player.stream.playing.listen((p) {
        if (!p) _saveSpot();
        playing = p;
        viz.playing = p;
        settings.animating = p && settings.fullEffects;
        Native.wakeLock(p);
        _syncSession();
        notifyListeners();
      }),
      _player.stream.completed.listen((done) {
        // Only a real end counts: not the old file being replaced.
        final d = duration.value.inMilliseconds;
        if (done && !_loading && d > 0 && position.value.inMilliseconds >= d - 2500) {
          if (sleepAfterSong) {
            // The sleep timer said "at the end of this song".
            cancelSleep();
            return;
          }
          _pendingAuto = true;
          unawaited(_drain());
        }
      }),
      _player.stream.error.listen(_onPlayerError),
    ]);
    String lastFx = _fxKey();
    settings.addListener(() {
      viz.enabled = settings.reactiveBackground && settings.fullEffects;
      final fx = _fxKey();
      if (fx != lastFx) {
        lastFx = fx;
        unawaited(_applyAudioFx(reopen: true));
      }
    });
    session.onPlay = () => _player.play();
    session.onPause = () => _player.pause();
    session.onNext = next;
    session.onPrevious = previous;
    session.onSeek = (d) => _player.seek(d);
    session.onStop = () => _player.pause();
  }

  final AppSettings settings;
  final HarpAudioHandler session;
  final Player _player = Player();
  late final Visualizer viz;
  final List<StreamSubscription<dynamic>> _subs = [];

  /// Audio filters (spatial effects, loudness analysis) run in libmpv. They are
  /// verified on Linux; the Android build of libmpv stalls with them, so there
  /// the background uses its built-in pulse instead.
  static bool get fxSupported => !Platform.isAndroid && !Platform.isIOS;

  final ValueNotifier<Duration> position = ValueNotifier(Duration.zero);
  final ValueNotifier<Duration> duration = ValueNotifier(Duration.zero);

  /// Bumped when likes, playlists or downloads change, so screens reload.
  final ValueNotifier<int> libraryRev = ValueNotifier(0);

  /// The playing song's artwork colour (the app tints itself with it).
  final ValueNotifier<Color> accent = ValueNotifier(HarpColors.harpRed);

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

  /// The song that is actually loaded in the player (a restored queue starts with none).
  String? _loadedId;

  /// Where the restored song was when the app closed: picked up on the first play.
  (String, int)? _resumeAt;
  Timer? _spotTimer;

  /// Sleep timer.
  DateTime? sleepAt;
  bool sleepAfterSong = false;
  Timer? _sleepTimer;

  int _loadGen = 0;
  int _pendingSkips = 0;
  bool _pendingAuto = false;
  bool _draining = false;
  int _errorStreak = 0;

  api.SongView? get current => queue.current;
  double get progress {
    final d = duration.value.inMilliseconds;
    return d == 0 ? 0 : (position.value.inMilliseconds / d).clamp(0, 1).toDouble();
  }

  bool get currentSuperliked => _currentSuperliked;
  String? get currentReason => current == null ? null : reasons[current!.id];

  String _fxKey() => '${settings.spatial}|${settings.reactiveBackground}|${settings.fullEffects}';

  /// The audio filter chain. Changing it mid-song makes libmpv rebuild the audio
  /// pipeline, which on some systems leaves playback stalled, so after a change
  /// the song is reopened at the same position (and resumes by itself).
  Future<void> _applyAudioFx({bool reopen = false}) async {
    if (!fxSupported) return;
    await viz.applyFilters(spatial: settings.spatial, analyze: settings.reactiveBackground && settings.fullEffects);
    if (reopen && current != null && !_loading) {
      await _reopenAt(position.value);
    }
  }

  Future<void> _reopenAt(Duration at) async {
    final s = current;
    if (s == null) return;
    final wasPlaying = playing;
    final gen = _loadGen;
    _loading = true;
    try {
      final st = await api.resolveStream(id: s.id, quality: settings.streamQuality);
      if (gen != _loadGen) return;
      await _player.open(Media(st.url, start: at, httpHeaders: st.local ? null : {for (final (k, v) in st.headers) k: v}), play: wasPlaying);
    } catch (_) {
      // Leave the song as it was.
    } finally {
      _loading = false;
    }
  }

  Future<void> init() async {
    // The queue comes back as it was; the song waits, paused, where it stopped.
    queue = await api.queueState();
    final spot = await api.settingGet(key: 'resume_spot');
    if (spot != null && spot.contains('|')) {
      final i = spot.lastIndexOf('|');
      final ms = int.tryParse(spot.substring(i + 1)) ?? 0;
      _resumeAt = (spot.substring(0, i), ms);
    }
    if (queue.current == null) {
      resumeOffer = await api.lastPlayed();
    } else {
      _publishNowPlaying(queue.current!);
      session.setState(playing: false, loading: false, position: Duration(milliseconds: _resumeAt?.$2 ?? 0));
      viz.songSeed = queue.current!.id;
      unawaited(_updateAccent(queue.current!));
    }
    _spotTimer = Timer.periodic(const Duration(seconds: 15), (_) {
      if (playing) _saveSpot();
    });
    notifyListeners();
  }

  /// Remembers the song and position, so a restart picks up where you were.
  void _saveSpot() {
    final s = current;
    if (s == null || _loadedId != s.id) return;
    unawaited(api.settingSet(key: 'resume_spot', value: '${s.id}|${position.value.inMilliseconds}'));
  }

  // --- sleep timer ---

  /// Pause after [d] (null cancels).
  void setSleep(Duration? d) {
    _sleepTimer?.cancel();
    sleepAfterSong = false;
    sleepAt = d == null ? null : DateTime.now().add(d);
    if (d != null) {
      _sleepTimer = Timer(d, () {
        _player.pause();
        sleepAt = null;
        notifyListeners();
      });
    }
    notifyListeners();
  }

  /// Pause when the current song ends.
  void setSleepAfterSong() {
    _sleepTimer?.cancel();
    sleepAt = null;
    sleepAfterSong = true;
    notifyListeners();
  }

  void cancelSleep() {
    _sleepTimer?.cancel();
    sleepAt = null;
    if (sleepAfterSong) {
      sleepAfterSong = false;
      _player.pause();
    }
    notifyListeners();
  }

  void _bump() => libraryRev.value++;

  void _syncSession() {
    final s = current;
    if (s == null) {
      session.setIdle();
      return;
    }
    session.setState(playing: playing, loading: loading, position: position.value);
  }

  Future<void> _updateAccent(api.SongView s) async {
    final path = ThumbCache.peek(s.thumbnail) ?? await ThumbCache.path(s.thumbnail);
    if (path.isEmpty) return;
    final c = await ArtColor.of(path);
    if (c != null && current?.id == s.id) accent.value = c;
  }

  void _publishNowPlaying(api.SongView s) {
    final local = ThumbCache.peek(s.thumbnail);
    session.setNowPlaying(
      id: s.id,
      title: s.title,
      artist: s.artist,
      album: s.album,
      duration: s.durationSecs == null ? null : Duration(seconds: s.durationSecs!),
      art: local != null ? Uri.file(local) : Uri.tryParse(s.thumbnail),
    );
  }

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
    await _supersede();
    queue = await api.queuePlayPlaylist(id: id, shuffle: shuffle);
    await _afterQueueReplaced();
  }

  /// Plays a list (an album, a podcast, a playlist from a link).
  Future<void> playList(List<api.SongView> songs, {bool shuffle = false}) async {
    if (songs.isEmpty) return;
    _mood = null;
    await _supersede();
    queue = await api.queuePlayList(songs: songs, shuffle: shuffle);
    await _afterQueueReplaced();
  }

  Future<void> _afterQueueReplaced() async {
    _currentSource = 'queue';
    _currentSuperliked = false;
    skippedRecently = null;
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
    await playList(d, shuffle: shuffle);
  }

  Future<void> resume() async {
    final s = resumeOffer;
    resumeOffer = null;
    if (s != null) await playSong(s);
  }

  /// A direct command (play this, previous...) cancels pending skips and any
  /// load still in flight.
  Future<void> _supersede() async {
    _pendingSkips = 0;
    _pendingAuto = false;
    _loadGen++;
    await _finishCurrent(skipped: false);
  }

  Future<void> _start(api.SongView s, String source) async {
    await _supersede();
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
    final gen = ++_loadGen;
    _loading = true;
    loading = true;
    error = null;
    position.value = Duration.zero;
    duration.value = Duration.zero;
    viz.songSeed = s.id;
    _publishNowPlaying(s);
    unawaited(_updateAccent(s));
    _syncSession();
    notifyListeners();
    var superseded = false;
    try {
      if (settings.offline && !s.downloaded) {
        throw 'Offline mode: this song is not downloaded.';
      }
      final st = await api.resolveStream(id: s.id, quality: settings.streamQuality);
      if (gen != _loadGen || _pendingSkips > 0) {
        superseded = true; // a newer song was asked for while this one was resolving
        return;
      }
      // A restored song starts where it was when the app closed.
      final resume = _resumeAt != null && _resumeAt!.$1 == s.id && _resumeAt!.$2 > 5000 ? Duration(milliseconds: _resumeAt!.$2) : null;
      _resumeAt = null;
      await _player.open(Media(st.url, start: resume, httpHeaders: st.local ? null : {for (final (k, v) in st.headers) k: v}));
      if (gen != _loadGen) {
        superseded = true;
        return;
      }
      _loadedId = s.id;
      _retried = false;
      _errorStreak = 0;
      unawaited(api.saveLastPlayed(s: s));
      unawaited(_topUpAutoplay());
      _prefetch();
    } catch (e) {
      if (gen == _loadGen) {
        error = "Couldn't play “${s.title}”: ${_friendly('$e')}";
        _errorStreak++;
        // A broken song should not stall the queue: move on (a few times at most).
        if (_errorStreak <= 3 && !settings.offline || _errorStreak <= 1) {
          Future.delayed(const Duration(milliseconds: 1500), () {
            if (gen == _loadGen && error != null) unawaited(next());
          });
        }
      }
    } finally {
      if (!superseded && gen == _loadGen) {
        _loading = false;
        loading = false;
        _syncSession();
        notifyListeners();
      }
    }
  }

  static String _friendly(String e) {
    final l = e.toLowerCase();
    if (l.contains('502') || l.contains('tunnel') || l.contains('timed out') || l.contains('urlopen') || l.contains('network')) {
      return 'the connection dropped. Check your internet and try again.';
    }
    if (l.contains('not available') || l.contains('private') || l.contains('removed')) return 'this video is not available.';
    return e.length > 160 ? '${e.substring(0, 160)}…' : e;
  }

  /// Fetches the next songs (and this one) into the play cache so skips are
  /// instant and playback survives a dropped connection or a sleeping screen.
  void _prefetch() {
    if (settings.cacheMb <= 0 || settings.offline) return;
    unawaited(_prefetchIfAllowed());
  }

  Future<void> _prefetchIfAllowed() async {
    // Don't fetch ahead on mobile data unless allowed in Settings.
    if (!settings.prefetchOnMobile && await Native.isMetered()) return;
    final ids = <String>[
      ...queue.user.take(2).map((s) => s.id),
      ...queue.autoplay.take(3).map((s) => s.id),
      if (current != null) current!.id,
    ];
    if (ids.isNotEmpty) await api.prefetchSongs(ids: ids, quality: settings.streamQuality, limitMb: settings.cacheMb);
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
      _prefetch();
    } catch (_) {
      // Offline or yt-dlp broken: what is queued keeps playing.
    }
  }

  // --- controls ---

  Future<void> togglePlay() async {
    if (current == null) return;
    // After a restart the queue is back but nothing is loaded yet.
    if (_loadedId != current!.id) {
      _pendingSkips = 0;
      await _load();
      return;
    }
    await _player.playOrPause();
  }

  Future<void> seekFraction(double f) => _player.seek(Duration(milliseconds: (duration.value.inMilliseconds * f).round()));
  Future<void> seekTo(Duration d) => _player.seek(d);

  /// Skip. Several quick taps are folded into one jump to the last song.
  Future<void> next() {
    _pendingSkips++;
    return _drain();
  }

  Future<void> previous() async {
    if (position.value.inSeconds > 4) {
      await _player.seek(Duration.zero);
      return;
    }
    await _supersede();
    queue = await api.queuePrevious();
    await _load();
  }

  /// Jump to an item in the queue, skipping the songs before it.
  Future<void> jumpTo(String section, int index) async {
    await _supersede();
    queue = await api.queueJump(section: section, index: index);
    _currentSource = section == 'auto' ? 'recommended' : 'queue';
    _currentSuperliked = false;
    await _load();
  }

  /// Runs queued skips (and a natural end of song) one after another, loading
  /// only the song we end up on.
  Future<void> _drain() async {
    if (_draining) return;
    _draining = true;
    try {
      while (_pendingSkips > 0 || _pendingAuto) {
        final auto = _pendingAuto && _pendingSkips == 0;
        if (_pendingSkips > 0) {
          _pendingSkips--;
        } else {
          _pendingAuto = false;
        }
        await _step(auto: auto);
        if (_pendingSkips > 0) continue; // more skips arrived: do not load this one
        await _load();
      }
    } finally {
      _draining = false;
    }
  }

  /// Moves the queue on by one song without loading it.
  Future<void> _step({required bool auto}) async {
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
    if (skipped && completion < 0.5 && completion > 0) {
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
      return "Couldn't download: ${_friendly('$e')}";
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
    _prefetch();
  }

  Future<void> addToQueue(api.SongView s) async {
    queue = await api.queueAdd(s: s);
    notifyListeners();
    _prefetch();
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
    // An expired stream URL: resolve again once.
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
    _sleepTimer?.cancel();
    _spotTimer?.cancel();
    _saveSpot();
    viz.dispose();
    _player.dispose();
    position.dispose();
    duration.dispose();
    libraryRev.dispose();
    downloading.dispose();
    accent.dispose();
    super.dispose();
  }
}
