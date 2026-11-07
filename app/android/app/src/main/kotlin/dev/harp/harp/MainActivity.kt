package dev.harp.harp

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.harp/native")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    // Android runs programs only from the app's library folder,
                    // so Python and QuickJS ship as libpython.so / libqjs.so there.
                    "nativeLibraryDir" -> result.success(applicationInfo.nativeLibraryDir)
                    // Changes with every install, so Python's unpacked library is refreshed.
                    "versionStamp" -> result.success(
                        packageManager.getPackageInfo(packageName, 0).lastUpdateTime.toString()
                    )
                    else -> result.notImplemented()
                }
            }
    }
}
