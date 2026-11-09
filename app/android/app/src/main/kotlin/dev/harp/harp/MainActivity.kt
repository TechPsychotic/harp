package dev.harp.harp

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.PowerManager
import com.ryanheise.audioservice.AudioServiceActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : AudioServiceActivity() {
    private var wake: PowerManager.WakeLock? = null
    private var wifi: WifiManager.WifiLock? = null

    /** Keeps the CPU and Wi-Fi awake while music plays, so a long screen-off does not stop it. */
    private fun setWake(on: Boolean) {
        try {
            if (on) {
                if (wake == null) {
                    val pm = applicationContext.getSystemService(Context.POWER_SERVICE) as PowerManager
                    wake = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "harp:playback").apply { setReferenceCounted(false) }
                }
                if (wake?.isHeld != true) wake?.acquire(6 * 60 * 60 * 1000L)
                if (wifi == null) {
                    val wm = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
                    val mode = if (Build.VERSION.SDK_INT >= 29) WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF
                    wifi = wm.createWifiLock(mode, "harp:playback").apply { setReferenceCounted(false) }
                }
                if (wifi?.isHeld != true) wifi?.acquire()
            } else {
                if (wake?.isHeld == true) wake?.release()
                if (wifi?.isHeld == true) wifi?.release()
            }
        } catch (_: Exception) {
        }
    }

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
                    "wakeLock" -> {
                        setWake(call.argument<Boolean>("on") ?: false)
                        result.success(null)
                    }
                    "requestNotifications" -> {
                        if (Build.VERSION.SDK_INT >= 33 &&
                            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
                        ) {
                            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 7)
                        }
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
    }

    override fun onDestroy() {
        setWake(false)
        super.onDestroy()
    }
}
