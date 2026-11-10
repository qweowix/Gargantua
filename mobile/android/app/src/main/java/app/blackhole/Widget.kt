package app.blackhole

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.widget.RemoteViews
import androidx.media3.common.util.UnstableApi
import coil3.SingletonImageLoader
import coil3.request.ImageRequest
import coil3.request.allowHardware
import coil3.toBitmap

/** Виджет на рабочий стол: обложка, название и три кнопки. Рисует его сервис плеера при каждой смене трека
 *  и паузе; пока музыка не запущена, нажатие открывает приложение */
@UnstableApi
class Widget : AppWidgetProvider() {
    override fun onUpdate(ctx: Context, mgr: AppWidgetManager, ids: IntArray) {
        update(ctx, last?.first, last?.second == true, lastArt)
    }

    override fun onReceive(ctx: Context, intent: Intent) {
        super.onReceive(ctx, intent)
        val cmd = intent.action?.removePrefix("app.blackhole.widget.")?.takeIf { it != intent.action } ?: return
        val svc = PlayerService.live
        if (svc != null) svc.widgetCommand(cmd)
        else ctx.startActivity(Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    companion object {
        private var last: Pair<Track?, Boolean>? = null
        private var lastArt: Bitmap? = null

        private fun click(ctx: Context, cmd: String) = PendingIntent.getBroadcast(
            ctx, cmd.hashCode(), Intent(ctx, Widget::class.java).setAction("app.blackhole.widget.$cmd"), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        fun update(ctx: Context, t: Track?, playing: Boolean, art: Bitmap? = null) {
            last = t to playing
            lastArt = art
            val mgr = AppWidgetManager.getInstance(ctx)
            val ids = mgr.getAppWidgetIds(ComponentName(ctx, Widget::class.java))
            if (ids.isEmpty()) return
            val v = RemoteViews(ctx.packageName, R.layout.widget)
            v.setTextViewText(R.id.w_title, t?.title ?: "Gargantua")
            v.setTextViewText(R.id.w_artist, t?.artist ?: tr("Ничего не играет", "Nothing is playing", "Нічого не грає", "Nič nehrá"))
            if (art != null) v.setImageViewBitmap(R.id.w_cover, art) else v.setImageViewResource(R.id.w_cover, R.mipmap.ic_launcher)
            v.setImageViewResource(R.id.w_play, if (playing) R.drawable.w_pause else R.drawable.w_play)
            v.setOnClickPendingIntent(R.id.w_prev, click(ctx, "prev"))
            v.setOnClickPendingIntent(R.id.w_play, click(ctx, "toggle"))
            v.setOnClickPendingIntent(R.id.w_next, click(ctx, "next"))
            val open = PendingIntent.getActivity(ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            v.setOnClickPendingIntent(R.id.w_cover, open)
            v.setOnClickPendingIntent(R.id.w_text, open)
            runCatching { mgr.updateAppWidget(ids, v) }
        }

        /** Обложка для виджета: маленькая (RemoteViews не любит большие картинки) и не аппаратная */
        suspend fun art(ctx: Context, url: String): Bitmap? {
            val req = ImageRequest.Builder(ctx).data(url).size(256).allowHardware(false).build()
            return SingletonImageLoader.get(ctx).execute(req).image?.toBitmap()
        }
    }
}
