import 'dart:io';

import 'package:archive/archive_io.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import 'src/rust/api/engine.dart' as api;

const _native = MethodChannel('dev.harp/native');

/// Starts the Rust engine with the right paths for this platform.
///
/// Desktop: the engine downloads and verifies yt-dlp itself.
/// Android: the app ships a static Python and QuickJS as native libraries
/// (see scripts/android-tools.sh); Python's standard library is an asset that
/// is unpacked once per app version.
Future<void> bootEngine() async {
  final support = await getApplicationSupportDirectory();
  final data = '${support.path}/harp';
  if (Platform.isAndroid) {
    final lib = await _native.invokeMethod<String>('nativeLibraryDir') ?? '';
    final version = await _native.invokeMethod<String>('versionStamp') ?? '0';
    final home = Directory('$data/python');
    final python = File('$lib/libpython.so');
    final hasPython = await python.exists();
    if (hasPython) await _unpackPythonLib(home, version);
    await api.startEngine(
      cfg: api.EngineConfig(
        dataDir: data,
        python: hasPython ? python.path : null,
        pythonHome: hasPython ? home.path : null,
        quickjs: await File('$lib/libqjs.so').exists() ? '$lib/libqjs.so' : null,
        caDir: '/system/etc/security/cacerts',
      ),
    );
  } else {
    await api.startEngine(cfg: api.EngineConfig(dataDir: data));
  }
}

Future<void> _unpackPythonLib(Directory home, String version) async {
  final marker = File('${home.path}/version');
  if (await marker.exists() && (await marker.readAsString()).trim() == version) return;
  final bytes = (await rootBundle.load('assets/python.zip')).buffer.asUint8List();
  final lib = Directory('${home.path}/lib/python3.13');
  if (await lib.exists()) await lib.delete(recursive: true);
  await lib.create(recursive: true);
  final archive = ZipDecoder().decodeBytes(bytes);
  for (final f in archive) {
    final out = '${lib.path}/${f.name}';
    if (!File(out).absolute.path.startsWith(lib.absolute.path)) continue; // zip-slip guard
    if (f.isFile) {
      await File(out).create(recursive: true);
      await File(out).writeAsBytes(f.content as List<int>);
    } else {
      await Directory(out).create(recursive: true);
    }
  }
  await marker.writeAsString(version);
}
