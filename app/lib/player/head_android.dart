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
      for (final p in spatial.spatialHeadStartPackets()) {
        await _m.invokeMethod('send', {'data': Uint8List.fromList(p.data)});
        if (p.waitMs > 0) await Future<void>.delayed(Duration(milliseconds: p.waitMs));
      }
      _backoff = 5;
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
        _open = false;
        _link('searching', (m['reason'] as String?) ?? 'The AirPods disconnected');
        _scheduleRetry();
    }
  }

  Future<void> _stop() async {
    _retry?.cancel();
    if (_open) {
      try {
        for (final p in spatial.spatialHeadStopPackets()) {
          await _m.invokeMethod('send', {'data': Uint8List.fromList(p)});
        }
        await _m.invokeMethod('close');
      } catch (_) {}
    }
    _open = false;
    _link('off');
  }
}
