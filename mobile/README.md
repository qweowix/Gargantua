# Gargantua для Android

Та же логика, что у плеера на ПК: поиск (YT Music, SoundCloud, Яндекс), импорт плейлистов по ссылке
(Spotify, Apple Music, YT Music, SoundCloud, Яндекс), тексты (lrclib, Liriqo, Spicy Lyrics), миксы на главной,
статистика и тот же аккаунт — плейлисты общие с компьютером.

- `core/` — Rust-ядро. Модули не копии: подключаются из `../src` через `#[path]`, наружу — один JNI-метод
  `Core.call(method, json)` (см. `core/src/api.rs`).
- `android/` — Kotlin + Jetpack Compose. Звук — Media3/ExoPlayer в `PlayerService` (уведомление, экран
  блокировки, наушники). Прямые ссылки на звук достаёт NewPipeExtractor: yt-dlp на телефоне не запустить.

## Сборка

Нужны: rustup с целью `aarch64-linux-android`, Android SDK (platform 36, build-tools 36) и NDK, JDK 17+.

```sh
rustup target add aarch64-linux-android
./build-core.sh                      # ядро → android/app/src/main/jniLibs/arm64-v8a
cd android && ./gradlew assembleRelease
# → android/app/build/outputs/apk/release/app-release.apk
```

Ключи Firebase берутся так же, как для ПК: `BLACKHOLE_FIREBASE_KEY` / `BLACKHOLE_FIREBASE_PROJECT`
в окружении или в `.cargo/config.toml` корня репо.

Подпись: `android/release.jks` + пароли `bh.storePassword` / `bh.keyPassword` в `android/local.properties`.
Оба файла в git не едут. **Потеряешь ключ — новые версии не встанут поверх старой**, только через удаление.

Пока только arm64 (почти все телефоны с 2017 года).
