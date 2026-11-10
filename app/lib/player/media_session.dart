import 'package:audio_service/audio_service.dart';

/// What the operating system sees: a media session with a title, artwork and
/// transport controls. It gives Android its notification, lock-screen and
/// Bluetooth/headset controls and keeps playback alive with the screen off;
/// on Linux it appears over MPRIS (GNOME/KDE media widgets, media keys), and on
/// Windows in the system media overlay.
///
/// It only forwards: the real work stays in `PlayerController`.
class HarpAudioHandler extends BaseAudioHandler with SeekHandler {
  Future<void> Function()? onPlay;
  Future<void> Function()? onPause;
  Future<void> Function()? onNext;
  Future<void> Function()? onPrevious;
  Future<void> Function(Duration)? onSeek;
  Future<void> Function()? onStop;

  /// Android Auto / media browsers: the library as folders, and playing from it.
  Future<List<MediaItem>> Function(String parentId)? onBrowse;
  Future<void> Function(String mediaId)? onPlayFromId;

  @override
  Future<List<MediaItem>> getChildren(String parentMediaId, [Map<String, dynamic>? options]) async =>
      await onBrowse?.call(parentMediaId) ?? const [];

  @override
  Future<void> playFromMediaId(String mediaId, [Map<String, dynamic>? extras]) async => onPlayFromId?.call(mediaId);

  @override
  Future<void> play() async => onPlay?.call();
  @override
  Future<void> pause() async => onPause?.call();
  @override
  Future<void> skipToNext() async => onNext?.call();
  @override
  Future<void> skipToPrevious() async => onPrevious?.call();
  @override
  Future<void> seek(Duration position) async => onSeek?.call(position);
  @override
  Future<void> stop() async {
    await onStop?.call();
    await super.stop();
  }

  void setNowPlaying({required String id, required String title, required String artist, String? album, Duration? duration, Uri? art}) {
    mediaItem.add(MediaItem(id: id, title: title, artist: artist, album: album, duration: duration, artUri: art));
  }

  /// Fills in the length once the player knows it (only for the song still showing).
  void setDuration(String id, Duration d) {
    final m = mediaItem.value;
    if (m == null || m.id != id) return;
    if (m.duration != null && (m.duration! - d).abs() < const Duration(seconds: 1)) return;
    mediaItem.add(m.copyWith(duration: d));
  }

  void setState({required bool playing, required bool loading, required Duration position, bool hasNext = true, bool hasPrevious = true, double speed = 1}) {
    playbackState.add(PlaybackState(
      controls: [
        if (hasPrevious) MediaControl.skipToPrevious,
        playing ? MediaControl.pause : MediaControl.play,
        if (hasNext) MediaControl.skipToNext,
      ],
      systemActions: const {MediaAction.seek, MediaAction.seekForward, MediaAction.seekBackward},
      androidCompactActionIndices: const [0, 1, 2],
      processingState: loading ? AudioProcessingState.loading : AudioProcessingState.ready,
      playing: playing,
      updatePosition: position,
      speed: speed,
    ));
  }

  void setIdle() {
    playbackState.add(PlaybackState(processingState: AudioProcessingState.idle, playing: false));
  }
}
