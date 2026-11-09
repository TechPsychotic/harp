import 'package:flutter_test/flutter_test.dart';
import 'package:harp/player/stream_headers.dart';

void main() {
  test('browser headers become ones the engine accepts', () {
    // What yt-dlp really sends.
    final raw = [
      ('User-Agent', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36'),
      ('Accept', 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8'),
      ('Accept-Language', 'en-us,en;q=0.5'),
      ('Sec-Fetch-Mode', 'navigate'),
    ];
    final h = engineHeaders(raw, local: false);
    expect(h.userAgent, contains('KHTML, like Gecko'), reason: 'kept whole, sent as mpv user-agent');
    expect(h.headers, {'Sec-Fetch-Mode': 'navigate'});
    for (final v in h.headers!.values) {
      expect(v.contains(','), isFalse);
    }
    expect(engineHeaders(raw, local: true), (headers: null, userAgent: null));
    expect(engineHeaders(const [], local: false).headers, isNull);
  });
}
