import 'package:flutter_test/flutter_test.dart';
import 'package:harp/screens/library.dart' show sortSongs;
import 'package:harp/screens/now_playing.dart' show speedLabel;
import 'package:harp/src/rust/api/engine.dart' as api;

api.SongView song(String id, String title, String artist) =>
    api.SongView(id: id, title: title, artist: artist, thumbnail: '', liked: false, downloaded: false);

void main() {
  test('songs sort by title or artist, recent keeps order', () {
    final l = [song('1', 'zebra', 'B'), song('2', 'Apple', 'c'), song('3', 'mango', 'a')];
    expect(sortSongs(l, 'recent').map((s) => s.id), ['1', '2', '3']);
    expect(sortSongs(l, 'title').map((s) => s.id), ['2', '3', '1']);
    expect(sortSongs(l, 'artist').map((s) => s.id), ['3', '1', '2']);
    expect(l.first.id, '1', reason: 'the original list is untouched');
  });

  test('speed labels', () {
    expect(speedLabel(1), '1×');
    expect(speedLabel(1.5), '1.5×');
    expect(speedLabel(1.25), '1.25×');
    expect(speedLabel(2), '2×');
  });
}
