import 'dart:async';
import 'dart:io';

import 'package:audio_service/audio_service.dart';
import 'package:audio_session/audio_session.dart';
import 'package:flutter/material.dart' show Color;
import 'package:flutter/foundation.dart';
import 'package:mpv_audio_kit/mpv_audio_kit.dart' as mak;

import '../errors.dart' as herr;
import '../native.dart';
import '../settings.dart';
import '../theme/art_color.dart';
import '../theme/glass.dart';
import '../src/rust/api/engine.dart' as api;
import '../src/rust/api/party.dart' as net;
import '../widgets/cover.dart';
import 'media_session.dart';
import 'party.dart';
import 'stream_headers.dart';
import 'visualizer.dart';

/// Owns playback: the audio engine (mpv_audio_kit: libmpv with FFmpeg's audio
/// effects, on every platform), the Rust queue and taste
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
/// Where a list being played comes from (shown as "Playing from …").
typedef From = ({String kind, String id, String title});

class PlayerController extends ChangeNotifier {
  PlayerController(this.settings, this.session) {
    viz = Visualizer()
      ..position = (() => position.value)
      ..enabled = settings.reactiveBackground && settings.fullEffects;
    unawaited(viz.attach(_player).then((_) => _applyAudioFx()));
    _bind(_player);
    String lastFx = _fxKey();
    var lastSpeed = settings.speed;
    if (lastSpeed != 1) unawaited(_player.setRate(lastSpeed));
    settings.addListener(() {
      if (settings.speed != lastSpeed) {
        lastSpeed = settings.speed;
        unawaited(_player.setRate(lastSpeed));
        _syncSession();
      }
      viz.enabled = settings.reactiveBackground && settings.fullEffects;
      final fx = _fxKey();
      if (fx != lastFx) {
        lastFx = fx;
        unawaited(_applyAudioFx());
      }
    });
    session.onPlay = () => setPlaying(true);
    session.onPause = () => setPlaying(false);
    session.onNext = next;
    session.onPrevious = previous;
    session.onSeek = seekTo;
    session.onStop = () => setPlaying(false);
    session.onBrowse = _browse;
    session.onPlayFromId = _playFromId;
    unawaited(_watchOutput());
    // Gapless: the next song waits in the engine and starts the moment this one ends.
    unawaited(_player.setGapless(mak.Gapless.weak).catchError((_) {}));
    unawaited(_player.setPrefetchPlaylist(true).catchError((_) {}));
  }

  /// Listens to a player (the current one; a crossfade swaps in another).
  /// The previous player's listeners go, so a song fading out can't move
  /// the queue or the screen.
  void _bind(mak.Player p) {
    for (final s in _playerSubs) {
      s.cancel();
    }
    _playerSubs
      ..clear()
      ..addAll([
        p.stream.position.listen((d) {
          position.value = d;
          _maybeCrossfade(d);
          final dur = duration.value;
          if (_armedId == null && dur > Duration.zero && dur - d < const Duration(seconds: 30) && !_arming) unawaited(_armNext());
          _checkEngineSong();
        }),
        p.stream.duration.listen((d) {
          duration.value = d;
          // Songs from search often come without a length. The notification
          // only draws its progress bar when it knows one, so pass on the real
          // length as soon as the player has it.
          final s = current;
          if (s != null && d > Duration.zero) session.setDuration(s.id, d);
        }),
        p.stream.playing.listen((p) {
          if (!p) _saveSpot();
          if (!p) _watchEnd();
          playing = p;
          viz.playing = p;
          settings.animating = p && settings.fullEffects;
          Native.wakeLock(p);
          if (p) unawaited(_focus?.setActive(true));
          _syncSession();
          onTransport?.call();
          notifyListeners();
        }),
        p.stream.completed.listen((done) {
          // Only a real end counts: not the old file being replaced.
          final d = duration.value.inMilliseconds;
          // A song lined up in the engine takes over by itself (_onEnginePlaylist
          // moves the queue): advancing here too would skip a song, or show one
          // song while another plays.
          final pl = p.state.playlist;
          final linedUp = _armedId != null || _following || pl.index + 1 < pl.items.length;
          // A party guest waits for the host's next song instead of moving on alone.
          if (done && !_loading && !linedUp && steer == null && d > 0 && position.value.inMilliseconds >= d - 2500) {
            if (sleepAfterSong) {
              // The sleep timer said "at the end of this song".
              cancelSleep();
              return;
            }
            _pendingAuto = true;
            unawaited(_drain());
          }
        }),
        p.stream.error.listen(_onPlayerError),
        p.stream.playlist.listen(_onEnginePlaylist),
      ]);
  }

  final List<StreamSubscription<dynamic>> _playerSubs = [];

  AudioSession? _focus;
  bool _resumeAfterInterruption = false;

  /// Like other music apps: pause when headphones are unplugged or a Bluetooth
  /// device drops (so the song doesn't jump to the speaker), pause for calls,
  /// and lower the volume while navigation speaks. Android and Apple only:
  /// desktop systems don't report this.
  Future<void> _watchOutput() async {
    if (!(Platform.isAndroid || Platform.isIOS)) return;
    try {
      final s = await AudioSession.instance;
      await s.configure(const AudioSessionConfiguration.music());
      _focus = s;
      _subs.add(s.becomingNoisyEventStream.listen((_) => _pauseHere()));
      _subs.add(s.interruptionEventStream.listen((e) {
        if (e.begin) {
          if (e.type == AudioInterruptionType.duck) {
            _player.setVolume(35);
          } else if (playing) {
            // Come back after a short interruption (a call), not after another
            // app started its own music.
            _resumeAfterInterruption = e.type == AudioInterruptionType.pause;
            _pauseHere(resumable: true);
          }
        } else {
          _player.setVolume(100);
          if (_resumeAfterInterruption && e.type == AudioInterruptionType.pause) _playHere();
          _resumeAfterInterruption = false;
        }
      }));
    } catch (_) {
      // Without it playback still works; it just won't react to the output changing.
    }
  }

  final AppSettings settings;
  final HarpAudioHandler session;
  mak.Player _player = mak.Player();

  /// Listening together (host or join a party).
  late final PartyController party = PartyController(this);
  String? _userAgent;
  late final Visualizer viz;
  final List<StreamSubscription<dynamic>> _subs = [];

  /// Audio effects (equalizer, spatial, even volume) run in the engine on every
  /// platform since the switch to mpv_audio_kit (checked on Android and Linux).
  static bool get fxSupported => true;

  final ValueNotifier<Duration> position = ValueNotifier(Duration.zero);
  final ValueNotifier<Duration> duration = ValueNotifier(Duration.zero);

  /// Bumped when likes, playlists or downloads change, so screens reload.
  final ValueNotifier<int> libraryRev = ValueNotifier(0);

  Timer? _syncTimer;
  bool _syncing = false;

  /// Keeps paired devices and followed playlists up to date (each only if
  /// its switch is on in Settings › Backup).
  Future<void> syncInBackground() async {
    if (_syncing || settings.offline) return;
    _syncing = true;
    try {
      final hasNet = await net.netStartIfNeeded();
      if (!hasNet && !settings.autoPlaylists) return;
      var changed = false;
      if (settings.autoPlaylists && (await net.playlistsRefresh()) > 0) changed = true;
      if (settings.autoSync && hasNet) {
        final r = await net.syncNow();
        if (r.any((l) => l.endsWith(': synced'))) changed = true;
      }
      if (changed) {
        libraryRev.value++;
        savedRev.value++;
      }
    } catch (_) {
      // Offline, or the other device is off: next time.
    } finally {
      _syncing = false;
    }
  }

  /// Bumped when you follow an artist or save an album (pages showing those reload).
  final ValueNotifier<int> savedRev = ValueNotifier(0);

  /// Songs that are in at least one of your playlists (their rows show a ✓).
  final ValueNotifier<Set<String>> playlisted = ValueNotifier(const {});

  Future<void> refreshPlaylisted() async {
    try {
      playlisted.value = (await api.playlistedSongIds()).toSet();
    } catch (_) {}
  }

  /// The playing song's artwork colour (the app tints itself with it).
  final ValueNotifier<Color> accent = ValueNotifier(HarpColors.harpRed);

  /// A few colours from the playing song's artwork, for the player's background.
  final ValueNotifier<List<Color>> palette = ValueNotifier(const [HarpColors.harpRed]);

  /// Song ids being downloaded right now.
  final ValueNotifier<Set<String>> downloading = ValueNotifier({});

  api.QueueView queue = const api.QueueView(user: [], context: [], autoplay: [], repeat: 'off');
  bool playing = false;
  bool loading = false;
  String? error;

  /// What actually went wrong (yt-dlp's or the engine's own words), shown when
  /// you tap the error, so a problem can be reported precisely.
  String? errorDetail;

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

  String _fxKey() => '${settings.spatial}|${settings.eq}|${settings.normalize}|${settings.eq == 'custom' ? settings.customEq.join(',') : ''}|${settings.hpName}';

  /// The effects chain. The engine updates it live, mid-song, without a gap.
  Future<void> _applyAudioFx() =>
      viz.applyEffects(
        spatial: settings.spatial,
        eq: settings.eq,
        normalize: settings.normalize,
        custom: settings.customEq,
        freqs: AppSettings.eqBands,
        headphone: settings.hpFilters,
        headphonePreamp: settings.hpPreamp,
      );

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
    unawaited(refreshPlaylisted());
    libraryRev.addListener(refreshPlaylisted);
    // Your devices and friends' playlists: at start, then every 10 minutes.
    unawaited(Future<void>.delayed(const Duration(seconds: 5), syncInBackground));
    _syncTimer = Timer.periodic(const Duration(minutes: 10), (_) => syncInBackground());
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
    _saveLongSpot();
  }

  /// Long tracks (podcast episodes, mixes, audiobooks: 10 minutes or more)
  /// remember where you stopped, each on its own, and start there next time.
  /// Finished (or barely started) ones forget.
  static const _longTrack = Duration(minutes: 10);
  void _saveLongSpot() {
    final id = _loadedId;
    final d = duration.value, pos = position.value;
    if (id == null || d < _longTrack) return;
    final keep = pos > const Duration(seconds: 30) && pos < d - const Duration(seconds: 45);
    unawaited(api.settingSet(key: 'pos:$id', value: keep ? '${pos.inMilliseconds}' : ''));
  }

  // --- sleep timer ---

  /// Pause after [d] (null cancels).
  void setSleep(Duration? d) {
    _sleepTimer?.cancel();
    sleepAfterSong = false;
    sleepAt = d == null ? null : DateTime.now().add(d);
    if (d != null) {
      _sleepTimer = Timer(d, () {
        _pauseHere();
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
      _pauseHere();
    }
    notifyListeners();
  }

  void _bump() => libraryRev.value++;

  String _widgetKey = '';

  /// The home-screen widget follows the song and play state (Android).
  void _syncWidget() {
    final s = current;
    final key = '${s?.id}|$playing';
    if (key == _widgetKey) return;
    _widgetKey = key;
    unawaited(Native.updateWidget(
      title: s?.title ?? 'Harp',
      artist: s?.artist ?? 'Tap to open',
      art: s == null ? '' : (ThumbCache.peek(s.thumbnail) ?? ''),
      playing: playing,
    ));
  }

  void _syncSession() {
    _syncWidget();
    final s = current;
    if (s == null) {
      session.setIdle();
      return;
    }
    session.setState(playing: playing, loading: loading, position: position.value, speed: settings.speed);
  }

  Future<void> _updateAccent(api.SongView s) async {
    final path = ThumbCache.peek(s.thumbnail) ?? await ThumbCache.path(s.thumbnail);
    if (path.isEmpty) return;
    final c = await ArtColor.of(path);
    if (c != null && current?.id == s.id) accent.value = c;
    final pal = await ArtColor.paletteOf(path);
    if (pal.isNotEmpty && current?.id == s.id) palette.value = pal;
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
    // In a party as a controller: the song plays for everyone (through the host).
    if (steerPlay != null) {
      steerPlay!(s);
      return Future.value();
    }
    _mood = null;
    return _start(s, source);
  }

  /// Set while you're a controller in a party: songs you start play for everyone.
  void Function(api.SongView s)? steerPlay;

  /// As a guest: the host's next songs (shown in the party, fetched ahead).
  final ValueNotifier<List<api.SongView>> partyUpNext = ValueNotifier(const []);

  /// What plays next here, for the party's guests to fetch ahead.
  List<api.SongView> get upcoming => [...queue.user, ...queue.context, ...queue.autoplay].take(10).toList();

  Future<void> playSearchResult(api.SongView s) => playSong(s);

  /// Smart shuffle: pick by taste, optionally nudged by a genre, mood or artist.
  /// [label] names it in "Playing from" (a mood's name).
  Future<void> smartShuffle({String? genre, String? artist, String? label}) async {
    if (settings.offline) {
      // No connection: your downloads and own files, ranked by your taste.
      final picks = await api.recommendOffline(count: 40, exclude: const []);
      if (picks.isEmpty) return playDownloads(shuffle: true);
      return playList(picks, from: (kind: 'smart', id: '', title: 'Smart Shuffle (offline)'));
    }
    loading = true;
    error = null;
    errorDetail = null;
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
      // A new start replaced whatever list was playing; say where this comes from.
      queue = await api.queueSetSource(
        kind: genre != null ? 'mood' : (artist != null ? 'radio' : 'smart'),
        id: genre ?? artist ?? '',
        title: label ?? (artist != null ? '$artist Radio' : (genre ?? 'Smart Shuffle')),
      );
    } catch (e) {
      error = '$e';
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> playPlaylist(int id, {bool shuffle = false, bool smart = false}) async {
    _mood = null;
    await _supersede();
    queue = await api.queuePlayPlaylist(id: id, shuffle: shuffle);
    await _afterQueueReplaced();
    if (smart) unawaited(mixInSmartPicks(id));
  }

  /// Smart shuffle: songs like this playlist's, mixed in (one after every
  /// three of its songs) while it plays. Fetched after it started, so the
  /// music doesn't wait.
  Future<void> mixInSmartPicks(int playlistId) async {
    try {
      final picks = await api.playlistSmartPicks(id: playlistId, count: 12);
      final src = queue.source;
      if (picks.isEmpty || src == null || src.kind != 'playlist' || src.id != '$playlistId') return;
      for (final s in picks) {
        reasons[s.id] = 'Smart Shuffle: picked to go with this playlist';
      }
      queue = await api.queueMixIn(songs: picks, every: 3);
      notifyListeners();
      _prefetch();
    } catch (_) {}
  }

  /// Plays a list (an album, a podcast, liked songs, downloads…) as the
  /// context; [from] says where it's from ("Playing from …").
  Future<void> playList(List<api.SongView> songs, {bool shuffle = false, From? from}) async {
    if (songs.isEmpty) return;
    if (steerPlay != null) {
      steerPlay!(shuffle ? (songs.toList()..shuffle()).first : songs.first);
      return;
    }
    _mood = null;
    await _supersede();
    queue = await api.queuePlayList(songs: songs, shuffle: shuffle, sourceKind: from?.kind ?? '', sourceId: from?.id ?? '', sourceTitle: from?.title ?? '');
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
    await playList(d, shuffle: shuffle, from: (kind: 'downloads', id: '', title: 'Downloaded'));
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

  /// Loads the queue's current song. [play]: false loads it paused (a reload
  /// that corrects something must not start music you paused); [at]: where
  /// to start (a reload of the same song carries on where it was).
  Future<void> _load({bool play = true, Duration? at}) async {
    final s = queue.current;
    if (s == null) return;
    final gen = ++_loadGen;
    if (_loadedId != null && _loadedId != s.id) _prevId = _loadedId;
    _userPaused = !play;
    _fadeTried = null;
    _saveLongSpot(); // the song being left, while its position is still known
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
    // Whatever the engine was playing stops now: if this song then fails to
    // load, nothing else may keep playing under its title.
    if (_loadedId != s.id || _player.state.playlist.items.length > 1) {
      _loadedId = null;
      _armedId = null;
      try {
        await _player.pause();
        final pl = _player.state.playlist;
        for (var i = pl.items.length - 1; i > pl.index; i--) {
          await _player.remove(i);
        }
      } catch (_) {}
    }
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
      var resume = at ?? (_resumeAt != null && _resumeAt!.$1 == s.id && _resumeAt!.$2 > 5000 ? Duration(milliseconds: _resumeAt!.$2) : null);
      _resumeAt = null;
      // A long track you stopped partway through starts where you left it.
      if (resume == null && (s.durationSecs == null || s.durationSecs! >= _longTrack.inSeconds)) {
        final ms = int.tryParse(await api.settingGet(key: 'pos:${s.id}') ?? '');
        if (ms != null && ms > 0) resume = Duration(milliseconds: ms);
        if (gen != _loadGen) {
          superseded = true;
          return;
        }
      }
      // play is explicit: this engine doesn't start a newly opened song by
      // itself (the old one did), which left skips and autoplay paused. Paused
      // meanwhile (or loading to stay paused): it opens paused.
      final h = engineHeaders(st.headers, local: st.local);
      if (h.userAgent != null && h.userAgent != _userAgent) {
        await _player.setRawProperty('user-agent', h.userAgent!);
        _userAgent = h.userAgent;
      }
      final opened = !_userPaused;
      await _player.open(mak.Media(st.url, start: resume, httpHeaders: h.headers, extras: {'harpId': s.id}), play: opened);
      _armedId = null; // open() replaced the engine's list
      // Play or pause tapped while it opened: that has the last word.
      if (!_userPaused != opened) await (_userPaused ? _player.pause() : _player.play());
      if (gen != _loadGen) {
        superseded = true;
        return;
      }
      _loadedId = s.id;
      onTransport?.call();
      _retried = false;
      _errorStreak = 0;
      unawaited(api.saveLastPlayed(s: s));
      if (steer == null) unawaited(_topUpAutoplay()); // a party guest's queue is the host's
      _tidyCache();
      _prefetch();
    } catch (e) {
      if (gen == _loadGen) {
        final he = herr.classify('$e');
        error = "Couldn't play “${s.title}” · ${he.code}: ${he.message}";
        errorDetail = '${he.code}\n\n${herr.neutral('$e')}';
        _errorStreak++;
        // Skipping on to the next song would only ask the source again.
        if (_isBotCheck('$e')) _errorStreak = 99;
        // A broken song should not stall the queue: move on (a few times at most).
        // Not as a party guest: that would skip the song for everyone (the
        // host's next update tries this one again).
        if (steer == null && (_errorStreak <= 3 && !settings.offline || _errorStreak <= 1 && !_isBotCheck('$e'))) {
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

  /// The source refusing for asking too much (its own words only: a bare
  /// "429" could be part of a song's id).
  static bool _isBotCheck(String e) => identical(herr.classify(e), herr.sourceBusy);

  /// Fetches the next songs (and this one) into the play cache so skips are
  /// instant and playback survives a dropped connection or a sleeping screen.
  void _prefetch() {
    if (settings.cacheMb <= 0 || settings.offline) return;
    unawaited(_prefetchIfAllowed());
  }

  Timer? _prefetchTimer;

  /// How many songs ahead are kept on the device: five, fewer if the cache
  /// limit in Settings is small (about 10 MB a song at the best quality).
  /// How many songs ahead are kept on the device: as many as Settings asks
  /// for, as long as they fit in the cache (about 10 MB a song at best quality).
  int get _aheadCount => settings.cacheMb <= 0 ? 0 : settings.aheadSongs.clamp(0, settings.cacheMb ~/ 10);

  /// The songs that play next (not ones already downloaded: they're on the
  /// device anyway, and are never fetched again).
  List<api.SongView> get _upcoming {
    final q = queue;
    final out = <api.SongView>[];
    for (final s in [...q.user, ...q.context, ...q.autoplay]) {
      if (out.length >= _aheadCount) break;
      if (!s.downloaded && !out.any((x) => x.id == s.id)) out.add(s);
    }
    return out;
  }

  /// The song before this one keeps its cached file (in case you go back to
  /// it) until this one is over; older cached songs are deleted.
  String? _prevId;

  void _tidyCache() {
    final cur = current;
    if (cur == null) return;
    unawaited(api.cacheKeepOnly(keep: [?_prevId, cur.id, ..._upcoming.map((s) => s.id)]));
  }

  /// Fetches the next few songs onto the device, one at a time and a few
  /// seconds apart (a burst is what makes the source refuse). Waits until
  /// this song has played 6 s, so quick skipping doesn't fetch anything.
  Future<void> _prefetchIfAllowed() async {
    _prefetchTimer?.cancel();
    // A party guest fetches at once: the host already decided what's next.
    final wait = (steer != null ? Duration.zero : const Duration(seconds: 6)) - position.value;
    if (wait > Duration.zero) {
      final id = current?.id;
      _prefetchTimer = Timer(wait, () {
        if (current?.id == id) unawaited(_prefetchIfAllowed());
      });
      return;
    }
    // Don't fetch ahead on mobile data unless allowed in Settings.
    if (!settings.prefetchOnMobile && await Native.isMetered()) return;
    if (api.youtubeBackingOff()) return;
    final ids = _upcoming.map((s) => s.id).toList();
    if (ids.isEmpty) return;
    await api.prefetchSongs(ids: ids, quality: settings.streamQuality, limitMb: settings.cacheMb);
  }

  Future<void> _topUpAutoplay() async {
    if (queue.autoplay.length >= 3) return;
    try {
      if (settings.offline) {
        final inQueue = [?queue.current?.id, ...queue.user.map((e) => e.id), ...queue.context.map((e) => e.id)];
        final picks = await api.recommendOffline(count: 10, exclude: inQueue);
        queue = await api.queueSetAutoplay(songs: picks);
      } else {
        final q = await api.fillAutoplay(count: 10, genre: _mood);
        final why = _mood != null ? '${queue.source?.title ?? _mood} mood · picked for you' : 'Like what you’re playing now';
        for (final s in q.autoplay) {
          reasons.putIfAbsent(s.id, () => why);
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

  // --- gapless ---

  /// The song waiting in the engine to follow this one (null: none).
  String? _armedId;
  bool _arming = false, _armAgain = false;

  /// Queue changes re-arm the next song (cheap when nothing changed).
  @override
  void notifyListeners() {
    super.notifyListeners();
    if (_loadedId != null) unawaited(_armNext());
  }

  /// What plays after this song: your queue, then the list, then autoplay.
  api.SongView? get _upNext {
    final q = queue;
    if (q.repeat == 'one') return q.current;
    return [...q.user, ...q.context, ...q.autoplay].firstOrNull;
  }

  Future<void> _armNext() async {
    if (_arming) {
      _armAgain = true;
      return;
    }
    _arming = true;
    try {
      do {
        _armAgain = false;
        await _armOnce();
      } while (_armAgain);
    } catch (_) {
      // Not armed: the song still moves on the ordinary way when this one ends.
    } finally {
      _arming = false;
    }
  }

  Future<void> _armOnce() async {
    final cur = current;
    // Not as a party guest (the host decides), not mid-load, not while crossfading
    // (then the next song starts on a second player instead).
    var next = (steer != null || cur == null || _loadedId != cur.id || _loading || settings.crossfade > 0) ? null : _upNext;
    // Only a song that's on the device (downloaded or fetched ahead) waits in
    // the engine. A streaming link could expire or fail while it waits, and
    // the engine would then stop on it with the next song on screen. Songs
    // that aren't on the device load the ordinary way when this one ends.
    if (next != null && !next.downloaded && !api.songOnDevice(id: next.id)) next = null;
    if (next?.id == _armedId) return;
    final d = duration.value;
    final left = d - position.value;
    // Never swap what's lined up in the last 3 s: the engine may be moving into it.
    if (_armedId != null && d > Duration.zero && left < const Duration(seconds: 3)) return;
    final pl = _player.state.playlist;
    for (var i = pl.items.length - 1; i > pl.index; i--) {
      await _player.remove(i);
    }
    _armedId = null;
    if (next == null || (settings.offline && !next.downloaded)) return;
    final st = await api.resolveStream(id: next.id, quality: settings.streamQuality);
    if (!st.local) return;
    // The queue may have changed while that resolved.
    if (_upNext?.id != next.id || _loadedId != current?.id || _loading) {
      _armAgain = true;
      return;
    }
    final h = engineHeaders(st.headers, local: st.local);
    await _player.add(mak.Media(st.url, httpHeaders: h.headers, extras: {'harpId': next.id}));
    _armedId = next.id;
  }

  DateTime _lastEngineCheck = DateTime.fromMillisecondsSinceEpoch(0);

  /// The song the engine plays and the one on screen must be the same. If
  /// they ever differ (a race between the queue changing and the engine moving
  /// on), the screen's song is loaded properly.
  void _checkEngineSong() {
    final now = DateTime.now();
    if (now.difference(_lastEngineCheck) < const Duration(seconds: 2)) return;
    _lastEngineCheck = now;
    _watchCache();
    if (_loading || _fading || steer != null) return;
    final cur = current;
    final pl = _player.state.playlist;
    if (cur == null || pl.index >= pl.items.length || pl.items.isEmpty) return;
    final playingId = pl.items[pl.index].extras?['harpId'];
    if (_following || playingId is! String || playingId == cur.id) {
      _mismatchSince = null;
      return;
    }
    // Seen twice in a row (2 s apart), so a song change in progress isn't mistaken
    // for a mismatch: what you hear wins. If that song is coming up, the queue
    // moves to it; otherwise the song on screen is loaded properly.
    final since = _mismatchSince;
    if (since == null || since.$1 != playingId) {
      _mismatchSince = (playingId, now);
      return;
    }
    _mismatchSince = null;
    if (_indexIn(queue, playingId) != null) {
      unawaited(_onGaplessAdvance(playingId));
    } else {
      unawaited(_load(play: !_userPaused));
    }
  }

  (String, DateTime)? _mismatchSince;

  /// Upcoming songs already on this device (fetched ahead or downloaded).
  /// Fetching runs in the background, so this is looked at every 2 s: a song
  /// that just arrived is shown as ready and lined up for a gapless start.
  Set<String> readyAhead = const {};

  void _watchCache() {
    final q = queue;
    final ready = {
      for (final s in [...q.user, ...q.context, ...q.autoplay].take(_aheadCount.clamp(1, 50)))
        if (s.downloaded || api.songOnDevice(id: s.id)) s.id,
    };
    if (ready.length == readyAhead.length && ready.containsAll(readyAhead)) return;
    readyAhead = ready;
    notifyListeners(); // also lines up the next song if it just arrived
  }

  /// True while the queue is catching up with a song the engine moved into.
  bool _following = false;

  /// Where [id] is among the songs coming up: (section, index), or null.
  static (String, int)? _indexIn(api.QueueView q, String id) {
    for (final (name, list) in [('user', q.user), ('context', q.context), ('auto', q.autoplay)]) {
      final i = list.indexWhere((s) => s.id == id);
      if (i >= 0) return (name, i);
    }
    return null;
  }

  /// The engine moved into the armed song by itself: catch the queue up.
  void _onEnginePlaylist(mak.Playlist pl) {
    if (pl.index < 1 || pl.index >= pl.items.length) return;
    final id = pl.items[pl.index].extras?['harpId'];
    if (id is! String || id != _armedId) return;
    unawaited(_onGaplessAdvance(id));
  }

  Future<void> _onGaplessAdvance(String id) async {
    if (_following) return;
    _following = true;
    try {
      await _followEngine(id);
    } finally {
      _following = false;
    }
  }

  Future<void> _followEngine(String id) async {
    _armedId = null;
    if (_loadedId != null && _loadedId != id) _prevId = _loadedId;
    final before = queue;
    if (before.current?.id != id) {
      await _finishCurrent(skipped: false, completion: 1.0); // it played to the end
      // The queue moves to the song the engine is playing: usually the next one,
      // but if the queue changed meanwhile, wherever that song now is.
      final at = _indexIn(before, id);
      queue = at != null ? await api.queueJump(section: at.$1, index: at.$2) : await api.queueNext(autoAdvance: true);
    }
    final s = queue.current;
    if (s == null || s.id != id) {
      // The queue changed at the last moment: load what it says properly.
      await _load(play: !_userPaused);
      return;
    }
    _currentSource = _sourceOf(s, before);
    _currentSuperliked = false;
    skippedRecently = null;
    _loadedId = id;
    viz.songSeed = id;
    _publishNowPlaying(s);
    unawaited(_updateAccent(s));
    _syncSession();
    onTransport?.call();
    unawaited(api.saveLastPlayed(s: s));
    // Keep the engine's list to [this song, the next one].
    try {
      while (_player.state.playlist.index > 0) {
        await _player.remove(0);
      }
    } catch (_) {}
    notifyListeners();
    if (steer == null) unawaited(_topUpAutoplay());
    _tidyCache();
    _prefetch();
    _ensureStarted(id);
  }

  /// After the engine moved on by itself: if the new song hasn't really
  /// started within a few seconds (and you didn't pause), load it again.
  void _ensureStarted(String id) {
    Future<void>.delayed(const Duration(seconds: 5), () {
      if (current?.id != id || _loading || _userPaused || steer != null) return;
      if (position.value < const Duration(seconds: 1)) {
        _pendingSkips = 0;
        unawaited(_load());
      }
    });
  }

  // --- crossfade ---

  bool _fading = false;
  String? _fadeTried;

  void _maybeCrossfade(Duration pos) {
    final secs = settings.crossfade;
    final d = duration.value;
    if (secs <= 0 || _fading || _loading || steer != null || sleepAfterSong || d <= Duration.zero) return;
    if (queue.repeat == 'one' || d - pos > Duration(seconds: secs) || pos < const Duration(seconds: 5)) return;
    final next = _upNext;
    if (next == null || (settings.offline && !next.downloaded)) return;
    // One try per song: retrying on every position tick is what got Harp
    // flagged as a bot.
    final key = '${current?.id}>${next.id}';
    if (key == _fadeTried) return;
    _fadeTried = key;
    unawaited(_crossfade(next, secs));
  }

  /// The next song starts on a second player at volume 0 while this one plays
  /// on; the two volumes cross over [secs] seconds; then the second player is
  /// the player and the first goes.
  Future<void> _crossfade(api.SongView next, int secs) async {
    _fading = true;
    final old = _player;
    mak.Player? b;
    try {
      final st = await api.resolveStream(id: next.id, quality: settings.streamQuality);
      final h = engineHeaders(st.headers, local: st.local);
      b = mak.Player();
      final ua = h.userAgent ?? _userAgent;
      if (ua != null) await b.setRawProperty('user-agent', ua);
      await b.setVolume(0);
      if (settings.speed != 1) await b.setRate(settings.speed);
      await b.open(mak.Media(st.url, httpHeaders: h.headers, extras: {'harpId': next.id}), play: true);
      // The queue moves on now (the song that ends was heard to the end).
      final before = queue;
      await _finishCurrent(skipped: false, completion: 1.0);
      queue = await api.queueNext(autoAdvance: true);
      if (queue.current?.id != next.id) {
        await b.dispose();
        b = null;
        await _load();
        return;
      }
      _player = b;
      _bind(b);
      await viz.attach(b);
      await _applyAudioFx();
      _currentSource = _sourceOf(next, before);
      _currentSuperliked = false;
      skippedRecently = null;
      _loadedId = next.id;
      _armedId = null;
      viz.songSeed = next.id;
      _publishNowPlaying(queue.current!);
      unawaited(_updateAccent(queue.current!));
      onTransport?.call();
      unawaited(api.saveLastPlayed(s: queue.current!));
      notifyListeners();
      unawaited(_topUpAutoplay());
      _prefetch();
      // The crossing.
      const steps = 40;
      for (var i = 1; i <= steps; i++) {
        await Future<void>.delayed(Duration(milliseconds: secs * 1000 ~/ steps));
        final t = i / steps;
        await b.setVolume(100 * t);
        await old.setVolume(100 * (1 - t));
      }
    } catch (_) {
      // Couldn't start the next one: the song ends and moves on the usual way.
      if (b != null && !identical(b, _player)) await b.dispose();
    } finally {
      if (!identical(old, _player)) {
        try {
          await old.dispose();
        } catch (_) {}
      }
      _fading = false;
    }
  }

  // --- Android Auto and other media browsers ---

  /// The library as a browsable tree: folders, then songs.
  Future<List<MediaItem>> _browse(String parent) async {
    MediaItem folder(String id, String title, {String? sub}) => MediaItem(id: id, title: title, artist: sub, playable: false);
    MediaItem songItem(String id, api.SongView s) =>
        MediaItem(id: id, title: s.title, artist: s.artist, album: s.album, artUri: Uri.tryParse(s.thumbnail), playable: true,
            duration: s.durationSecs == null ? null : Duration(seconds: s.durationSecs!));
    List<MediaItem> songs(String prefix, List<api.SongView> l) => [for (var i = 0; i < l.length && i < 200; i++) songItem('$prefix:$i', l[i])];
    try {
      switch (parent) {
        case AudioService.browsableRootId:
          return [
            MediaItem(id: 'smart', title: 'Smart Shuffle', artist: 'Picked from your taste', playable: true),
            folder('liked', 'Liked Songs'),
            folder('playlists', 'Playlists'),
            folder('downloads', 'Downloaded', sub: 'Plays without a connection'),
            folder('history', 'Recently Played'),
          ];
        case AudioService.recentRootId:
          final h = await api.history(limit: 1);
          return songs('hist', h);
        case 'liked':
          return songs('liked', await api.likedSongs());
        case 'downloads':
          return songs('dl', (await api.downloads()).map((d) => d.song).toList());
        case 'history':
          return songs('hist', await api.history(limit: 100));
        case 'playlists':
          return [for (final p in await api.playlists()) folder('pl:${p.id}', p.name, sub: '${p.count} songs')];
      }
      if (parent.startsWith('pl:')) {
        final id = int.parse(parent.substring(3));
        return songs('pl:$id', await api.playlistSongs(id: id));
      }
    } catch (_) {}
    return const [];
  }

  /// Plays what was picked in the car: the song, then the rest of its list.
  Future<void> _playFromId(String id) async {
    if (id == 'smart') return smartShuffle();
    final i = int.tryParse(id.substring(id.lastIndexOf(':') + 1)) ?? 0;
    final kind = id.substring(0, id.lastIndexOf(':'));
    switch (kind) {
      case 'liked':
        final l = await api.likedSongs();
        if (i < l.length) await playList(l.sublist(i), from: (kind: 'liked', id: '', title: 'Liked Songs'));
      case 'dl':
        final l = (await api.downloads()).map((d) => d.song).toList();
        if (i < l.length) await playList(l.sublist(i), from: (kind: 'downloads', id: '', title: 'Downloaded'));
      case 'hist':
        final l = await api.history(limit: 100);
        if (i < l.length) await playList(l.sublist(i), from: (kind: 'history', id: '', title: 'Recently Played'));
      default:
        if (kind.startsWith('pl:')) {
          final pid = int.parse(kind.substring(3));
          final l = await api.playlistSongs(id: pid);
          final name = (await api.playlists()).where((p) => p.id == pid).firstOrNull?.name ?? 'Playlist';
          if (i < l.length) await playList(l.sublist(i), from: (kind: 'playlist', id: '$pid', title: name));
        }
    }
  }

  // --- listening together ---

  /// Set while you're a guest in a party: play, pause, skip and seek go to the
  /// host (whose player acts and tells everyone) instead of acting here.
  void Function(String action, int positionMs)? steer;

  /// Set while you host a party: called after the song, play/pause or the
  /// position changed, so the guests follow.
  VoidCallback? onTransport;

  /// When you pause, playback stopping is fine; when the song just stops near its
  /// end by itself (a lined-up stream that failed to open, say), move on.
  bool _userPaused = false;

  void _watchEnd() {
    final id = current?.id;
    final d = duration.value, pos = position.value;
    if (_userPaused || _loading || steer != null || d <= Duration.zero || d - pos > const Duration(seconds: 3)) return;
    Future<void>.delayed(const Duration(milliseconds: 1800), () {
      if (!playing && !_userPaused && !_loading && current?.id == id && !_draining) {
        _pendingAuto = true;
        unawaited(_drain());
      }
    });
  }

  /// Guest: do what the host's player is doing (without steering it back).
  /// The queue here is the host's ([upNext]: what plays next there), so the
  /// queue sheet shows it and its songs are fetched onto this device ahead.
  ///
  /// Staying in step, the way multi-room speakers do it: a gap over 0.7 s
  /// (e.g. a new song that took a while to load) is closed with one exact
  /// seek; smaller drift by playing up to 5 % faster or slower (the pitch is
  /// kept) until we're back on the host's beat, so there's no jump.
  Future<void> followHost({required api.SongView? song, required bool playing, required Duration position, required String host, List<api.SongView> upNext = const []}) async {
    if (song == null) {
      if (this.playing) await _pauseHere();
      return;
    }
    final received = DateTime.now();
    // Where the host is now (its position moved on while we worked), plus your
    // own offset (Bluetooth adds its own delay that no app can measure).
    Duration hostNow() => (playing ? position + DateTime.now().difference(received) : position) + Duration(milliseconds: syncOffsetMs);
    final ids = upNext.map((s) => s.id).join(',');
    if (current?.id != song.id || _loadedId != song.id) {
      _mood = null;
      await _supersede();
      _currentSource = 'party';
      _currentSuperliked = false;
      skippedRecently = null;
      queue = await api.queueMirror(current: song, upcoming: upNext, title: host);
      _mirrored = ids;
      resumeOffer = null;
      // Starts where the host is, playing or paused as there: no jump from 0.
      final at = hostNow();
      await _load(play: playing, at: at > const Duration(milliseconds: 500) ? at : null);
      if (current?.id != song.id) return;
      final late = hostNow() - this.position.value;
      if (late.abs() > const Duration(milliseconds: 700)) await _player.seek(hostNow(), exact: true);
      return;
    }
    if (ids != _mirrored) {
      _mirrored = ids;
      queue = await api.queueMirror(current: song, upcoming: upNext, title: host);
      notifyListeners();
      _prefetch();
    }
    if (_loading) return;
    final diff = hostNow() - this.position.value; // > 0: we're behind
    if (diff.abs() > const Duration(milliseconds: 700)) {
      await _player.seek(hostNow(), exact: true);
      await _nudgeRate(0);
    } else if (playing) {
      await _nudgeRate(diff.inMilliseconds);
    }
    if (playing != this.playing) await (playing ? _playHere() : _pauseHere());
  }

  /// The host's upcoming songs last mirrored here (their ids).
  String _mirrored = '';

  double _nudge = 1;

  /// Guest: shift where we play relative to the host (ms; + plays ahead), to
  /// make up for Bluetooth or speaker delay that differs between devices.
  int syncOffsetMs = 0;

  /// Leaving a party: back to your own speed.
  Future<void> endFollow() {
    _mirrored = '';
    return _nudgeRate(0);
  }

  /// [behindMs] > 0: speed up a little; < 0: slow down; ~0: normal speed.
  /// The correction is sized to close the gap within about two seconds (the
  /// time to the host's next update), at most 5 %.
  Future<void> _nudgeRate(int behindMs) async {
    final want = behindMs.abs() < 60 ? 1.0 : (1 + (behindMs / 2000).clamp(-0.05, 0.05));
    if ((want - _nudge).abs() < 0.002) return;
    _nudge = want;
    try {
      await _player.setRate(settings.speed * want);
    } catch (_) {}
  }

  Future<void> togglePlay() async {
    if (steer != null) return steer!(playing ? 'pause' : 'play', position.value.inMilliseconds);
    // While a song loads, the button decides whether it starts.
    return setPlaying(_loading ? _userPaused : !playing);
  }

  /// Play or pause, from the app, the notification or headphones. What you
  /// asked for is remembered, so a reload (a dropped stream, a correction)
  /// never starts music you paused.
  Future<void> setPlaying(bool want) async {
    if (steer != null) {
      if (want != playing) steer!(want ? 'play' : 'pause', position.value.inMilliseconds);
      return;
    }
    _userPaused = !want;
    if (!want) _resumeAfterInterruption = false; // paused during a call: stay paused after it
    final s = current;
    if (s == null) return;
    if (_loading) {
      notifyListeners();
      return; // the song being loaded starts, or not, as asked
    }
    if (!want) return _player.pause();
    // After a restart the queue is back but nothing is loaded yet; and if the
    // engine stopped on a song that never started, play loads it properly.
    final stalled = !playing && (_player.state.completed || duration.value == Duration.zero);
    if (_loadedId != s.id || stalled) {
      _pendingSkips = 0;
      await _load();
      return;
    }
    await _player.play();
  }

  /// Pauses this device only (headphones out, a call, the sleep timer, the
  /// party host pausing), remembered like your own pause.
  Future<void> _pauseHere({bool resumable = false}) {
    _userPaused = true;
    if (!resumable) _resumeAfterInterruption = false;
    return _player.pause();
  }

  Future<void> _playHere() {
    _userPaused = false;
    return _player.play();
  }

  Future<void> seekFraction(double f) => seekTo(Duration(milliseconds: (duration.value.inMilliseconds * f).round()));
  Future<void> seekTo(Duration d) async {
    if (steer != null) return steer!('seek', d.inMilliseconds);
    await _player.seek(d);
    onTransport?.call();
    // The notification extrapolates from the last position it was told about,
    // so tell it about seeks too or its bar drifts.
    position.value = d;
    _syncSession();
  }

  /// Skip. Several quick taps are folded into one jump to the last song.
  Future<void> next() async {
    if (steer != null) return steer!('next', 0);
    _pendingSkips++;
    return _drain();
  }

  Future<void> previous() async {
    if (steer != null) return steer!('previous', 0);
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
    // A party guest's queue is the host's: the song is played (or suggested) there.
    if (steer != null) {
      final list = switch (section) { 'user' => queue.user, 'context' => queue.context, _ => queue.autoplay };
      if (index < list.length) steerPlay?.call(list[index]);
      return;
    }
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
    final hadNext = before.user.isNotEmpty || before.context.isNotEmpty || before.autoplay.isNotEmpty || before.repeat != 'off';
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
  Future<void> _finishCurrent({required bool skipped, bool record = true, double? completion}) async {
    final s = queue.current;
    if (s == null || !record) return;
    completion ??= duration.value.inMilliseconds == 0 ? 0.0 : progress;
    try {
      await api.recordPlay(s: s, completion: completion, superliked: _currentSuperliked, source: _currentSource);
    } catch (_) {}
    if (skipped && completion < 0.5 && completion > 0) {
      skippedRecently = s;
      _skipChipTimer?.cancel();
    _syncTimer?.cancel();
    _prefetchTimer?.cancel();
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
      final he = herr.classify('$e');
      return "Couldn't download · ${he.code}: ${he.message}";
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

  /// A party guest's queue is the host's: queueing a song asks the host to
  /// play it next (set by the party).
  void Function(api.SongView s)? steerQueue;

  /// A party guest: the queue here mirrors the host's and isn't edited here.
  bool get guest => steer != null;

  Future<void> playNext(api.SongView s) async {
    if (steerQueue != null) return steerQueue!(s);
    queue = await api.queuePlayNext(s: s);
    notifyListeners();
    _prefetch();
  }

  Future<void> addToQueue(api.SongView s) async {
    if (steerQueue != null) return steerQueue!(s);
    queue = await api.queueAdd(s: s);
    notifyListeners();
    _prefetch();
  }

  /// [section]: "user", "context" or "auto".
  Future<void> removeFromQueue(String section, int i) async {
    if (guest) return;
    queue = await api.queueRemove(section: section, index: i);
    notifyListeners();
  }

  /// Plays an upcoming song next; the others keep their order.
  Future<void> promoteInQueue(String section, int i) async {
    if (guest) return;
    queue = await api.queuePromote(section: section, index: i);
    notifyListeners();
    _prefetch();
  }

  Future<void> moveInQueue(String section, int from, int to) async {
    if (guest) return;
    queue = await api.queueMove(section: section, from: from, to: to);
    notifyListeners();
    _prefetch();
  }

  Future<void> shuffleUpcoming() async {
    if (guest) return;
    queue = await api.queueShuffle();
    notifyListeners();
  }

  Future<void> cycleRepeat() async {
    if (guest) return;
    final next = switch (queue.repeat) { 'off' => 'all', 'all' => 'one', _ => 'off' };
    queue = await api.queueSetRepeat(mode: next);
    notifyListeners();
  }

  void _onPlayerError(mak.PlayerError e) {
    // The engine also reports mpv's log errors here; only a failed song counts.
    if (e is! mak.EndFileError || e.reason != mak.MpvEndFileReason.error) return;
    final message = e.message;
    if (_loading) return;
    final s = current;
    if (s == null) return;
    // An expired stream URL: resolve again once.
    // Where it was and whether it was playing stay as they were: a stream
    // that dropped while paused reloads paused (it used to start playing).
    if (!_retried) {
      _retried = true;
      final at = position.value;
      final play = !_userPaused;
      api.forgetStream(id: s.id).then((_) => _load(play: play, at: at > const Duration(seconds: 2) ? at : null));
      return;
    }
    error = "Couldn't play “${s.title}” · ${herr.playbackFailed.code}: ${herr.playbackFailed.message}";
    errorDetail = '${herr.playbackFailed.code}\n\n${herr.neutral(message)}';
    notifyListeners();
  }

  @override
  void dispose() {
    for (final s in [..._subs, ..._playerSubs]) {
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
    palette.dispose();
    playlisted.dispose();
    libraryRev.dispose();
    downloading.dispose();
    accent.dispose();
    super.dispose();
  }
}
