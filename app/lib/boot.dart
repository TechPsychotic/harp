import 'dart:io';

import 'package:archive/archive_io.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import 'src/rust/api/engine.dart' as api;
import 'widgets/cover.dart';

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
  await ThumbCache.init(data);
  if (Platform.isAndroid) {
    final lib = await _native.invokeMethod<String>('nativeLibraryDir') ?? '';
    final version = await _native.invokeMethod<String>('versionStamp') ?? '0';
    final home = Directory('$data/python');
    final python = File('$lib/libpython.so');
    final hasPython = await python.exists();
    if (hasPython) await _unpackPythonLib(home, version);
    final bundled = await _unpackBundledYtdlp('$data/tools', version);
    await api.startEngine(
      cfg: api.EngineConfig(
        dataDir: data,
        python: hasPython ? python.path : null,
        pythonHome: hasPython ? home.path : null,
        quickjs: await File('$lib/libqjs.so').exists() ? '$lib/libqjs.so' : null,
        caDir: '/system/etc/security/cacerts',
        bundledYtdlp: bundled,
      ),
    );
  } else {
    await api.startEngine(cfg: api.EngineConfig(dataDir: data));
  }
}

/// The yt-dlp shipped inside the app, unpacked once per install. Used only if
/// downloading the current one fails (and there is no earlier copy).
Future<String?> _unpackBundledYtdlp(String tools, String version) async {
  final f = File('$tools/yt-dlp.bundled');
  final stamp = File('$tools/yt-dlp.bundled.version');
  try {
    if (await f.exists() && await stamp.exists() && (await stamp.readAsString()).trim() == version) return f.path;
    final bytes = (await rootBundle.load('assets/yt-dlp.pyz')).buffer.asUint8List();
    await f.parent.create(recursive: true);
    await f.writeAsBytes(bytes, flush: true);
    await stamp.writeAsString(version);
    return f.path;
  } catch (_) {
    return null; // not bundled (a desktop build, or a build without the tools script)
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
