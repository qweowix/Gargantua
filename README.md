<p align="center">
  <img src="assets/icon-256.png" width="128" alt="Gargantua">
</p>

<h1 align="center">Gargantua</h1>

<p align="center">
  <b>Все сервисы — одна дыра.</b><br>
  Бесплатный музыкальный плеер для Linux, Windows и Android: YouTube Music, SoundCloud, Spotify, Apple Music, Яндекс Музыка и твои файлы — в одном окне. Без рекламы: вход по почте, плейлисты в аккаунте и одинаковые на компьютере и телефоне, всё остальное хранится у тебя.
</p>

<p align="center">
  <a href="https://github.com/qweowix/Gargantua/releases/latest"><b>Скачать</b></a> ·
  <a href="https://t.me/BlackHolePlayer">Telegram</a> ·
  <a href="#english">English</a>
</p>

![Главная](docs/home.png)

## Что умеет

- **Один поиск по всем сервисам.** YouTube Music, SoundCloud, Яндекс Музыка; артисты и альбомы каруселями сверху, результаты по мере набора. Вкладка «В текстах» ищет песню по строчке из неё.
- **Импорт плейлистов по ссылке** из Spotify, Apple Music, YouTube Music, SoundCloud и Яндекса. Треки из Spotify и Apple сразу подбираются на YouTube Music; сомнительные совпадения плеер показывает и спрашивает.
- **Компьютер и телефон заодно.** Приложение для Android с теми же плейлистами и статистикой. **Connect**: звук играет на одном устройстве, управлять можно с любого, сессия перекидывается одним нажатием.
- **Тексты песен по словам**, как в Apple Music: караоке-заливка, бэк-вокал и дуэты, несколько стилей и эффектов ухода строки (тускнеют, сдув, в дыру, падение, взрыв…). Источники — lrclib, Liriqo, Spicy Lyrics.
- **Панель «Сейчас играет»**: обложка, спектр и текст или очередь справа от списка. Очередь и плейлисты перетаскиваются мышью.
- **Заметки к моментам трека**: метка с подписью на нужной секунде, точками на полосе прогресса.
- **Локальные файлы** рядом со стримингом: папки с музыкой, теги и обложки из файлов.
- **Главная с миксами** — «Твой микс», миксы по любимым артистам и «Открытия», плюс умные подборки из статистики: «Давно не слушал», «Заслушано в этом месяце», «Лайкнул и забыл».
- **Статистика**: сегодня, неделя, месяц, год прослушиваний квадратиками как на GitHub, любимый артист и топ треков, история прослушиваний, итоги месяца и года карточкой, которую можно сохранить картинкой.
- **Офлайн**: дослушанные треки тихо сохраняются на диск и потом играют без интернета; на телефоне плейлист можно скачать целиком.
- **Скачивание в mp3** с тегами и обложкой.
- **Звук**: кроссфейд, 10-полосный эквалайзер с пресетами, скорость 0.5–2× (с тональностью или как slowed / sped up), таймер сна. На телефоне — тот же эквалайзер.
- **Умное перемешивание**: раз в 2–3 трека подмешивает похожий не из плейлиста. **Радио по треку** и «дальше похожее», когда плейлист закончился.
- **Горячие клавиши**, все переназначаются в настройках.
- **Discord**: статус «Слушает» с обложкой и кнопкой на трек.
- **15 тем** плюс цвета из обложки текущего трека (включены по умолчанию).
- **Обмен плейлистами через QR-код.**
- **Аккаунт**: вход по почте, плейлисты хранятся на сервере и одинаковые на всех устройствах. Правки улетают сразу, свежие подтягиваются при запуске, раз в минуту и по кнопке. Настройка своего Firebase — [docs/firebase.md](docs/firebase.md).
- **Обновляется сам**: находит новую версию на GitHub и ставит её по кнопке.
- Медиа-клавиши и системные контролы (MPRIS на Linux), виджет на рабочий стол на Android, русский / английский / украинский / словацкий интерфейс.
- **Лёгкий**: нативный интерфейс на Rust и Slint, без браузерного движка внутри.

| | |
|---|---|
| ![Текст песни](docs/lyrics.png) | ![Текст песни в цвет обложки](docs/lyrics-tvgirl.png) |
| ![Плейлист](docs/playlist.png) | ![Статистика](docs/stats.png) |
| ![Настройки](docs/settings.png) | ![О программе](docs/about.png) |

## Установка

### Windows

1. Скачай `Gargantua-windows-x86_64.zip` из [релизов](https://github.com/qweowix/Gargantua/releases/latest).
2. Распакуй куда угодно и запусти `Gargantua.exe`.

mpv и yt-dlp уже лежат в папке `bin`, ставить ничего не надо. Для скачивания в mp3 положи туда же `ffmpeg.exe`.

### Linux

1. Поставь зависимости:
   ```sh
   # Arch / CachyOS / Manjaro
   sudo pacman -S mpv yt-dlp python
   # Debian / Ubuntu
   sudo apt install mpv python3 pipx && pipx install yt-dlp
   # Fedora
   sudo dnf install mpv yt-dlp python3
   ```
   `ffmpeg` нужен только для скачивания в mp3.
2. Скачай `Gargantua-linux-x86_64.tar.gz` из [релизов](https://github.com/qweowix/Gargantua/releases/latest), распакуй и запусти `./install.sh` — плеер появится в меню приложений и в `~/.local/bin`. Или просто запусти `./blackhole` из папки.

yt-dlp лучше держать свежим (`yt-dlp -U` или через пакетный менеджер): YouTube постоянно что-то меняет. В дистрибутивах со старым yt-dlp ставь его через pipx.

### Android

Скачай `Gargantua-android-arm64.apk` из [релизов](https://github.com/qweowix/Gargantua/releases/latest) и установи (Android 8+, arm64). Войди в тот же аккаунт, что на компьютере, — плейлисты подтянутся сами. Если YouTube не отдаёт звук, на компьютере нажми Настройки → Куки YouTube → «На телефон», а на телефоне — «Взять с компа».

### macOS

Готовой сборки пока нет, собирается из исходников: `tools/build-macos.sh` (нужны `brew install mpv yt-dlp`).

## Если YouTube пишет «Sign in to confirm you're not a bot»

Иногда YouTube помечает IP (общежитие, общий NAT) и перестаёт отдавать звук без входа. Открой настройки → **Куки YouTube** и выбери браузер, в котором ты залогинен в YouTube. Плеер сначала пробует без кук и подключает их, только когда YouTube отказал.

## Горячие клавиши

| Клавиша | Действие |
|---|---|
| `Space` | пауза / играть |
| `←` / `→` | перемотка назад / вперёд |
| `Ctrl+←` / `Ctrl+→` | предыдущий / следующий трек |
| `↑` / `↓` | громкость |
| `M` | без звука |
| `L` | текст песни |
| `S` / `R` | перемешать / повтор |
| `F` | во весь экран |
| `Ctrl+F` | поиск |
| `Ctrl+R` | синхронизировать плейлисты с аккаунтом |

Любую можно переназначить в настройках. Клавиши работают по месту на клавиатуре, раскладка не важна.

## Где лежат данные

| | Linux | Windows |
|---|---|---|
| плейлисты | в аккаунте (Firestore) | в аккаунте |
| настройки, вход | `~/.config/BlackHole/` | `%APPDATA%\BlackHole\` |
| статистика | там же, `stats.json` | там же |
| история прослушиваний | там же, `history.json` | там же |
| кэш (тексты, обложки, офлайн-треки, заметки, журнал ошибок) | `~/.cache/BlackHole/` | `%LOCALAPPDATA%\BlackHole\` |
| скачанные mp3 | `~/Music/BlackHole` (меняется в настройках) | `Музыка\BlackHole` |

## Сборка из исходников

Нужен Rust (stable).

```sh
git clone https://github.com/qweowix/Gargantua
cd Gargantua
cargo build --release
./target/release/blackhole
```

На Linux для сборки нужны заголовки fontconfig, freetype и xkbcommon (на Ubuntu: `libfontconfig1-dev libfreetype-dev libxkbcommon-dev`). На Windows — обычный MSVC-тулчейн; иконка exe вшивается сама. mpv и yt-dlp плеер ищет рядом с exe, в `bin/` рядом с exe, в `PATH` и в `~/.local/bin`.

Тесты: `cargo test --release`. Снимки интерфейса без окна (программным рендером) — `cargo test --release -- --ignored shots`, картинки появятся в `target/shots/`.

Android-приложение лежит в `mobile/`: Rust-ядро из тех же исходников (`mobile/build-core.sh`, нужен Android NDK) и оболочка на Kotlin / Compose (`mobile/android`, `./gradlew assembleRelease`).

Релизы собирает GitHub Actions (`.github/workflows/release.yml`): пушнул тег `v*` — архивы для Linux и Windows и APK появились в Releases. Для APK в секретах репозитория должен лежать ключ подписи (`ANDROID_KEYSTORE`, `ANDROID_STORE_PASSWORD`, `ANDROID_KEY_PASSWORD`).

### Как устроено

- Интерфейс — [Slint](https://slint.dev) со Skia-рендером, весь в `ui/app.slint`.
- Звук — [mpv](https://mpv.io) по JSON IPC, ссылки на потоки — [yt-dlp](https://github.com/yt-dlp/yt-dlp), который живёт в фоне и не запускается заново на каждый трек.
- Поиск и импорт — внутренние веб-API сервисов, ключи и аккаунты не нужны.
- Телефон — то же Rust-ядро через JNI, звук — Media3 (ExoPlayer), ссылки на потоки — [NewPipeExtractor](https://github.com/TeamNewPipe/NewPipeExtractor).
- Аккаунты — Firebase Auth и Firestore по REST (`src/cloud.rs`), проект задаётся переменными при сборке, см. [docs/firebase.md](docs/firebase.md).
- Переводы — `lang/*/LC_MESSAGES/blackhole.po`.

## Благодарности

[lrclib](https://lrclib.net), Liriqo и [Spicy Lyrics](https://spicylyrics.org) — тексты песен. [mpv](https://mpv.io), [yt-dlp](https://github.com/yt-dlp/yt-dlp) и [NewPipeExtractor](https://github.com/TeamNewPipe/NewPipeExtractor) — без них ничего бы не играло. Интерфейс сделан на [Slint](https://slint.dev).

## Лицензия

Код Gargantua — под [MIT](LICENSE): бери, меняй и распространяй как угодно, сохранив строку об авторстве.

Сторонние части остаются под своими лицензиями. В архиве для Windows рядом лежат mpv (GPL/LGPL) и yt-dlp (Unlicense) — это отдельные программы. APK включает NewPipeExtractor под GPL-3.0, поэтому собранный APK целиком распространяется на условиях GPL-3.0. Интерфейс использует Slint по его бесплатной лицензии.

---

<a name="english"></a>
## English

**Gargantua** is a free music player for Linux, Windows and Android. It searches YouTube Music, SoundCloud and Yandex Music at once, imports playlists from Spotify, Apple Music and others by link, plays your local files, and shows word-by-word synced lyrics with karaoke effects. It has no ads. You sign in with email, and your playlists live in your account, the same on every computer. Everything else is stored locally.

Also included: daily mixes, listening stats with a GitHub-style heatmap, an offline cache, mp3 downloads, crossfade, an EQ, a sleep timer, rebindable hotkeys, Discord Rich Presence, 15 themes plus a cover-coloured theme, QR playlist sharing, and an English/Russian/Ukrainian UI.

Since 1.0: an Android app sharing playlists and stats with the desktop player, Connect (play on one device, control from any, hand the session over), a “Now Playing” panel with a spectrum, smart shuffle, notes at moments of a track, search by a lyric line, listening history, month and year recaps, and self-update.

**Install:** grab the archive for your OS from [Releases](https://github.com/qweowix/Gargantua/releases/latest).
- **Windows:** unzip it and run `Gargantua.exe`; mpv and yt-dlp are bundled.
- **Linux:** install `mpv`, `yt-dlp` and `python3`, unpack the archive and run `./install.sh`.
- **Android:** install `Gargantua-android-arm64.apk` (Android 8+, arm64) and sign in to the same account.

**Build from source:** `cargo build --release`.

**License:** [MIT](LICENSE) for Gargantua's own code. Bundled third-party parts keep their licenses; the APK includes NewPipeExtractor (GPL-3.0), so the built APK as a whole is distributed under GPL-3.0 terms.
