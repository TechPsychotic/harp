package dev.harp.harp

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Shader
import android.view.KeyEvent
import android.view.View
import android.widget.RemoteViews

/**
 * Harp's home-screen widgets, all showing what's playing:
 *  - [HarpWidget]: cover, song and previous / play-pause / next in one row (4×1).
 *  - [HarpMiniWidget]: song with play-pause and next (2×1).
 *  - [HarpCoverWidget]: the cover with the song and a play-pause button on it (2×2).
 *  - [HarpLargeWidget]: big cover, song and the three buttons; or, with live lyrics (on by default),
 *    the line being sung large under the song (4×2 and up).
 * Each widget has its own settings (background opacity; live lyrics for the large one), see
 * [WidgetSettingsActivity]. The app tells them what's playing (MainActivity "widget",
 * "widgetLyric"); the buttons send media-button presses to Harp's media service, like
 * headphone buttons, so they work without the app's screen.
 */
open class HarpWidget : AppWidgetProvider() {
    open val layout = R.layout.harp_widget

    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        for (id in ids) {
            try { manager.updateAppWidget(id, views(context, layout, id)) } catch (_: Exception) {}
        }
    }

    override fun onDeleted(context: Context, ids: IntArray) {
        val e = prefs(context).edit()
        for (id in ids) e.remove("alpha_$id").remove("lyrics_$id")
        e.apply()
    }

    companion object {
        private const val PREFS = "harp_widget"
        private const val DEFAULT_ALPHA = 92

        fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

        /** Background opacity, percent. */
        fun alphaOf(context: Context, id: Int) = prefs(context).getInt("alpha_$id", DEFAULT_ALPHA)

        /** Live lyrics on "Now playing": on unless turned off in its settings. */
        fun lyricsOn(context: Context, id: Int) = prefs(context).getBoolean("lyrics_$id", true)

        private val kinds = listOf(
            HarpWidget::class.java to R.layout.harp_widget,
            HarpMiniWidget::class.java to R.layout.harp_widget_mini,
            HarpCoverWidget::class.java to R.layout.harp_widget_cover,
            HarpLargeWidget::class.java to R.layout.harp_widget_large,
        )

        fun layoutFor(className: String) = kinds.firstOrNull { it.first.name == className }?.second ?: R.layout.harp_widget

        /** The app calls this when the song or play state changes. */
        fun update(context: Context, title: String, artist: String, art: String, playing: Boolean) {
            prefs(context).edit()
                .putString("title", title).putString("artist", artist).putString("art", art)
                .putBoolean("playing", playing).apply()
            refreshAll(context)
        }

        /** The lyric line being sung and the next ("" when there are none). */
        fun lyric(context: Context, line: String, next: String) {
            val p = prefs(context)
            if (p.getString("lyric", "") == line && p.getString("lyric_next", "") == next) return
            p.edit().putString("lyric", line).putString("lyric_next", next).apply()
            val manager = AppWidgetManager.getInstance(context)
            for (id in manager.getAppWidgetIds(ComponentName(context, HarpLargeWidget::class.java))) {
                if (lyricsOn(context, id)) try { manager.updateAppWidget(id, views(context, R.layout.harp_widget_large, id)) } catch (_: Exception) {}
            }
        }

        /** Whether any widget shows live lyrics (the app follows them only then). */
        fun lyricsWanted(context: Context): Boolean {
            val ids = AppWidgetManager.getInstance(context).getAppWidgetIds(ComponentName(context, HarpLargeWidget::class.java))
            return ids.any { lyricsOn(context, it) }
        }

        fun refreshAll(context: Context) {
            val manager = AppWidgetManager.getInstance(context)
            for ((cls, layout) in kinds) {
                for (id in manager.getAppWidgetIds(ComponentName(context, cls))) {
                    // Each on its own: one failing (a cover too big, say) doesn't stop the others.
                    try { manager.updateAppWidget(id, views(context, layout, id)) } catch (_: Exception) {}
                }
            }
        }

        private fun mediaKey(context: Context, code: Int, request: Int): PendingIntent {
            val intent = Intent(Intent.ACTION_MEDIA_BUTTON).apply {
                component = ComponentName(context, "com.ryanheise.audioservice.MediaButtonReceiver")
                putExtra(Intent.EXTRA_KEY_EVENT, KeyEvent(KeyEvent.ACTION_DOWN, code))
            }
            return PendingIntent.getBroadcast(context, request, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }

        /** One widget's views. */
        fun views(context: Context, layout: Int, id: Int): RemoteViews {
            val p = prefs(context)
            val views = RemoteViews(context.packageName, layout)
            val alpha = alphaOf(context, id) * 255 / 100
            views.setInt(R.id.widget_bg, "setImageAlpha", alpha)
            val title = p.getString("title", null)
            val artist = if (title == null) "Tap to open" else p.getString("artist", null) ?: ""
            val playing = p.getBoolean("playing", false)
            views.setImageViewResource(R.id.widget_play, if (playing) R.drawable.ic_w_pause else R.drawable.ic_w_play)

            val large = layout == R.layout.harp_widget_large
            val lyric = p.getString("lyric", "") ?: ""
            val showLyrics = large && lyricsOn(context, id) && lyric.isNotEmpty() && title != null
            views.setTextViewText(R.id.widget_title, title ?: "Harp")
            views.setTextViewText(R.id.widget_artist, artist)
            if (large) {
                // Two arrangements: the song with a big cover, or (live lyrics) the line being sung.
                views.setViewVisibility(R.id.widget_plain, if (showLyrics) View.GONE else View.VISIBLE)
                views.setViewVisibility(R.id.widget_lyrics, if (showLyrics) View.VISIBLE else View.GONE)
                views.setTextViewText(R.id.widget_title_l, title ?: "Harp")
                views.setTextViewText(R.id.widget_artist_l, artist)
                views.setTextViewText(R.id.widget_lyric, lyric)
                val next = p.getString("lyric_next", "") ?: ""
                views.setViewVisibility(R.id.widget_lyric_next, if (next.isEmpty()) View.GONE else View.VISIBLE)
                views.setTextViewText(R.id.widget_lyric_next, next)
                views.setImageViewResource(R.id.widget_play_l, if (playing) R.drawable.ic_w_pause else R.drawable.ic_w_play)
            }

            if (layout != R.layout.harp_widget_mini) {
                val art = p.getString("art", null)
                val cover = layout == R.layout.harp_widget_cover
                val bitmap = try {
                    if (art.isNullOrEmpty()) null else decodeArt(art, if (cover || large) 400 else 200)
                } catch (_: Exception) { null }
                // The cover widget is all cover (the widget clips its corners); the others round theirs.
                val shown = if (bitmap == null || cover) bitmap else rounded(bitmap, bitmap.width * 0.12f)
                val artIds = if (large) listOf(R.id.widget_art, R.id.widget_art_l) else listOf(R.id.widget_art)
                for (artId in artIds) {
                    if (shown != null) views.setImageViewBitmap(artId, shown)
                    else views.setImageViewResource(artId, if (cover) R.drawable.widget_art_fill else R.drawable.widget_preview_art)
                }
                // On the cover widget the cover is the background: it follows the opacity too.
                if (cover) views.setInt(R.id.widget_art, "setImageAlpha", maxOf(alpha, 60))
            }
            if (layout == R.layout.harp_widget || large) {
                views.setOnClickPendingIntent(R.id.widget_prev, mediaKey(context, KeyEvent.KEYCODE_MEDIA_PREVIOUS, 1))
            }
            views.setOnClickPendingIntent(R.id.widget_play, mediaKey(context, KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, 2))
            if (layout != R.layout.harp_widget_cover) {
                views.setOnClickPendingIntent(R.id.widget_next, mediaKey(context, KeyEvent.KEYCODE_MEDIA_NEXT, 3))
            }
            if (large) {
                views.setOnClickPendingIntent(R.id.widget_prev_l, mediaKey(context, KeyEvent.KEYCODE_MEDIA_PREVIOUS, 1))
                views.setOnClickPendingIntent(R.id.widget_play_l, mediaKey(context, KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, 2))
                views.setOnClickPendingIntent(R.id.widget_next_l, mediaKey(context, KeyEvent.KEYCODE_MEDIA_NEXT, 3))
            }
            val open = PendingIntent.getActivity(
                context, 0, Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                PendingIntent.FLAG_IMMUTABLE,
            )
            views.setOnClickPendingIntent(R.id.widget_root, open)
            return views
        }

        /** The cover scaled down to about [max] px: widgets have a small memory limit. */
        private fun decodeArt(path: String, max: Int): Bitmap? {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(path, bounds)
            var sample = 1
            while (bounds.outWidth / (sample * 2) >= max && bounds.outHeight / (sample * 2) >= max) sample *= 2
            val b = BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
            val side = maxOf(b.width, b.height)
            if (side <= max) return b
            val k = max.toFloat() / side
            return Bitmap.createScaledBitmap(b, (b.width * k).toInt().coerceAtLeast(1), (b.height * k).toInt().coerceAtLeast(1), true)
        }

        /** The cover with rounded corners (widgets can't clip an image themselves everywhere). */
        private fun rounded(b: Bitmap, radius: Float): Bitmap {
            val out = Bitmap.createBitmap(b.width, b.height, Bitmap.Config.ARGB_8888)
            val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { shader = BitmapShader(b, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP) }
            Canvas(out).drawRoundRect(RectF(0f, 0f, b.width.toFloat(), b.height.toFloat()), radius, radius, paint)
            return out
        }
    }
}

class HarpMiniWidget : HarpWidget() {
    override val layout = R.layout.harp_widget_mini
}

class HarpCoverWidget : HarpWidget() {
    override val layout = R.layout.harp_widget_cover
}

class HarpLargeWidget : HarpWidget() {
    override val layout = R.layout.harp_widget_large
}
