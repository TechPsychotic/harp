import 'package:flutter_test/flutter_test.dart';
import 'package:harp/errors.dart';

void main() {
  test('raw messages map to Harp codes', () {
    expect(classify("ERROR: [youtube] abc: Sign in to confirm you’re not a bot").code, 'H-101');
    expect(classify('HTTP Error 429: Too Many Requests').code, 'H-101');
    expect(classify('Sign in to confirm your age').code, 'H-108', reason: 'age check is not a rate limit');
    expect(classify('ERROR: [youtube] a429b: Video unavailable').code, 'H-103', reason: 'a 429 inside an id is not a rate limit');
    expect(classify('<urlopen error Tunnel connection failed: 502 Bad Gateway>').code, 'H-102');
    expect(classify('Offline mode: this song is not downloaded.').code, 'H-104');
    expect(classify('this file is no longer on the device').code, 'H-106');
    expect(classify('The read operation timed out').code, 'H-107');
    expect(classify('something odd').code, 'H-199');
  });
  test('details name no service', () {
    final n = neutral('ERROR: [youtube] x: see https://www.youtube.com/watch?v=x on YouTube Music (rr1.googlevideo.com)');
    expect(n.toLowerCase().contains('youtube'), isFalse);
    expect(n.contains('googlevideo'), isFalse);
  });
}
