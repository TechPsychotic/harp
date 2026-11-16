import 'dart:async';

import 'package:flutter/services.dart';

import '../src/rust/api/spatial.dart' as spatial;

/// 3D audio's head tracking on Android: the Kotlin side (AirPodsLink) holds the AirPods'
/// control channel, every packet the AirPods send goes to the Rust head tracker, which steers the
/// 3D engine. AirPods talk to one app at a time: Lyre lets go of them while in the background.
class AndroidHead {
  static const _m = MethodChannel('dev.harp/airpods');
  static const _events = EventChannel('dev.harp/airpods/events');

  StreamSubscription? _sub;
  bool _want = false;
  bool _open = false;
  Timer? _retry;
  Timer? _watch;
  int _backoff = 5;

  void _link(String state, [String detail = '']) => spatial.spatialHeadLink(state: state, detail: detail);

  /// Head tracking on or off (3D on, with head tracking, on Android).
  Future<void> sync(bool want) async {
    if (want == _want) return;
    _want = want;
    if (want) {
      _backoff = 5;
      await _start();
    } else {
      await _stop();
    }
  }

  Future<void> _start() async {
    _retry?.cancel();
    if (!_want || _open) return;
    _link('searching', 'Looking for connected AirPods');
    try {
      final allowed = await _m.invokeMethod<bool>('permission') ?? false;
      if (!allowed) {
        _link('error', 'Harp needs the Nearby devices permission for head tracking');
        return;
      }
      _sub ??= _events.receiveBroadcastStream().listen(_onEvent);
      final name = await _m.invokeMethod<String>('open') ?? 'AirPods';
      _open = true;
      _link('connected', name);
      await _sendStart(all: true);
      _watchMotion();
    } on PlatformException catch (e) {
      _open = false;
      final why = e.code == 'no_airpods' ? 'No AirPods connected' : '${e.message ?? 'Couldn\'t connect'}${e.details != null ? '\n${e.details}' : ''}';
      _link(e.code == 'no_airpods' ? 'searching' : 'error', why);
      _scheduleRetry();
    } catch (e) {
      _open = false;
      _link('error', '$e');
      _scheduleRetry();
    }
  }

  Future<void> _sendStart({required bool all}) async {
    final packets = spatial.spatialHeadStartPackets();
    // Without `all`: only the motion-stream requests (the last three), asked again.
    for (final p in all ? packets : packets.skip(packets.length - 3)) {
      await _m.invokeMethod('send', {'data': Uint8List.fromList(p.data)});
      if (p.waitMs > 0) await Future<void>.delayed(Duration(milliseconds: p.waitMs));
    }
  }

  bool get _moving => spatial.spatialStatus().head == 'tracking' || spatial.spatialStatus().head == 'paused';

  /// The AirPods sometimes don't start their motion stream (after many sessions in a row, or
  /// while another app holds them): ask again after 6 s; after 15 s give up for a while and
  /// say what to do.
  void _watchMotion() {
    _watch?.cancel();
    var asked = 1;
    _watch = Timer.periodic(const Duration(seconds: 6), (t) async {
      if (!_want || !_open) return t.cancel();
      if (_moving) {
        _backoff = 5;
        return;
      }
      if (asked < 3) {
        asked++;
        try {
          await _sendStart(all: asked == 3);
        } catch (_) {}
        return;
      }
      t.cancel();
      final why = spatial.spatialStatus().headDetail;
      await _closeLink();
      _link('error', "The AirPods aren't sending motion data ($why). Close Lyre (swipe it away), or turn Bluetooth off and on, or put the AirPods in the case for 10 s.");
      _scheduleRetry();
    });
  }

  Future<void> _closeLink() async {
    if (_open) {
      try {
        for (final p in spatial.spatialHeadStopPackets()) {
          await _m.invokeMethod('send', {'data': Uint8List.fromList(p)});
        }
        await _m.invokeMethod('close');
      } catch (_) {}
    }
    _open = false;
  }

  /// Every attempt is a new session with the buds: 5 s, then 15, 30, then every minute.
  void _scheduleRetry() {
    if (!_want) return;
    final wait = _backoff;
    _backoff = wait < 15 ? 15 : (wait < 30 ? 30 : 60);
    _retry = Timer(Duration(seconds: wait), _start);
  }

  void _onEvent(dynamic e) {
    final m = (e as Map).cast<String, Object?>();
    switch (m['type']) {
      case 'packet':
        spatial.spatialHeadFeed(data: m['data'] as Uint8List);
      case 'closed':
        _watch?.cancel();
        _open = false;
        _link('searching', (m['reason'] as String?) ?? 'The AirPods disconnected');
        _scheduleRetry();
    }
  }

  Future<void> _stop() async {
    _retry?.cancel();
    _watch?.cancel();
    await _closeLink();
    _link('off');
  }
}
