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

        /** Тосты из сервиса и фоновых задач — показывает MainActivity */
        val toasts = MutableSharedFlow<String>(extraBufferCapacity = 8)

        /** ключи любимых треков и включена ли перемешка — по ним сервис рисует кнопки в уведомлении */
        val liked = MutableStateFlow<Set<String>>(emptySet())
        val shuffleOn = MutableStateFlow(false)

        /** нажатия в уведомлении ("like", "shuffle"): плейлисты и перемешка живут в AppState, он их и выполняет */
        val commands = MutableSharedFlow<String>(extraBufferCapacity = 8)
    }
}
