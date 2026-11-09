package dev.harp.harp

import android.app.Application
import android.util.Log
import java.io.File
import java.util.Date

/**
 * Writes any crash (in the app or its media service) to a file before the process
 * dies, so Settings can show what happened. No network, nothing leaves the phone.
 */
class HarpApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            try {
                File(filesDir, "last_crash.txt").writeText(
                    "${Date()}\nthread: ${thread.name}\n${Log.getStackTraceString(error)}"
                )
            } catch (_: Throwable) {
            }
            previous?.uncaughtException(thread, error)
        }
    }
}
