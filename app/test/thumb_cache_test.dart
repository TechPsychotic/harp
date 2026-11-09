import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:harp/widgets/cover.dart';

void main() {
  test('cover cache file names match the Rust side (SHA-256 of the URL, first 24 hex chars)', () async {
    final dir = Directory.systemTemp.createTempSync('harp-thumbs-test');
    await ThumbCache.init(dir.path);
    final f = File('${dir.path}/thumbs/804075528fff04a2c680f8a8.img');
    f.writeAsBytesSync([1, 2, 3]);
    expect(ThumbCache.peek('https://x/a.jpg'), f.path);
    expect(ThumbCache.peek('https://x/other.jpg'), isNull);
    // Already cached: no network is touched.
    expect(await ThumbCache.path('https://x/a.jpg'), f.path);
    // Not a web address: nothing to fetch.
    expect(await ThumbCache.path('file:///etc/passwd'), '');
    ThumbCache.forgetAll();
    expect(ThumbCache.peek('https://x/a.jpg'), f.path, reason: 'found again on disk');
    dir.deleteSync(recursive: true);
  });
}
