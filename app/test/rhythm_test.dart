import 'package:flutter_test/flutter_test.dart';
import 'package:harp/screens/rhythm.dart';

void main() {
  test('Encore opens from the second week of December into early January', () {
    expect(encoreSeason(DateTime(2026, 12, 7)), isFalse);
    expect(encoreSeason(DateTime(2026, 12, 8)), isTrue);
    expect(encoreSeason(DateTime(2027, 1, 7)), isTrue);
    expect(encoreSeason(DateTime(2027, 1, 8)), isFalse);
    expect(encoreSeason(DateTime(2026, 11, 10)), isFalse);
    expect(encoreYear(DateTime(2027, 1, 3)), 2026);
    expect(encoreYear(DateTime(2026, 12, 20)), 2026);
  });
  test('minutes and hours read well', () {
    expect(minutesText(999.4), '999');
    expect(minutesText(12345), '12,345');
    expect(minutesText(1234567), '1,234,567');
    expect(hourText(0), '12 am');
    expect(hourText(13), '1 pm');
  });
}
