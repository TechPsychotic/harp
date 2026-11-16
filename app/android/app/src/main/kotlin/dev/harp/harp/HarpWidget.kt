package dev.harp.harp

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.view.KeyEvent
import android.widget.RemoteViews

/**
 * Harp's home-screen widgets, all showing what's playing:
 *  - [HarpWidget]: cover, song and previous / play-pause / next in one row (4×1).
 *  - [HarpMiniWidget]: song with play-pause and next (2×1).
 *  - [HarpCoverWidget]: the cover with a play-pause button on it (2×2).
 *  - [HarpLargeWidget]: big cover, song, artist and the three buttons (4×2 and up).
 * The app tells them what's playing (see MainActivity "widget"); the buttons send
 * media-button presses to Harp's media service, like headphone buttons, so they work
 * without the app's screen.
 */
class HarpWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        manager.updateAppWidget(ids, views(context, R.layout.harp_widget))
    }

    companion object {
        private const val PREFS = "harp_widget"

        /** The app calls this when the song or play state changes. */
        fun update(context: Context, title: String, artist: String, art: String, playing: Boolean) {
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                .putString("title", title).putString("artist", artist).putString("art", art)
                .putBoolean("playing", playing).apply()
            val manager = AppWidgetManager.getInstance(context)
            for ((cls, layout) in kinds) {
                val ids = manager.getAppWidgetIds(ComponentName(context, cls))
                if (ids.isNotEmpty()) manager.updateAppWidget(ids, views(context, layout))
            }
        }

        private val kinds = listOf(
            HarpWidget::class.java to R.layout.harp_widget,
            HarpMiniWidget::class.java to R.layout.harp_widget_mini,
            HarpCoverWidget::class.java to R.layout.harp_widget_cover,
            HarpLargeWidget::class.java to R.layout.harp_widget_large,
        )

        private fun mediaKey(context: Context, code: Int, request: Int): PendingIntent {
            val intent = Intent(Intent.ACTION_MEDIA_BUTTON).apply {
                component = ComponentName(context, "com.ryanheise.audioservice.MediaButtonReceiver")
                putExtra(Intent.EXTRA_KEY_EVENT, KeyEvent(KeyEvent.ACTION_DOWN, code))
            }
            return PendingIntent.getBroadcast(context, request, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }

        /** One widget's views; layouts without a view simply don't have that id. */
        fun views(context: Context, layout: Int): RemoteViews {
            val p = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val views = RemoteViews(context.packageName, layout)
            val title = p.getString("title", null)
            views.setTextViewText(R.id.widget_title, title ?: "Harp")
            views.setTextViewText(R.id.widget_artist, if (title == null) "Tap to open" else p.getString("artist", null) ?: "")
            views.setImageViewResource(
                R.id.widget_play,
                if (p.getBoolean("playing", false)) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
            )
            if (layout != R.layout.harp_widget_mini) {
                val art = p.getString("art", null)
                val big = layout == R.layout.harp_widget_cover || layout == R.layout.harp_widget_large
                val bitmap = try {
                    if (art.isNullOrEmpty()) null else BitmapFactory.decodeFile(art, BitmapFactory.Options().apply { inSampleSize = if (big) 1 else 2 })
                } catch (_: Exception) { null }
                if (bitmap != null) views.setImageViewBitmap(R.id.widget_art, bitmap) else views.setImageViewResource(R.id.widget_art, R.mipmap.ic_launcher)
            }
            if (layout != R.layout.harp_widget_mini && layout != R.layout.harp_widget_cover) {
                views.setOnClickPendingIntent(R.id.widget_prev, mediaKey(context, KeyEvent.KEYCODE_MEDIA_PREVIOUS, 1))
            }
            views.setOnClickPendingIntent(R.id.widget_play, mediaKey(context, KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, 2))
            if (layout != R.layout.harp_widget_cover) {
                views.setOnClickPendingIntent(R.id.widget_next, mediaKey(context, KeyEvent.KEYCODE_MEDIA_NEXT, 3))
            }
            val open = PendingIntent.getActivity(
                context, 0, Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                PendingIntent.FLAG_IMMUTABLE,
            )
            views.setOnClickPendingIntent(R.id.widget_root, open)
            return views
        }
    }
}

class HarpMiniWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        manager.updateAppWidget(ids, HarpWidget.views(context, R.layout.harp_widget_mini))
    }
}

class HarpCoverWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        manager.updateAppWidget(ids, HarpWidget.views(context, R.layout.harp_widget_cover))
    }
}

class HarpLargeWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        manager.updateAppWidget(ids, HarpWidget.views(context, R.layout.harp_widget_large))
    }
}
