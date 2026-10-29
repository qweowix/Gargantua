package app.blackhole

import androidx.media3.common.util.UnstableApi
import okhttp3.OkHttpClient
import okhttp3.Request
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Живая проверка NewPipe (один трек): BH_LIVE=1 ./gradlew testReleaseUnitTest */
@UnstableApi
class StreamLiveTest {
    @Test
    fun youtubeAudioOpens() {
        assumeTrue(System.getenv("BH_LIVE") != null)
        val http = OkHttpClient()
        Streams.init(http)
        val url = Streams.audio("https://music.youtube.com/watch?v=7j6C9METNm0")
        println("звук: ${url?.take(120)}…")
        assertTrue("нет ссылки", url != null)
        val req = Request.Builder().url(url!!).header("Range", "bytes=0-65535")
        Streams.headersFor(url).forEach { (k, v) -> req.header(k, v) }
        http.newCall(req.build()).execute().use { r ->
            val n = r.body?.bytes()?.size ?: 0
            println("ответ: ${r.code}, ${n} байт, клиент c=${url.substringAfter("&c=").substringBefore("&")}")
            assertTrue("YouTube не отдал звук: ${r.code}", r.code == 200 || r.code == 206)
        }
    }
}
