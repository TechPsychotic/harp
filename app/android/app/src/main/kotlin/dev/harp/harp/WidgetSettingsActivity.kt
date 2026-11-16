package dev.harp.harp

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
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
        val info = manager.getAppWidgetInfo(id)
        val cls = info?.provider?.className ?: ""
        val layout = HarpWidget.layoutFor(cls)
        val p = HarpWidget.prefs(this)
        var alpha = HarpWidget.alphaOf(this, id)
        var lyrics = HarpWidget.lyricsOn(this, id)

        val text = Color.parseColor("#F5F5F7")
        val dim = Color.parseColor("#A1A1A6")
        val accent = Color.parseColor("#FA2D48")
        fun round(color: Int, r: Float) = GradientDrawable().apply { setColor(color); cornerRadius = dp(r).toFloat() }
        fun label(s: CharSequence, size: Float, color: Int, bold: Boolean = false) = TextView(this).apply {
            this.text = s; textSize = size; setTextColor(color)
            if (bold) typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
        }
        fun full(top: Float = 0f) = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(top) }

        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(22f), dp(22f), dp(22f), dp(18f))
            background = round(Color.parseColor("#1C1C1F"), 28f)
        }
        root.addView(label("Widget settings", 20f, text, true))
        root.addView(label(info?.loadLabel(packageManager) ?: "Harp", 14f, dim), full(2f))

        // The preview: the real widget drawn over a stand-in wallpaper, so opacity shows.
        val stage = FrameLayout(this).apply {
            background = GradientDrawable(GradientDrawable.Orientation.TL_BR, intArrayOf(Color.parseColor("#3A6073"), Color.parseColor("#16222A"), Color.parseColor("#734B6D"))).apply { cornerRadius = dp(20f).toFloat() }
            setPadding(dp(14f), dp(18f), dp(14f), dp(18f))
        }
        val (pw, ph) = when (layout) {
            R.layout.harp_widget_mini -> dp(170f) to dp(76f)
            R.layout.harp_widget_cover -> dp(170f) to dp(170f)
            R.layout.harp_widget -> ViewGroup.LayoutParams.MATCH_PARENT to dp(76f)
            else -> ViewGroup.LayoutParams.MATCH_PARENT to dp(180f)
        }
        fun preview() {
            p.edit().putInt("alpha_$id", alpha).putBoolean("lyrics_$id", lyrics).apply()
            stage.removeAllViews()
            val v = HarpWidget.views(this, layout, id).apply(this, stage)
            stage.addView(v, FrameLayout.LayoutParams(pw, ph, Gravity.CENTER))
        }
        root.addView(stage, full(16f))

        // Background opacity: the name and the value on one line, the slider under it.
        val opacityRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        opacityRow.addView(label("Background", 16f, text), LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        val value = label("$alpha%", 16f, dim)
        opacityRow.addView(value)
        root.addView(opacityRow, full(20f))
        root.addView(label("Lower lets more of the wallpaper through.", 13f, dim), full(2f))
        root.addView(SeekBar(this).apply {
            max = 100
            progress = alpha
            progressTintList = ColorStateList.valueOf(accent)
            thumbTintList = ColorStateList.valueOf(accent)
            progressBackgroundTintList = ColorStateList.valueOf(Color.parseColor("#48484A"))
            setPadding(0, dp(8f), 0, dp(8f))
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar?, v: Int, fromUser: Boolean) {
                    alpha = v
                    value.text = "$alpha%"
                    preview()
                }
                override fun onStartTrackingTouch(s: SeekBar?) {}
                override fun onStopTrackingTouch(s: SeekBar?) {}
            })
        }, full(6f))

        if (layout == R.layout.harp_widget_large) {
            @Suppress("DEPRECATION")
            val sw = Switch(this).apply {
                isChecked = lyrics
                thumbTintList = ColorStateList(arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()), intArrayOf(Color.WHITE, Color.parseColor("#D1D1D6")))
                trackTintList = ColorStateList(arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()), intArrayOf(accent, Color.parseColor("#48484A")))
                setOnCheckedChangeListener { _, on -> lyrics = on; preview() }
            }
            val row = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                setOnClickListener { sw.isChecked = !sw.isChecked }
            }
            val words = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
            words.addView(label("Live lyrics", 16f, text))
            words.addView(label("The line being sung, for songs with synced lyrics.", 13f, dim), full(2f))
            row.addView(words, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { marginEnd = dp(12f) })
            row.addView(sw)
            root.addView(View(this).apply { setBackgroundColor(Color.parseColor("#2C2C2E")) }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(1f)).apply { topMargin = dp(14f) })
            root.addView(row, full(14f))
        }

        root.addView(TextView(this).apply {
            this.text = "Done"
            textSize = 16f
            setTextColor(Color.WHITE)
            typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
            gravity = Gravity.CENTER
            background = round(accent, 24f)
            setOnClickListener { finish() }
        }, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(48f)).apply { topMargin = dp(22f) })

        setContentView(root)
        // A set width: the dialog otherwise shrinks to its content and squeezes everything.
        val screen = resources.displayMetrics.widthPixels
        window.setLayout(minOf(screen - dp(32f), dp(420f)), ViewGroup.LayoutParams.WRAP_CONTENT)
        preview()
    }

    override fun onPause() {
        super.onPause()
        HarpWidget.refreshAll(this)
    }
}
