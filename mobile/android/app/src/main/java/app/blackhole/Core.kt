package app.blackhole

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Rust-ядро (mobile/core): тот же поиск, импорт, тексты и аккаунт, что у плеера на ПК */
object Core {
    init {
        System.loadLibrary("blackhole_core")
    }

    @JvmStatic
    external fun call(method: String, args: String): String
}

val json = Json {
    ignoreUnknownKeys = true
    encodeDefaults = true
    explicitNulls = false
    coerceInputValues = true
}

/** Ошибка из ядра: код Firebase ("EMAIL_EXISTS") или текст */
class CoreError(val code: String) : Exception(code)

/** Блокирующий вызов — только из фонового потока */
fun coreNow(method: String, args: JsonObject = JsonObject(emptyMap())): JsonElement {
    val out = json.parseToJsonElement(Core.call(method, args.toString())).jsonObject
    out["err"]?.let { throw CoreError(it.jsonPrimitive.content) }
    return out["ok"] ?: JsonNull
}

suspend fun core(method: String, build: JsonObjectBuilder.() -> Unit = {}): JsonElement =
    withContext(Dispatchers.IO) { coreNow(method, buildJsonObject(build)) }

inline fun <reified T> JsonElement.to(): T = json.decodeFromJsonElement(this)

fun JsonObjectBuilder.put(key: String, t: Track) = put(key, json.encodeToJsonElement(t))
fun JsonObjectBuilder.put(key: String, v: List<Playlist>) = put(key, json.encodeToJsonElement(v))
