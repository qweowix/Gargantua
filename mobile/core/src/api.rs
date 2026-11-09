//! Методы ядра для Kotlin. Логика — та же, что в main.rs плеера на ПК (поиск, импорт, радио, тексты)

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::model::{Playlist, Src, Track};
use crate::stats::{self, Stats};
use crate::{apple, cloud, connect, home, lyrics, qr, soundcloud, spicy, spotify, yandex, ytmusic};

pub fn call(method: &str, args: &str) -> String {
    let a: Value = serde_json::from_str(args).unwrap_or(Value::Null);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(method, &a)));
    match r {
        Ok(Ok(v)) => json!({ "ok": v }),
        Ok(Err(e)) => json!({ "err": format!("{e:#}") }),
        Err(_) => json!({ "err": "panic" }),
    }
    .to_string()
}

fn s<'a>(a: &'a Value, k: &str) -> &'a str {
    a[k].as_str().unwrap_or("")
}

fn track(a: &Value) -> Result<Track> {
    serde_json::from_value(a["track"].clone()).context("нет трека")
}

fn run(m: &str, a: &Value) -> Result<Value> {
    Ok(match m {
        "init" => {
            // dirs на Android ничего не знает: папки приложения подсовываем через XDG, как на Linux
            std::env::set_var("HOME", s(a, "files"));
            std::env::set_var("XDG_CONFIG_HOME", s(a, "files"));
            std::env::set_var("XDG_DATA_HOME", s(a, "files"));
            std::env::set_var("XDG_CACHE_HOME", s(a, "cache"));
            let on = cloud::cfg().is_some();
            crate::model::CLOUD.store(on, Ordering::Relaxed);
            json!({ "cloud": on })
        }

        // ── поиск и каталог ──
        "search" => {
            let q = s(a, "q");
            let r = match s(a, "src") {
                "sc" => soundcloud::search(q, 20),
                "ya" => yandex::search(q, 20),
                _ => ytmusic::search(q, 20),
            }?;
            json!(r)
        }
        "cards" => {
            let q = s(a, "q").to_string();
            let q2 = q.clone();
            let albums = std::thread::spawn(move || ytmusic::search_cards(&q2, ytmusic::ALBUMS, 12));
            let artists = ytmusic::search_cards(&q, ytmusic::ARTISTS, 8).unwrap_or_default();
            let albums = albums.join().ok().and_then(Result::ok).unwrap_or_default();
            json!({ "artists": artists, "albums": albums })
        }
        "artist" => {
            let id = match s(a, "id") {
                "" => ytmusic::find_artist(s(a, "name"))?,
                id => id.to_string(),
            };
            let r = ytmusic::artist(&id)?;
            json!({
                "id": id, "name": r.name, "desc": r.desc, "photo": r.photo, "stats": r.stats, "top": r.top,
                "songs_id": r.songs_id, "albums": r.albums, "singles": r.singles, "similar": r.similar,
            })
        }
        "album" => {
            let (title, sub, cover, tracks) = ytmusic::album(s(a, "id"))?;
            json!({ "title": title, "sub": sub, "cover": cover, "tracks": tracks })
        }
        "all_songs" => json!(ytmusic::all_songs(s(a, "id"), s(a, "artist"))?),
        "radio" => json!(radio_for(&track(a)?)?),
        "resolve" => json!(resolve(&track(a)?)?),
        "sc_stream" => json!(crate::soundcloud::stream(s(a, "url"))?),
        "import" => import(s(a, "url"))?,
        // QR плейлиста (формат как на ПК): картинки PNG в кэше, пути — по порядку частей
        "qr_encode" => {
            let pl: crate::model::Playlist = serde_json::from_value(a["playlist"].clone())?;
            let dir = dirs::cache_dir().context("нет кэша")?.join("qr");
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir)?;
            let mut paths = vec![];
            for (i, c) in qr::codes(&pl).iter().enumerate() {
                let p = dir.join(format!("{}-{}.png", pl.name.chars().filter(|c| c.is_alphanumeric()).take(30).collect::<String>(), i + 1));
                qr::render(c, 8)?.save(&p)?;
                paths.push(p.to_string_lossy().to_string());
            }
            json!(paths)
        }
        // картинки (из галереи или с камеры) → плейлист. Ссылочный QR подтягивает плейлист по ссылке
        // и накатывает правки отправителя — как на ПК
        "qr_decode" => {
            let paths: Vec<std::path::PathBuf> = serde_json::from_value(a["paths"].clone())?;
            match qr::decode_files(&paths)? {
                qr::Shared::Full { name, tracks } => json!({ "name": name, "tracks": tracks }),
                qr::Shared::Cloud { .. } => anyhow::bail!("QR повреждён"),
                qr::Shared::Linked { name, url, removed, added } => {
                    let v = import(&url)?;
                    let mut tracks: Vec<Track> = serde_json::from_value(v["tracks"].clone())?;
                    tracks.retain(|t| !removed.contains(&t.key()));
                    tracks.extend(added);
                    json!({ "name": name, "tracks": tracks, "url": url })
                }
            }
        }
        "lyrics" => lyrics_for(&track(a)?, s(a, "spicy")),
        // перевод строк на язык интерфейса: null — текст уже на этом языке
        "translate" => {
            let lines: Vec<String> = serde_json::from_value(a["lines"].clone())?;
            json!(lyrics::translate(&lines, s(a, "lang"))?)
        }

        // ── статистика и главная ──
        "listen" => {
            let t = track(a)?;
            let mut st = stats_lock();
            if a["counted"].as_bool() == Some(true) {
                st.play(&t);
            }
            st.add(&t, a["secs"].as_f64().unwrap_or(0.0), stats::today());
            // на диск — не чаще раза в минуту (и при сворачивании, см. stats_save)
            static SAVED: Mutex<Option<Instant>> = Mutex::new(None);
            let mut saved = SAVED.lock().unwrap();
            if saved.is_none_or(|t| t.elapsed() > Duration::from_secs(60)) {
                st.save();
                *saved = Some(Instant::now());
            }
            Value::Null
        }
        "stats_save" => {
            stats_lock().save();
            Value::Null
        }
        "stats" => stats_json(&stats_lock().with(&others_lock())),
        // своя доля статистики аккаунта — в облако, доли других устройств — к себе (как на ПК раз в минуту)
        // ── Connect: одна сессия на все устройства (как на ПК, см. connect.rs) ──
        "connect_start" => {
            let dev = stats_lock().device.clone();
            connect::start(&dev, s(a, "name"), "phone", || {});
            Value::Null
        }
        // длинный опрос: ждёт изменений после версии since (или ms), отдаёт разобранный узел и свежие команды
        "connect_wait" => {
            let (ver, tree) = connect::wait(a["since"].as_u64().unwrap_or(0), std::time::Duration::from_millis(a["ms"].as_u64().unwrap_or(20_000)));
            json!({
                "ver": ver, "now": connect::now(), "me": connect::me(),
                "host": connect::active_host(&tree), "devices": connect::devices(&tree),
                "state": connect::state(&tree),
                "cmds": connect::take_cmds(&tree).into_iter().map(|(op, a)| json!({"op": op, "a": a})).collect::<Vec<_>>(),
            })
        }
        "connect_publish" => {
            connect::publish_bg(serde_json::from_value(a["state"].clone())?, a["queue"].as_bool().unwrap_or(false));
            Value::Null
        }
        "connect_send" => {
            connect::send_bg(s(a, "to"), s(a, "op"), a["a"].clone());
            Value::Null
        }
        "connect_bye" => {
            connect::bye(serde_json::from_value(a["state"].clone()).ok());
            Value::Null
        }
        // куки YouTube, которые перенёс комп (Настройки → Куки YouTube → На телефон). Пусто — не переносили
        "yt_cookies" => json!(cloud::get_yt_cookies(&cfg()?)?),
        "stats_sync" => {
            let own = stats_lock().clone();
            *others_lock() = cloud::sync_stats(&cfg()?, &own)?;
            Value::Null
        }
        "home" => {
            let st = stats_lock().with(&others_lock());
            let cached = home::load();
            let fresh = cached.day == stats::today() && !cached.mixes.is_empty() && cached.seeds == home::seed_key(&st);
            if st.top_tracks(1).is_empty() || (fresh && a["force"].as_bool() != Some(true)) {
                json!(cached)
            } else {
                let n = |i: usize| a["names"][i].as_str().unwrap_or("").to_string();
                let h = home::build(&st, radio_for, |name| ytmusic::find_artist(name).and_then(|id| ytmusic::artist(&id)).map(|a| a.similar).unwrap_or_default(), (n(0), n(1), n(2)));
                if !h.mixes.is_empty() {
                    home::save(&h);
                }
                json!(h)
            }
        }

        // ── аккаунт ──
        "account" => {
            let acc = cloud::account();
            json!({ "configured": cloud::cfg().is_some(), "email": acc.email, "signed_in": acc.signed_in(), "verified": acc.verified })
        }
        "sign_in" => json!(cloud::sign_in(&cfg()?, s(a, "lang"), s(a, "email").trim(), s(a, "password"))?),
        "sign_up" => json!(cloud::sign_up(&cfg()?, s(a, "lang"), s(a, "email").trim(), s(a, "password"))?),
        "send_verify" => json!(cloud::send_verify(&cfg()?, s(a, "lang"))?),
        "send_reset" => json!(cloud::send_reset(&cfg()?, s(a, "lang"), s(a, "email").trim())?),
        "check_verified" => json!(cloud::check_verified(&cfg()?, s(a, "lang"))?),
        "sign_out" => {
            cloud::sign_out();
            Value::Null
        }
        // база слияния в Kotlin не ходит (u64 не влезает в Long): держим её тут до cloud_commit
        "cloud_load" => {
            let r = cloud::load(&cfg()?)?;
            *pending() = Some(r.base);
            json!(r.playlists)
        }
        "cloud_sync" => {
            let local: Vec<Playlist> = serde_json::from_value(a["playlists"].clone())?;
            let r = cloud::sync(&cfg()?, &local)?;
            *pending() = Some(r.base);
            json!(r.playlists)
        }
        "cloud_commit" => {
            if let Some(base) = pending().take() {
                cloud::update(|acc| acc.base = base);
            }
            Value::Null
        }
        "cloud_dirty" => {
            let local: Vec<Playlist> = serde_json::from_value(a["playlists"].clone())?;
            json!(cloud::dirty(&local))
        }
        _ => bail!("нет метода {m}"),
    })
}

fn cfg() -> Result<cloud::Cfg> {
    cloud::cfg().context("NOT_CONFIGURED")
}

fn pending() -> std::sync::MutexGuard<'static, Option<HashMap<String, u64>>> {
    static P: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);
    P.lock().unwrap()
}

fn others_lock() -> std::sync::MutexGuard<'static, Stats> {
    static S: OnceLock<Mutex<Stats>> = OnceLock::new();
    S.get_or_init(Default::default).lock().unwrap()
}

fn stats_lock() -> std::sync::MutexGuard<'static, Stats> {
    static S: OnceLock<Mutex<Stats>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Stats::load())).lock().unwrap()
}

/// Что дать NewPipe: YouTube и полные SoundCloud играют сами, остальное (Spotify, превью SC, Яндекс)
/// ищем на YT Music — как resolve() в плеере на ПК
fn resolve(t: &Track) -> Result<String> {
    match t.src {
        Src::Yt => Ok(if t.url.is_empty() { format!("https://music.youtube.com/watch?v={}", t.id) } else { t.url.clone() }),
        Src::Sc if !t.snip => Ok(t.url.clone()),
        _ => ytmusic::best_match(&format!("{} {}", t.artist, t.title)),
    }
}

fn radio_for(t: &Track) -> Result<Vec<Track>> {
    let vid = match t.src {
        Src::Yt => t.id.clone(),
        _ => ytmusic::best_match(&format!("{} {}", t.artist, t.title)).and_then(|u| ytmusic::video_id(&u).context("нет videoId"))?,
    };
    ytmusic::radio(&vid)
}

fn source_of(url: &str) -> Option<Src> {
    [("spotify", Src::Sp), ("soundcloud", Src::Sc), ("youtu", Src::Yt), ("yandex", Src::Ya)].into_iter().find(|(k, _)| url.contains(k)).map(|(_, s)| s)
}

/// Плейлист по ссылке. Spotify и Apple Music сами не играются — каждый трек сразу ищем на YT Music
/// (в 6 потоков); сомнительные совпадения на телефоне берём без вопросов, ненайденные считаем
fn import(url: &str) -> Result<Value> {
    let url = url.trim();
    let apple = apple::is_apple(url);
    let src = source_of(url);
    if apple || src == Some(Src::Sp) {
        let (name, wants) = if apple { apple::import(url)? } else { spotify::import(url)? };
        let next = AtomicUsize::new(0);
        let found: Mutex<Vec<Option<Track>>> = Mutex::new(vec![None; wants.len()]);
        std::thread::scope(|sc| {
            for _ in 0..6 {
                sc.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(w) = wants.get(i) else { break };
                    let r = ytmusic::find_same(w).or_else(|_| ytmusic::find_same(w));
                    found.lock().unwrap()[i] = r.ok().flatten().map(|(t, _)| t);
                });
            }
        });
        let found = found.into_inner().unwrap();
        let missing = found.iter().filter(|f| f.is_none()).count();
        let tracks: Vec<Track> = found.into_iter().flatten().collect();
        return Ok(json!({ "name": name, "tracks": tracks, "missing": missing, "url": url }));
    }
    let (name, tracks) = match src.context("неизвестная ссылка")? {
        Src::Yt => ytmusic::import(url)?,
        Src::Sc => soundcloud::import(url)?,
        Src::Ya => yandex::import(url)?,
        _ => bail!("неизвестная ссылка"),
    };
    Ok(json!({ "name": name, "tracks": tracks, "missing": 0, "url": url }))
}

/// Все источники параллельно, ждём не дольше 10 с, выбираем лучший — как decide_lyrics на ПК
fn lyrics_for(t: &Track, spicy_key: &str) -> Value {
    let (tx, rx) = mpsc::channel::<Option<lyrics::Found>>();
    let mut n = 2;
    {
        let (tx, t) = (tx.clone(), t.clone());
        std::thread::spawn(move || {
            let lrclib = lyrics::Credit { provider: "LRCLIB".into(), ..Default::default() };
            let r = lyrics::fetch(&t.artist, &t.title, t.dur).map(|l| l.map(|lyr| lyrics::Found { lyr, credit: lrclib }));
            let _ = tx.send(r.ok().flatten());
        });
    }
    {
        let q = match t.src {
            Src::Yt => lyrics::Query::Video(t.id.clone()),
            Src::Sp => lyrics::Query::Url(t.url.clone()),
            _ => lyrics::Query::Meta { artist: t.artist.clone(), title: t.title.clone(), dur: t.dur },
        };
        let tx = tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(lyrics::fetch_liriqo(&q).ok().flatten());
        });
    }
    if let Some(k) = spicy::key(spicy_key) {
        n += 1;
        let (tx, t) = (tx.clone(), t.clone());
        std::thread::spawn(move || {
            let _ = tx.send(spicy::fetch(&t, &k).ok().flatten());
        });
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut all = vec![];
    for _ in 0..n {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(f) => all.extend(f),
            Err(_) => break,
        }
    }
    let Some(f) = lyrics::choose(&all) else { return json!({ "kind": "none" }) };
    let c = &f.credit;
    let credit = if c.via.is_empty() { c.provider.clone() } else { format!("{} · {}", c.provider, c.via) };
    match f.lyr {
        // строки как на ПК (концы, точки в проигрышах, дуэт справа); слова — отрезками [a, b) текста
        // в UTF-16, как считает Kotlin: слоги одного слова идут без пробела, их нельзя просто склеивать
        lyrics::Lyrics::Synced(lines) => json!({
            "kind": "synced",
            "credit": credit,
            "lines": lyrics::display_lines(&lines).iter().map(|l| {
                let u16 = |byte: usize| l.text[..byte].encode_utf16().count();
                let mut cursor = 0;
                let words: Vec<Value> = l.words.iter().filter_map(|w| {
                    let t = w.text.trim();
                    let at = cursor + l.text[cursor..].find(t)?;
                    cursor = at + t.len();
                    Some(json!({ "s": w.start, "e": w.end.max(w.start + 0.01), "a": u16(at), "b": u16(cursor) }))
                }).collect();
                json!({ "start": l.start, "end": l.end, "fill": l.fill_end, "text": l.text, "bg": l.bg, "right": l.right, "dots": l.dots, "words": words })
            }).collect::<Vec<_>>(),
        }),
        lyrics::Lyrics::Plain(lines) => json!({ "kind": "plain", "credit": credit, "plain": lines }),
    }
}

/// Сводка для экрана статистики: время, год клетками (уровни 0–4), топы
fn stats_json(s: &Stats) -> Value {
    let today = stats::today();
    let start = today - stats::weekday(today) - 52 * 7;
    let heat: Vec<i32> = (start..=today).map(|d| stats::level(s.days.get(&d).copied().unwrap_or(0))).collect();
    let top: Vec<Value> = s.top_tracks(10).into_iter().map(|t| json!({ "track": t.track, "plays": t.plays, "ms": t.ms })).collect();
    let artists: Vec<Value> = s.top_artists(10).into_iter().map(|(n, ms)| json!({ "name": n, "ms": ms })).collect();
    json!({
        "today": s.last(today, 1), "week": s.last(today, 7), "month": s.last(today, 30), "total": s.days.values().sum::<u64>(),
        "heat": heat, "heat_start_weekday": stats::weekday(start), "top": top, "artists": artists,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_method_is_an_error_not_a_crash() {
        let v: Value = serde_json::from_str(&call("nope", "{}")).unwrap();
        assert!(v["err"].as_str().unwrap().contains("nope"));
    }

    #[test]
    fn resolve_keeps_youtube_and_full_soundcloud() {
        let yt = Track { src: Src::Yt, id: "abc".into(), ..Default::default() };
        assert_eq!(resolve(&yt).unwrap(), "https://music.youtube.com/watch?v=abc");
        let sc = Track { src: Src::Sc, url: "https://soundcloud.com/a/b".into(), ..Default::default() };
        assert_eq!(resolve(&sc).unwrap(), "https://soundcloud.com/a/b");
    }

    #[test]
    fn bad_track_json_is_reported() {
        let v: Value = serde_json::from_str(&call("radio", "{}")).unwrap();
        assert!(v["err"].is_string());
    }
}
