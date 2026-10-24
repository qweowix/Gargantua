//! Офлайн-кэш: дослушанный до 30 с трек тихо докачивается на диск по той же прямой ссылке, по которой
//! его играет mpv (yt-dlp второй раз не зовём — это просто HTTP). Повторно он играет с диска мгновенно
//! и без интернета. Старые файлы вытесняются, когда кэш перерастает лимит.

use std::io::{Read, Write};
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
    if f.metadata().ok()?.len() == 0 {
        return None;
    }
    let _ = f.set_modified(SystemTime::now());
    p.to_str().map(String::from)
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
                    eprintln!("офлайн-кэш: {e:#}");
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

/// Кусками по 4 МБ через Range: googlevideo режет скорость до реального времени, если тянуть целиком
fn download(key: &str, url: &str) -> Result<()> {
    const CHUNK: u64 = 4 << 20;
    std::fs::create_dir_all(dir())?;
    let dst = path(key);
    let part = dst.with_extension("part");
    let mut out = std::fs::File::create(&part)?;
    let mut at = 0u64;
    let total = loop {
        let r = agent().get(url).set("Range", &format!("bytes={}-{}", at, at + CHUNK - 1)).call()?;
        // "bytes 0-4194303/5012345" — сколько всего
        let total = r.header("Content-Range").and_then(|c| c.rsplit('/').next()).and_then(|t| t.parse::<u64>().ok());
        let whole = r.status() == 200; // сервер не умеет Range — отдал весь файл разом
        let mut buf = Vec::new();
        r.into_reader().take(if whole { 64 << 20 } else { CHUNK + 1 }).read_to_end(&mut buf)?;
        if whole {
            out.write_all(&buf)?;
            at = buf.len() as u64;
            break at;
        }
        if buf.is_empty() {
            break total.unwrap_or(at);
        }
        out.write_all(&buf)?;
        at += buf.len() as u64;
        match total {
            Some(t) if at < t => continue,
            _ => break total.unwrap_or(at),
        }
    };
    drop(out);
    if at == 0 || at < total {
        let _ = std::fs::remove_file(&part);
        bail!("скачалось {at} из {total} байт");
    }
    std::fs::rename(part, dst)?;
    Ok(())
}

/// Самые давно игравшие — вон, пока кэш больше лимита
fn evict(limit: u64) {
    let Ok(rd) = std::fs::read_dir(dir()) else { return };
    let mut files: Vec<(SystemTime, u64, PathBuf)> =
        rd.flatten().filter_map(|e| e.metadata().ok().map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), e.path()))).collect();
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
