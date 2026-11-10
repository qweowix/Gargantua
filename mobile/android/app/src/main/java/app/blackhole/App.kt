package app.blackhole

import android.app.Application
import androidx.media3.common.util.UnstableApi
import androidx.media3.database.StandaloneDatabaseProvider
import androidx.media3.datasource.cache.LeastRecentlyUsedCacheEvictor
import androidx.media3.datasource.cache.SimpleCache
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import androidx.media3.datasource.ResolvingDataSource
import androidx.media3.datasource.cache.CacheDataSource
import androidx.media3.datasource.okhttp.OkHttpDataSource
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit

@UnstableApi
class App : Application() {
    override fun onCreate() {
        super.onCreate()
        val dirs = buildJsonObject {
            put("files", filesDir.absolutePath)
            put("cache", cacheDir.absolutePath)
        }
        cloudOn = runCatching { coreNow("init", dirs).jsonObject["cloud"]?.jsonPrimitive?.boolean == true }.getOrDefault(false)
        Streams.init(http)
        cache = SimpleCache(java.io.File(filesDir, "audio"), LeastRecentlyUsedCacheEvictor(CACHE_BYTES), StandaloneDatabaseProvider(this))
        YtAuth.init(this)
        BhLog.install()
        Offline.init(this)
        Eq.init(this)
    }

    companion object {
        var cloudOn = false

        /** Прослушанное лежит на диске, как на ПК: повторно играет мгновенно и без интернета. Давно не игравшее вытесняется */
        lateinit var cache: SimpleCache
        private const val CACHE_BYTES = 2L * 1024 * 1024 * 1024 // как на ПК

        val http: OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(20, TimeUnit.SECONDS)
            .build()

        /** Источник звука для плеера и для скачивания: диск (кеш) → сеть. Ссылка нужна, только когда байтов нет
         *  на диске: трек, что уже играл, стартует мгновенно, «назад» — тоже */
        fun cacheSource(): CacheDataSource.Factory {
            val net = ResolvingDataSource.Factory(OkHttpDataSource.Factory(http)) { Streams.resolve(it) }
            return CacheDataSource.Factory().setCache(cache).setUpstreamDataSourceFactory(net)
                .setCacheKeyFactory { spec -> spec.key ?: cacheKey(spec.uri) }
                .setFlags(CacheDataSource.FLAG_IGNORE_CACHE_ON_ERROR)
        }

        /** Куски HLS: подпись в ссылке каждый раз новая, а файл тот же. Сам плейлист не кешируем по пути — в нём свежие подписи */
        private fun cacheKey(uri: android.net.Uri) = if (uri.path?.endsWith(".m3u8") == true) uri.toString() else uri.buildUpon().clearQuery().build().toString()

        /** умное перемешивание: ключи треков, которые сервис подмешал в очередь (не из плейлиста) */
        val smartIds: MutableSet<String> = java.util.concurrent.ConcurrentHashMap.newKeySet()

        /** включили умное перемешивание — сервису пора подмешать первый трек, не дожидаясь смены трека */
        val smartKick = MutableSharedFlow<Unit>(extraBufferCapacity = 2)

        /** таймер сна: время (мс от эпохи), когда музыка затихнет; -1 — после текущего трека; 0 — выключен */
        val sleepAt = MutableStateFlow(0L)

        /** Тосты из сервиса и фоновых задач — показывает MainActivity */
        val toasts = MutableSharedFlow<String>(extraBufferCapacity = 8)

        /** ключи любимых треков и включена ли перемешка — по ним сервис рисует кнопки в уведомлении */
        val liked = MutableStateFlow<Set<String>>(emptySet())
        val shuffleOn = MutableStateFlow(false)

        /** нажатия в уведомлении ("like", "shuffle"): плейлисты и перемешка живут в AppState, он их и выполняет */
        val commands = MutableSharedFlow<String>(extraBufferCapacity = 8)
    }
}
