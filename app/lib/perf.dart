import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

/// A small on-screen readout of how smooth the app is (Settings › Performance
/// overlay). It reports the average time to produce a frame and the share of
/// frames that missed a 60 Hz deadline, over the last second or so.
class FrameMeter extends StatefulWidget {
  const FrameMeter({super.key});

  @override
  State<FrameMeter> createState() => _FrameMeterState();
}

class _FrameMeterState extends State<FrameMeter> {
  final List<FrameTiming> _recent = [];
  String _label = 'measuring…';

  void _onTimings(List<FrameTiming> t) {
    _recent.addAll(t);
    if (_recent.length < 30) return;
    final ms = _recent.map((f) => f.totalSpan.inMicroseconds / 1000).toList();
    final avg = ms.reduce((a, b) => a + b) / ms.length;
    final worst = ms.reduce((a, b) => a > b ? a : b);
    final janky = ms.where((m) => m > 16.7).length * 100 ~/ ms.length;
    _recent.clear();
    if (mounted) setState(() => _label = 'avg ${avg.toStringAsFixed(1)} ms · worst ${worst.toStringAsFixed(0)} ms · $janky% slow');
  }

  @override
  void initState() {
    super.initState();
    SchedulerBinding.instance.addTimingsCallback(_onTimings);
  }

  @override
  void dispose() {
    SchedulerBinding.instance.removeTimingsCallback(_onTimings);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return IgnorePointer(
      child: DecoratedBox(
        decoration: BoxDecoration(color: Colors.black.withValues(alpha: 0.7), borderRadius: BorderRadius.circular(8)),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
          child: Text(_label, style: const TextStyle(color: Colors.white, fontSize: 11, fontFeatures: [FontFeature.tabularFigures()])),
        ),
      ),
    );
  }
}
