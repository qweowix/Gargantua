//! Локальные файлы: сканируем папки с музыкой, теги и длительность — через lofty (без ffprobe на каждый файл).
//! Играет их mpv напрямую по пути.

use std::path::{Path, PathBuf};

use lofty::prelude::*;

use crate::model::{Src, Track};

/// Обложка локального трека: "lc:/путь/к/файлу" — картинку достаём из тегов, когда её попросят
pub const COVER: &str = "lc:";

const EXTS: [&str; 10] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav", "wv", "ape", "aiff"];

pub fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Все треки в папках (с подпапками, но не глубже 6 уровней), отсортированы по артисту и названию
pub fn scan(dirs: &[PathBuf]) -> Vec<Track> {
    let mut files = vec![];
    for d in dirs {
        walk(d, 0, &mut files);
    }
    files.sort();
    files.dedup();
    let mut out: Vec<Track> = files.iter().filter_map(|p| read(p)).collect();
    out.sort_by_cached_key(|t| (t.artist.to_lowercase(), t.title.to_lowercase()));
    out
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && depth < 6 && !e.file_name().to_string_lossy().starts_with('.') {
            walk(&p, depth + 1, out);
        } else if ft.is_file() && is_audio(&p) {
            out.push(p);
        }
    }
}

/// Трек по файлу: теги, а если их нет — имя файла "Артист - Название"
pub fn read(p: &Path) -> Option<Track> {
    let path = p.to_str()?.to_string();
    let stem = p.file_stem()?.to_string_lossy().to_string();
    let (mut title, mut artist) = match stem.split_once(" - ") {
        Some((a, t)) => (t.trim().to_string(), a.trim().to_string()),
        None => (stem.clone(), String::new()),
    };
    let (mut dur, mut cover) = (0, String::new());
    // с read_cover_art(false) lofty выкидывает картинки целиком и picture_count() всегда 0 — обложка
    // у своих файлов не показывалась никогда. Читаем с картинками (сканирование и так в фоне с nice 19)
    if let Ok(f) = lofty::probe::Probe::open(p).and_then(|pr| pr.read()) {
        dur = f.properties().duration().as_secs();
        if let Some(tag) = f.primary_tag().or_else(|| f.first_tag()) {
            if let Some(t) = tag.title().filter(|t| !t.trim().is_empty()) {
                title = t.trim().to_string();
            }
            if let Some(a) = tag.artist().filter(|a| !a.trim().is_empty()) {
                artist = a.trim().to_string();
            }
            if tag.picture_count() > 0 {
                cover = format!("{COVER}{path}");
            }
        }
    }
    Some(Track { src: Src::Lc, id: path.clone(), title, artist, dur, cover, url: path, snip: false, artist_id: String::new() })
}

/// Картинка из тегов файла (первая, обычно это обложка)
pub fn picture(path: &str) -> Option<Vec<u8>> {
    let f = lofty::read_from_path(path).ok()?;
    let tag = f.primary_tag().or_else(|| f.first_tag())?;
    let pic = tag.pictures().iter().find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront).or_else(|| tag.pictures().first())?;
    Some(pic.data().to_vec())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_tags_and_name() {
        let dir = std::env::temp_dir().join(format!("bh-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        // тегов нет — артист и название из имени файла; длительность читается из самого звука
        let ok = std::process::Command::new("ffmpeg")
            .args(["-loglevel", "error", "-f", "lavfi", "-i", "sine=d=2", "-y"])
            .arg(dir.join("sub/TV Girl - Lovers Rock.mp3"))
            .status()
            .is_ok_and(|s| s.success());
        std::fs::write(dir.join("notes.txt"), "не музыка").unwrap();
        if !ok {
            eprintln!("нет ffmpeg — пропускаю");
            return;
        }
        let t = super::scan(&[dir.clone()]);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].artist.as_str(), t[0].title.as_str(), t[0].dur), ("TV Girl", "Lovers Rock", 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Обложка в тегах mp3 должна находиться: в файле она есть, а плеер её не показывал
    #[test]
    fn finds_embedded_cover() {
        let dir = std::env::temp_dir().join(format!("bh-cover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.mp3");
        let ok = std::process::Command::new("ffmpeg")
            .args(["-loglevel", "error", "-f", "lavfi", "-i", "sine=d=1", "-f", "lavfi", "-i", "color=red:s=64x64:d=1", "-frames:v", "1"])
            .args(["-map", "0", "-map", "1", "-c:v", "mjpeg", "-disposition:v", "attached_pic", "-id3v2_version", "3", "-y"])
            .arg(&f)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            eprintln!("нет ffmpeg — пропускаю");
            return;
        }
        let t = super::read(&f).unwrap();
        assert!(!t.cover.is_empty(), "обложку не увидели");
        assert!(super::picture(f.to_str().unwrap()).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
