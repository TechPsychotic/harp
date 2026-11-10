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
 * Harp's home-screen widget: the song, its cover, and previous / play-pause /
 * next. The app tells it what's playing (see MainActivity "widget"); the
 * buttons send media-button presses to Harp's media service, like headphone
 * buttons, so they work without the app's screen.
 */
class HarpWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        render(context, manager, ids)
    }

    companion object {
        private const val PREFS = "harp_widget"

        /** The app calls this when the song or play state changes. */
        fun update(context: Context, title: String, artist: String, art: String, playing: Boolean) {
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                .putString("title", title).putString("artist", artist).putString("art", art)
                .putBoolean("playing", playing).apply()
            val manager = AppWidgetManager.getInstance(context)
            val ids = manager.getAppWidgetIds(ComponentName(context, HarpWidget::class.java))
            if (ids.isNotEmpty()) render(context, manager, ids)
        }

        private fun mediaKey(context: Context, code: Int, request: Int): PendingIntent {
            val intent = Intent(Intent.ACTION_MEDIA_BUTTON).apply {
                component = ComponentName(context, "com.ryanheise.audioservice.MediaButtonReceiver")
                putExtra(Intent.EXTRA_KEY_EVENT, KeyEvent(KeyEvent.ACTION_DOWN, code))
            }
            return PendingIntent.getBroadcast(context, request, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }

        private fun render(context: Context, manager: AppWidgetManager, ids: IntArray) {
            val p = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val views = RemoteViews(context.packageName, R.layout.harp_widget)
            views.setTextViewText(R.id.widget_title, p.getString("title", null) ?: "Harp")
            views.setTextViewText(R.id.widget_artist, p.getString("artist", null) ?: "Tap to open")
            views.setImageViewResource(
                R.id.widget_play,
                if (p.getBoolean("playing", false)) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
            )
            val art = p.getString("art", null)
            val bitmap = try {
                if (art.isNullOrEmpty()) null else BitmapFactory.decodeFile(art, BitmapFactory.Options().apply { inSampleSize = 2 })
            } catch (_: Exception) { null }
            if (bitmap != null) views.setImageViewBitmap(R.id.widget_art, bitmap) else views.setImageViewResource(R.id.widget_art, R.mipmap.ic_launcher)
            views.setOnClickPendingIntent(R.id.widget_prev, mediaKey(context, KeyEvent.KEYCODE_MEDIA_PREVIOUS, 1))
            views.setOnClickPendingIntent(R.id.widget_play, mediaKey(context, KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, 2))
            views.setOnClickPendingIntent(R.id.widget_next, mediaKey(context, KeyEvent.KEYCODE_MEDIA_NEXT, 3))
            val open = PendingIntent.getActivity(
                context, 0, Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                PendingIntent.FLAG_IMMUTABLE,
            )
            views.setOnClickPendingIntent(R.id.widget_root, open)
            manager.updateAppWidget(ids, views)
        }
    }
}
