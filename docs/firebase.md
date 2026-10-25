# Аккаунты: настройка Firebase

Аккаунты и синхронизация плейлистов работают через твой проект Firebase: вход по почте и паролю — Firebase Auth, плейлисты — Firestore. Вход обязателен: плеер без аккаунта дальше экрана входа не пускает, плейлисты хранятся только на сервере. Сборка без настроенного проекта работает по-старому, без входа, с плейлистами на диске (для разработки).

## 1. Проект

1. [console.firebase.google.com](https://console.firebase.google.com) → **Add project**. Google Analytics не нужен.
2. **Build → Authentication → Get started → Sign-in method → Email/Password** → включить (первый переключатель, «Email link» не нужен).
3. **Authentication → Templates → Email address verification**: тут меняются текст письма и имя отправителя. Язык письма плеер выбирает сам по языку интерфейса.
4. **Build → Firestore Database → Create database**: режим **production**, регион любой (для Европы `eur3` или `europe-west`).
5. **Firestore → Rules**: вставить содержимое [`packaging/firestore.rules`](../packaging/firestore.rules) → **Publish**.

## 2. Ключи

**Project settings** (шестерёнка) → **General**:
- **Project ID** → `BLACKHOLE_FIREBASE_PROJECT`;
- **Web API Key** → `BLACKHOLE_FIREBASE_KEY`. Если ключа нет, добавь веб-приложение (значок `</>`), он появится.

Web API key — не секрет: он только говорит, к какому проекту стучаться, а доступ к данным держат правила Firestore. Но в код он не вшит, его подставляют при сборке.

## 3. Сборка

Локально:
```sh
BLACKHOLE_FIREBASE_KEY=AIza... BLACKHOLE_FIREBASE_PROJECT=blackhole-xxxx cargo build --release
```
Те же переменные, заданные при запуске, перекрывают вшитые.

Релизы: **GitHub → Settings → Secrets and variables → Actions** → секреты `FIREBASE_KEY` и `FIREBASE_PROJECT`. Workflow сам передаёт их в сборку.

## Как работает синхронизация

- Плейлисты в `data.json` не пишутся. Старые плейлисты с диска при первом запуске откладываются в `playlists-local.json` и при первом входе доливаются в аккаунт; файл потом остаётся копией `playlists-local.uploaded.json`.
- Правка плейлиста уходит на сервер через 2 секунды, при выходе из аккаунта и при закрытии плеера.
- Плейлисты аккаунта лежат в одном документе `users/{uid}`: сжатый JSON. Своя обложка плейлиста остаётся только на том компьютере, где её поставили.
- Синхронизация идёт при запуске, раз в минуту, по кнопке ⟳ в плейлисте или в настройках, и по `Ctrl+R`.
- Слияние трёхстороннее, по имени плейлиста. Не трогал плейлист тут — берётся облачная версия (в том числе удаление). Не трогал там — берётся своя. Поменял в обоих местах — побеждает своя.
- Без подтверждённой почты Firestore ничего не отдаст и не примет.
