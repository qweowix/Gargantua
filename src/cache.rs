//! Офлайн-кэш: дослушанный до 30 с трек тихо докачивается на диск по той же прямой ссылке, по которой
//! его играет mpv (yt-dlp второй раз не зовём — это просто HTTP). Повторно он играет с диска мгновенно
//! и без интернета. Старые файлы вытесняются, когда кэш перерастает лимит.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::{bail, Result};

pub fn dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("BlackHole").join("audio")
}

fn path(key: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    dir().join(format!("{:016x}.audio", h.finish()))
}

/// Трек уже на диске — путь для mpv. Время файла обновляем: вытесняются давно не игравшие
pub fn get(key: &str) -> Option<String> {
    let p = path(key);
    let f = std::fs::File::options().append(true).open(&p).ok()?;
    if f.metadata().ok()?.len() == 0 || is_playlist(&p) {
        forget(key);
        return None;
    }
    let _ = f.set_modified(SystemTime::now());
    p.to_str().map(String::from)
}

/// Старые версии клали в кэш HLS-плейлист вместо звука
fn is_playlist(p: &std::path::Path) -> bool {
    let mut head = [0u8; 7];
    std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"#EXTM3U"
}

pub fn has(key: &str) -> bool {
    path(key).is_file()
}

/// Файл из кэша не сыграл — выкидываем, в следующий раз пойдёт из сети
pub fn forget(key: &str) {
    let _ = std::fs::remove_file(path(key));
}

pub fn size() -> u64 {
    std::fs::read_dir(dir()).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0)
}

pub fn clear() {
    let _ = std::fs::remove_dir_all(dir());
}

/// Своя очередь: один фоновый поток с nice 19 качает по одному треку
pub fn fetch(key: String, url: String, limit: u64, done: fn(u64)) {
    static TX: OnceLock<mpsc::Sender<(String, String, u64)>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<(String, String, u64)>();
        std::thread::spawn(move || {
            #[cfg(target_os = "linux")]
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 19)
            };
            for (key, url, limit) in rx {
                if path(&key).exists() {
                    continue;
                }
                if let Err(e) = download(&key, &url) {
                    crate::logln!("офлайн-кэш: {e:#}");
                }
                evict(limit);
                done(size());
            }
        });
        tx
    });
    let _ = tx.send((key, url, limit));
}

fn agent() -> &'static ureq::Agent {
    static A: OnceLock<ureq::Agent> = OnceLock::new();
    A.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
            .build()
    })
}

/// Трек целиком в память, потом одним махом в файл: при ошибке на диске не остаётся пустых .part
fn download(key: &str, url: &str) -> Result<()> {
    let body = grab(url)?;
    // некоторые треки (SoundCloud, часть YouTube) отдаются по HLS: по ссылке лежит не звук, а список
    // кусков. Раньше в кэш ложился сам список на 16 КБ, и с диска трек не играл
    let body = if body.starts_with(b"#EXTM3U") { hls(url, &body, 0)? } else { body };
    std::fs::create_dir_all(dir())?;
    let dst = path(key);
    let part = dst.with_extension("part");
    std::fs::write(&part, &body)?;
    std::fs::rename(part, dst)?;
    Ok(())
}

/// Кусками по 4 МБ через Range: googlevideo режет скорость до реального времени, если тянуть целиком
fn grab(url: &str) -> Result<Vec<u8>> {
    const CHUNK: u64 = 4 << 20;
    let mut out = Vec::new();
    let total = loop {
        let at = out.len() as u64;
        let r = agent().get(url).set("Range", &format!("bytes={}-{}", at, at + CHUNK - 1)).call()?;
        // "bytes 0-4194303/5012345" — сколько всего
        let total = r.header("Content-Range").and_then(|c| c.rsplit('/').next()).and_then(|t| t.parse::<u64>().ok());
        let whole = r.status() == 200; // сервер не умеет Range — отдал весь файл разом
        let mut buf = Vec::new();
        r.into_reader().take(if whole { 64 << 20 } else { CHUNK + 1 }).read_to_end(&mut buf)?;
        out.extend_from_slice(&buf);
        let at = out.len() as u64;
        if whole || buf.is_empty() {
            break if whole { at } else { total.unwrap_or(at) };
        }
        match total {
            Some(t) if at < t => continue,
            _ => break total.unwrap_or(at),
        }
    };
    let at = out.len() as u64;
    if at == 0 || at < total {
        bail!("скачалось {at} из {total} байт");
    }
    Ok(out)
}

/// HLS: склеиваем куски по порядку (с init-сегментом, если есть) — mpv такой файл играет как обычный
fn hls(url: &str, list: &[u8], depth: u8) -> Result<Vec<u8>> {
    let text = String::from_utf8_lossy(list);
    let uris = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    // мастер-плейлист: внутри не куски, а варианты качества — берём первый
    if text.contains("#EXT-X-STREAM-INF") {
        let Some(v) = uris.clone().next().filter(|_| depth == 0) else { bail!("HLS: нет варианта") };
        let v = join(url, v);
        return hls(&v, &grab(&v)?, 1);
    }
    if text.lines().any(|l| l.starts_with("#EXT-X-KEY") && !l.contains("METHOD=NONE")) {
        bail!("HLS зашифрован, не кэшируем");
    }
    let mut out = Vec::new();
    if let Some(m) = text.lines().find_map(|l| l.strip_prefix("#EXT-X-MAP:")) {
        let Some(u) = m.split("URI=\"").nth(1).and_then(|r| r.split('"').next()) else { bail!("HLS: кривой EXT-X-MAP") };
        out.extend(grab(&join(url, u))?);
    }
    for u in uris {
        out.extend(grab(&join(url, u))?);
    }
    if out.is_empty() {
        bail!("HLS: пустой список");
    }
    Ok(out)
}

/// Ссылка из плейлиста относительно самого плейлиста
fn join(base: &str, u: &str) -> String {
    if u.starts_with("http://") || u.starts_with("https://") {
        return u.to_string();
    }
    let base = base.split('?').next().unwrap_or(base);
    if u.starts_with('/') {
        let host_end = base.find("://").map_or(0, |i| i + 3);
        let root = base[host_end..].find('/').map_or(base.len(), |i| host_end + i);
        return format!("{}{u}", &base[..root]);
    }
    format!("{}{u}", &base[..base.rfind('/').map_or(0, |i| i + 1)])
}

/// Самые давно игравшие — вон, пока кэш больше лимита
fn evict(limit: u64) {
    let Ok(rd) = std::fs::read_dir(dir()) else { return };
    // .part здесь всегда мусор от упавших загрузок: качает один поток, и сейчас он тут
    let (parts, rd): (Vec<_>, Vec<_>) = rd.flatten().partition(|e| e.path().extension().is_some_and(|x| x == "part"));
    parts.iter().for_each(|e| drop(std::fs::remove_file(e.path())));
    let mut files: Vec<(SystemTime, u64, PathBuf)> =
        rd.into_iter().filter_map(|e| e.metadata().ok().map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), e.path()))).collect();
    files.sort();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    for (_, len, p) in files {
        if total <= limit {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hls_links() {
        let b = "https://cf.sndcdn.com/a/b/playlist.m3u8?sig=1";
        assert_eq!(super::join(b, "seg0.mp3"), "https://cf.sndcdn.com/a/b/seg0.mp3");
        assert_eq!(super::join(b, "/x/seg1.mp3"), "https://cf.sndcdn.com/x/seg1.mp3");
        assert_eq!(super::join(b, "https://o.com/s.ts"), "https://o.com/s.ts");
    }

    #[test]
    fn old_playlist_entry_is_dropped() {
        let k = "test:playlist-entry";
        std::fs::create_dir_all(super::dir()).unwrap();
        std::fs::write(super::path(k), "#EXTM3U\n#EXT-X-VERSION:7\n").unwrap();
        assert!(super::get(k).is_none());
        assert!(!super::has(k));
    }

    /// Живой YouTube: прямая ссылка от yt-dlp докачивается кусками и играется mpv с диска.
    /// Один запрос к yt-dlp — не гонять в цикле
    #[test]
    #[ignore]
    fn downloads_stream() {
        let url = crate::ytmusic::stream_url("https://music.youtube.com/watch?v=x4c_wI6kQyE").unwrap();
        super::download("test:cache", &url).unwrap();
        let p = super::get("test:cache").unwrap();
        let len = std::fs::metadata(&p).unwrap().len();
        println!("{p}: {} КБ", len / 1024);
        assert!(len > 500_000);
        let out = std::process::Command::new("mpv").args(["--no-config", "--ao=null", "--length=1", &p]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        super::forget("test:cache");
    }
}
