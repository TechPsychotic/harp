import 'dart:async';

import 'package:mpv_audio_kit/mpv_audio_kit.dart' as mak;

import '../src/rust/api/engine.dart' as api;
import 'stream_headers.dart';

/// Roadmap M2, step 1: a self-check of the candidate audio engine
/// (`mpv_audio_kit`) on this device. It runs only when you start it
/// (Settings › About › Test the new audio engine), plays a song through the
/// new engine with each effect we want, and reports what worked. Nothing in
/// normal playback uses it yet.
class EngineProbe {
  /// Runs the check on song [id] (streamed or local). [log] gets each line as
  /// it happens; the full report is returned.
  static Future<String> run(String id, {required String quality, void Function(String)? log}) async {
    final out = StringBuffer();
    void say(String s) {
      out.writeln(s);
      log?.call(s);
    }

    final started = DateTime.now();
    mak.Player? player;
    final subs = <StreamSubscription<dynamic>>[];
    try {
      // 1. Load the library.
      try {
        mak.MpvAudioKit.ensureInitialized();
        say('✓ Engine library loaded');
      } catch (e) {
        say('✗ Engine library failed to load: $e');
        return out.toString();
      }
      player = mak.Player();
      final p = player;
      say('  mpv: ${await p.getRawProperty('mpv-version') ?? '?'}');
      say('  FFmpeg: ${await p.getRawProperty('ffmpeg-version') ?? '?'}');

      // 2. Play a song through it.
      var pos = Duration.zero;
      var fftFrames = 0;
      var loudest = 0.0;
      final errors = <String>[];
      subs.add(p.stream.position.listen((d) => pos = d));
      subs.add(p.stream.error.listen((e) => errors.add(e.message)));
      subs.add(p.stream.fft.listen((f) {
        fftFrames++;
        for (final b in f.bands) {
          if (b > loudest) loudest = b;
        }
      }));
      final st = await api.resolveStream(id: id, quality: quality);
      final h = engineHeaders(st.headers, local: st.local);
      if (h.userAgent != null) await p.setRawProperty('user-agent', h.userAgent!);
      await p.open(mak.Media(st.url, httpHeaders: h.headers));
      await p.play();
      final t0 = DateTime.now();
      while (pos < const Duration(milliseconds: 1500) && DateTime.now().difference(t0) < const Duration(seconds: 20)) {
        await Future<void>.delayed(const Duration(milliseconds: 250));
      }
      if (pos < const Duration(milliseconds: 1500)) {
        say('✗ Playback did not start in 20 s${errors.isEmpty ? '' : ': ${errors.last}'}');
        return out.toString();
      }
      say('✓ Plays (started in ${DateTime.now().difference(t0).inMilliseconds} ms)');

      // 3. Each effect on its own: it must apply, and playback must keep going.
      final tests = <(String, List<mak.AudioEffect>)>[
        ('Equalizer (bass boost)', [const mak.EqualizerEffect(frequency: 60, gain: 6, width: 1, widthType: mak.EqualizerWidthType.q)]),
        ('Spatial: stereo widen', [const mak.StereowidenEffect()]),
        ('Spatial: headphone crossfeed', [const mak.CrossfeedEffect()]),
        ('Even volume (dynaudnorm)', [const mak.DynaudnormEffect()]),
        ('Even volume (loudnorm)', [const mak.LoudnormEffect(i: -16, tp: -1.5, lra: 11)]),
        ('All together', [
          const mak.DynaudnormEffect(),
          const mak.EqualizerEffect(frequency: 60, gain: 6, width: 1, widthType: mak.EqualizerWidthType.q),
          const mak.StereowidenEffect(),
        ]),
        // True 3D needs FFmpeg's sofalizer. With no data file it can't start either
        // way; the error says whether the filter itself exists.
        ('3D (sofalizer present?)', [const mak.RawAudioEffect('lavfi-sofalizer=sofa=/nonexistent.sofa')]),
      ];
      for (final (name, fx) in tests) {
        errors.clear();
        final before = pos;
        String? thrown;
        try {
          await p.setAudioEffects(fx).timeout(const Duration(seconds: 5));
        } catch (e) {
          thrown = '$e';
        }
        await Future<void>.delayed(const Duration(milliseconds: 2500));
        final moved = pos - before;
        final ok = thrown == null && moved > const Duration(milliseconds: 1500);
        final why = thrown ?? (errors.isNotEmpty ? errors.first : (ok ? '' : 'playback stalled'));
        say('${ok ? '✓' : '✗'} $name${why.isEmpty ? '' : ': ${why.length > 140 ? '${why.substring(0, 140)}…' : why}'}');
        if (!ok) {
          // Clear and make sure we're still playing before the next test.
          try {
            await p.setAudioEffects(const []).timeout(const Duration(seconds: 5));
          } catch (_) {}
          await Future<void>.delayed(const Duration(milliseconds: 1200));
        }
      }
      await p.setAudioEffects(const []);

      // 4. The live spectrum (what the background would follow).
      say(fftFrames > 0 && loudest > 0.05
          ? '✓ Live spectrum: $fftFrames frames, real levels'
          : '✗ Live spectrum: $fftFrames frames, loudest band ${loudest.toStringAsFixed(2)}');
    } catch (e) {
      say('✗ Stopped: $e');
    } finally {
      for (final s in subs) {
        await s.cancel();
      }
      try {
        await player?.dispose();
      } catch (_) {}
      say('Took ${DateTime.now().difference(started).inSeconds} s');
    }
    return out.toString();
  }
}
