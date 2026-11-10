import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:harp/update.dart';

void main() {
  test('versions compare by number, with or without a v', () {
    expect(Updates.newer('v0.9.1', '0.9.0'), isTrue);
    expect(Updates.newer('v0.10.0', '0.9.9'), isTrue, reason: 'numbers, not text');
    expect(Updates.newer('v0.9.0', '0.9.0'), isFalse);
    expect(Updates.newer('v0.8.5', '0.9.0'), isFalse);
    expect(Updates.newer('1.0.0+12', '0.9.0'), isTrue);
  });

  test('checksums are read from SHA256SUMS', () {
    const a = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
    const b = 'BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB';
    const sums = '$a  harp-linux-x64.tar.gz\n$b *harp-android-arm64.apk\n';
    expect(Updates.sumFor(sums, 'harp-linux-x64.tar.gz'), a);
    expect(Updates.sumFor(sums, 'harp-android-arm64.apk'), b.toLowerCase());
    expect(Updates.sumFor(sums, 'harp-macos.zip'), isNull);
  });
  test('releases survive being saved', () {
    const r = Release(tag: 'v1.0.0', url: 'u', assets: {'SHA256SUMS.txt': 's'});
    final back = Release.fromJson(jsonEncode(r.toJson()))!;
    expect(back.tag, 'v1.0.0');
    expect(back.assets['SHA256SUMS.txt'], 's');
    expect(Release.fromJson('nonsense'), isNull);
  });
}
