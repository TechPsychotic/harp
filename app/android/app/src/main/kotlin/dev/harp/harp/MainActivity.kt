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

    private var pendingAudio: MethodChannel.Result? = null
    private var links: MethodChannel? = null

    /** A harp:// party or sync link in an intent: opened directly, or inside shared text. */
    private fun linkOf(i: android.content.Intent?): String? {
        if (i == null) return null
        i.dataString?.let { if (it.startsWith("harp://")) return it }
        if (i.action == android.content.Intent.ACTION_SEND) {
            val text = i.getStringExtra(android.content.Intent.EXTRA_TEXT) ?: return null
            return Regex("harp://(party|sync|playlist)/\\S+").find(text)?.value
        }
        return null
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        linkOf(intent)?.let { links?.invokeMethod("link", it) }
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 8) {
            pendingAudio?.success(grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED)
            pendingAudio = null
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

    private fun installApk(apk: File) {
        val installer = packageManager.packageInstaller
        val params = android.content.pm.PackageInstaller.SessionParams(android.content.pm.PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        params.setAppPackageName(packageName)
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
            apk.inputStream().use { input ->
                session.openWrite("harp.apk", 0, apk.length()).use { out ->
                    input.copyTo(out)
                    session.fsync(out)
                }
            }
            val flags = android.app.PendingIntent.FLAG_UPDATE_CURRENT or
                (if (Build.VERSION.SDK_INT >= 31) android.app.PendingIntent.FLAG_MUTABLE else 0)
            val pending = android.app.PendingIntent.getBroadcast(
                this, id, android.content.Intent(this, UpdateReceiver::class.java), flags,
            )
            session.commit(pending.intentSender)
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        links = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.harp/links").apply {
            setMethodCallHandler { call, result ->
                if (call.method == "initial") {
                    result.success(linkOf(intent))
                    // Only once: don't reopen the same link after a restart of the screen.
                    intent = android.content.Intent(intent).apply { data = null; removeExtra(android.content.Intent.EXTRA_TEXT) }
                } else {
                    result.notImplemented()
                }
            }
        }
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
                    // The home-screen widget shows what's playing.
                    "widget" -> {
                        HarpWidget.update(
                            applicationContext,
                            call.argument<String>("title") ?: "",
                            call.argument<String>("artist") ?: "",
                            call.argument<String>("art") ?: "",
                            call.argument<Boolean>("playing") ?: false,
                        )
                        result.success(null)
                    }
                    // Updates: hand a downloaded, checked APK to Android's installer.
                    // Returns "permission" when Harp first needs "Install unknown apps".
                    "installApk" -> {
                        if (Build.VERSION.SDK_INT >= 26 && !packageManager.canRequestPackageInstalls()) {
                            startActivity(
                                android.content.Intent(
                                    android.provider.Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                                    android.net.Uri.parse("package:$packageName"),
                                )
                            )
                            result.success("permission")
                        } else {
                            try {
                                installApk(File(call.argument<String>("path")!!))
                                result.success("started")
                            } catch (e: Exception) {
                                result.error("install", e.message, null)
                            }
                        }
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
                    // Reading your music files: asks once, answers whether Harp may.
                    "requestAudioAccess" -> {
                        val perm = if (Build.VERSION.SDK_INT >= 33) Manifest.permission.READ_MEDIA_AUDIO else Manifest.permission.READ_EXTERNAL_STORAGE
                        if (checkSelfPermission(perm) == PackageManager.PERMISSION_GRANTED) {
                            result.success(true)
                        } else {
                            pendingAudio?.success(false)
                            pendingAudio = result
                            requestPermissions(arrayOf(perm), 8)
                        }
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
