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
}
