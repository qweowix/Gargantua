//! Скачивание в mp3: yt-dlp тянет лучший звук, ffmpeg перегоняет в MP3 (VBR V0 ≈ 245 кбит/с)
//! и сразу вшивает теги — название, артиста и обложку. Spotify/Яндекс/SC-превью качаются
//! с YT Music, как и играются.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::model::Track;
use crate::util::{command, find_bin};

/// Папка по умолчанию: ~/Music/BlackHole (на винде — "Музыка\BlackHole")
pub fn default_dir() -> PathBuf {
    dirs::audio_dir().or_else(|| dirs::home_dir().map(|h| h.join("Music"))).unwrap_or_default().join("BlackHole")
}

/// "Артист - Название.mp3" без символов, которые запрещены в именах файлов на винде/линуксе
pub fn file_name(t: &Track) -> String {
    let raw = if t.artist.is_empty() { t.title.clone() } else { format!("{} - {}", t.artist, t.title) };
    let clean: String = raw.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean: String = clean.trim().trim_end_matches('.').chars().take(150).collect();
    format!("{}.mp3", if clean.is_empty() { "track".into() } else { clean })
}

pub fn download(t: &Track, url: &str, dir: &Path) -> Result<PathBuf> {
    let ffmpeg = find_bin("ffmpeg").context("для mp3 нужен ffmpeg")?;
    std::fs::create_dir_all(dir)?;
    let tmp = dirs::cache_dir().context("нет папки кеша")?.join("BlackHole").join("dl");
    std::fs::create_dir_all(&tmp)?;
    let stem = format!("{}-{}", t.src.tag(), t.id.replace(['/', '\\', ':'], "_"));

    // 1. исходный звук как есть (opus/m4a), без перекодирования в yt-dlp
    let dst = tmp.join(format!("{stem}.%(ext)s"));
    let out = crate::util::ytdlp_run(|c| {
        c.args(["-f", "bestaudio/best", "--no-playlist", "--no-warnings", "--no-progress", "--print", "after_move:filepath", "-o"]).arg(&dst).arg(url);
    })?;
    if !out.status.success() {
        bail!("{}", crate::util::ytdlp_error(&out.stderr));
    }
    let src = PathBuf::from(String::from_utf8_lossy(&out.stdout).lines().last().context("yt-dlp не сказал, куда скачал")?.trim());

    // 2. обложка — не вышла, так и ладно, mp3 будет без неё
    let cover = crate::covers::jpeg(&t.cover).ok().and_then(|b| {
        let p = tmp.join(format!("{stem}.jpg"));
        std::fs::write(&p, b).ok().map(|_| p)
    });

    // 3. mp3 + теги. Пишем во временный файл и переименовываем: недокачанный mp3 не появится в папке
    let dst = dir.join(file_name(t));
    let part = dir.join(format!(".{}.part.mp3", stem));
    let mut cmd = command(&ffmpeg);
    cmd.args(["-y", "-loglevel", "error", "-i"]).arg(&src);
    if let Some(c) = &cover {
        cmd.arg("-i").arg(c).args(["-map", "0:a", "-map", "1:v", "-c:v", "copy", "-disposition:v", "attached_pic"]);
        cmd.args(["-metadata:s:v", "title=Album cover", "-metadata:s:v", "comment=Cover (front)"]);
    } else {
        cmd.args(["-map", "0:a"]);
    }
    cmd.args(["-c:a", "libmp3lame", "-q:a", "0", "-id3v2_version", "3", "-map_metadata", "-1"]);
    cmd.arg("-metadata").arg(format!("title={}", t.title));
    cmd.arg("-metadata").arg(format!("artist={}", t.artist));
    cmd.arg(&part);
    let out = cmd.output()?;
    let _ = std::fs::remove_file(&src);
    if let Some(c) = cover {
        let _ = std::fs::remove_file(c);
    }
    if !out.status.success() {
        let _ = std::fs::remove_file(&part);
        bail!("ffmpeg: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("упал"));
    }
    std::fs::rename(&part, &dst)?;
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use crate::model::{Src, Track};

    fn t(artist: &str, title: &str) -> Track {
        Track { src: Src::Yt, id: "x".into(), title: title.into(), artist: artist.into(), dur: 0, cover: String::new(), url: String::new(), snip: false, artist_id: String::new() }
    }

    #[test]
    fn names() {
        assert_eq!(super::file_name(&t("AC/DC", "T.N.T.")), "AC_DC - T.N.T.mp3");
        assert_eq!(super::file_name(&t("", "What? <3")), "What_ _3.mp3");
    }

    /// Реально качает короткий трек (нужны сеть, yt-dlp, ffmpeg)
    #[test]
    #[ignore]
    fn real_download() {
        let mut tr = t("Me at the zoo", "jawed");
        tr.id = "jNQXAC9IVRw".into();
        tr.cover = "https://i.ytimg.com/vi/jNQXAC9IVRw/mqdefault.jpg".into();
        let dir = std::env::temp_dir().join("bh-dl-test");
        let p = super::download(&tr, "https://www.youtube.com/watch?v=jNQXAC9IVRw", &dir).unwrap();
        let probe = std::process::Command::new("ffprobe").args(["-v", "error", "-show_entries", "format_tags=title,artist:stream=codec_name"]).arg(&p).output().unwrap();
        let info = String::from_utf8_lossy(&probe.stdout);
        println!("{}\n{info}", p.display());
        assert!(info.contains("mp3") && info.contains("mjpeg") && info.contains("TAG:artist=Me at the zoo"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
