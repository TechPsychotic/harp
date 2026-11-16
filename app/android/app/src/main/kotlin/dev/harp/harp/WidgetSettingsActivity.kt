package dev.harp.harp

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.Intent
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.util.TypedValue
import android.view.Gravity
import android.view.ViewGroup
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.SeekBar
import android.widget.Switch
import android.widget.TextView

/**
 * A widget's settings: shown when it's placed and from its "Settings" (long-press the widget).
 * Background opacity, with a live preview of the widget; live lyrics for "Harp now playing".
 */
class WidgetSettingsActivity : Activity() {
    private var id = AppWidgetManager.INVALID_APPWIDGET_ID

    private fun dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setBackgroundDrawableResource(android.R.color.transparent)
        id = intent?.extras?.getInt(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID) ?: AppWidgetManager.INVALID_APPWIDGET_ID
        // Backing out of the first setup keeps the widget with its defaults.
        setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id))
        if (id == AppWidgetManager.INVALID_APPWIDGET_ID) { finish(); return }
        val manager = AppWidgetManager.getInstance(this)
        val cls = manager.getAppWidgetInfo(id)?.provider?.className ?: ""
        val layout = HarpWidget.layoutFor(cls)
        val p = HarpWidget.prefs(this)
        var alpha = HarpWidget.alphaOf(this, id)
        var lyrics = HarpWidget.lyricsOn(this, id)

        val text = Color.parseColor("#F5F5F7")
        val dim = Color.parseColor("#A1A1A6")
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24f), dp(24f), dp(24f), dp(20f))
            background = GradientDrawable().apply { setColor(Color.parseColor("#1C1C1F")); cornerRadius = dp(28f).toFloat() }
        }
        fun label(s: String, size: Float, color: Int, top: Int = 0) = TextView(this).apply {
            this.text = s; textSize = size; setTextColor(color); setPadding(0, dp(top.toFloat()), 0, 0)
        }
        root.addView(label("Widget", 22f, text))
        // The preview: the real widget drawn over a stand-in wallpaper, so opacity shows.
        val stage = FrameLayout(this).apply {
            background = GradientDrawable(GradientDrawable.Orientation.TL_BR, intArrayOf(Color.parseColor("#3A6073"), Color.parseColor("#16222A"), Color.parseColor("#734B6D"))).apply { cornerRadius = dp(20f).toFloat() }
            setPadding(dp(14f), dp(14f), dp(14f), dp(14f))
        }
        val previewHeight = when (layout) {
            R.layout.harp_widget_mini -> 72f
            R.layout.harp_widget -> 80f
            R.layout.harp_widget_cover -> 160f
            else -> 170f
        }
        fun preview() {
            p.edit().putInt("alpha_$id", alpha).putBoolean("lyrics_$id", lyrics).apply()
            stage.removeAllViews()
            val v = HarpWidget.views(this, layout, id).apply(this, stage)
            val w = if (layout == R.layout.harp_widget_cover || layout == R.layout.harp_widget_mini) dp(180f) else ViewGroup.LayoutParams.MATCH_PARENT
            stage.addView(v, FrameLayout.LayoutParams(w, dp(previewHeight), Gravity.CENTER))
        }
        root.addView(stage, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(16f) })

        val opacityLabel = label("Background: $alpha%", 15f, text, 20)
        root.addView(opacityLabel)
        root.addView(label("Lower is more see-through.", 13f, dim))
        root.addView(SeekBar(this).apply {
            max = 100
            progress = alpha
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar?, v: Int, fromUser: Boolean) {
                    alpha = v
                    opacityLabel.text = "Background: $alpha%"
                    preview()
                }
                override fun onStartTrackingTouch(s: SeekBar?) {}
                override fun onStopTrackingTouch(s: SeekBar?) {}
            })
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(8f) })

        if (layout == R.layout.harp_widget_large) {
            @Suppress("DEPRECATION")
            root.addView(Switch(this).apply {
                this.text = "Live lyrics"
                textSize = 15f
                setTextColor(text)
                isChecked = lyrics
                setOnCheckedChangeListener { _, on -> lyrics = on; preview() }
            }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(20f) })
            root.addView(label("Shows the line being sung, when the song has synced lyrics.", 13f, dim))
        }

        root.addView(Button(this).apply {
            this.text = "Done"
            setOnClickListener { finish() }
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(20f); gravity = Gravity.END })

        setContentView(FrameLayout(this).apply {
            setPadding(dp(16f), dp(16f), dp(16f), dp(16f))
            addView(root, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER))
        })
        preview()
    }

    override fun onPause() {
        super.onPause()
        HarpWidget.refreshAll(this)
    }
}
