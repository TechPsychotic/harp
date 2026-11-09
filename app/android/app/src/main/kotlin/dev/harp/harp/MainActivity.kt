package dev.harp.harp

import android.Manifest
import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.Context
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.PowerManager
import com.ryanheise.audioservice.AudioServiceActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.util.Date

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

    /** What a crash left behind: our own stack trace, and Android's record of why the app last exited. */
    private fun crashReport(): String {
        val sb = StringBuilder()
        val f = File(filesDir, "last_crash.txt")
        if (f.exists()) sb.append("== Crash trace ==\n").append(f.readText().take(6000)).append("\n\n")
        if (Build.VERSION.SDK_INT >= 30) {
            try {
                val am = getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
                val names = mapOf(
                    ApplicationExitInfo.REASON_CRASH to "CRASH (Java exception)",
                    ApplicationExitInfo.REASON_CRASH_NATIVE to "CRASH_NATIVE (native code)",
                    ApplicationExitInfo.REASON_ANR to "ANR (not responding)",
                    ApplicationExitInfo.REASON_LOW_MEMORY to "LOW_MEMORY",
                    ApplicationExitInfo.REASON_SIGNALED to "SIGNALED",
                    ApplicationExitInfo.REASON_EXIT_SELF to "EXIT_SELF",
                    ApplicationExitInfo.REASON_USER_REQUESTED to "USER_REQUESTED",
                    ApplicationExitInfo.REASON_INITIALIZATION_FAILURE to "INITIALIZATION_FAILURE",
                    ApplicationExitInfo.REASON_PERMISSION_CHANGE to "PERMISSION_CHANGE",
                    ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE to "EXCESSIVE_RESOURCE_USAGE",
                    ApplicationExitInfo.REASON_OTHER to "OTHER",
                )
                val exits = am.getHistoricalProcessExitReasons(packageName, 0, 4)
                if (exits.isNotEmpty()) sb.append("== Recent exits (Android) ==\n")
                for (e in exits) {
                    sb.append("${Date(e.timestamp)}  ${names[e.reason] ?: "reason ${e.reason}"}  ${e.description ?: ""}\n")
                }
            } catch (_: Exception) {
            }
        }
        return sb.toString()
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
                    "crashReport" -> result.success(crashReport())
                    // The system share sheet (WhatsApp, Messages, copy…).
                    "share" -> {
                        val send = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
                            type = "text/plain"
                            putExtra(android.content.Intent.EXTRA_TEXT, call.argument<String>("text"))
                        }
                        startActivity(android.content.Intent.createChooser(send, null))
                        result.success(true)
                    }
                    "isMetered" -> {
                        val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as android.net.ConnectivityManager
                        result.success(cm.isActiveNetworkMetered)
                    }
                    "clearCrash" -> {
                        File(filesDir, "last_crash.txt").delete()
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
