use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Src {
    #[default]
    Yt,
    Sc,
    Sp,
    Ya,
    /// файл на диске: id и url — путь
    Lc,
}

impl Src {
    pub fn tag(self) -> &'static str {
        match self {
            Src::Yt => "yt",
            Src::Sc => "sc",
            Src::Sp => "sp",
            Src::Ya => "ya",
            Src::Lc => "lc",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Track {
    pub src: Src,
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub dur: u64, // секунды
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub url: String,
    /// SoundCloud отдаёт только 30-сек превью -> играем через YT Music
    #[serde(default)]
    pub snip: bool,
    /// id артиста в YT Music (browseId) — если известен, страница артиста открывается без поиска
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist_id: String,
}

impl Track {
    pub fn same(&self, o: &Track) -> bool {
        self.src == o.src && self.id == o.id
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.src.tag(), self.id)
    }

    /// Ссылка на трек там, откуда он (SoundCloud — на SoundCloud, даже если играет через YT Music).
    /// У файла на диске ссылки нет
    pub fn share_url(&self) -> Option<String> {
        match self.src {
            Src::Lc => None,
            _ if self.url.starts_with("http") => Some(self.url.clone()),
            Src::Yt if !self.id.is_empty() => Some(format!("https://music.youtube.com/watch?v={}", self.id)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
    /// откуда импортирован: тогда QR несёт ссылку + правки, а не весь список (1 код вместо 20)
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub desc: String,
    /// своя обложка: файл в папке настроек (covers/), пусто — логотип
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cover: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    pub keys: Vec<String>, // какие треки пришли по ссылке — чтобы понять, что потом добавили/удалили
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub volume: f32,
    pub lite: bool,
    pub karaoke: bool,
    pub lyr_anim: i32, // 0 плавно, 1 каскад, 2 фокус
    pub lyr_exit: i32, // 0 тускнеют, 2 сдув, 3 в дыру, 4 падение, 5 испарение, 6 взрыв (1 был распад)
    pub lang: String,  // "ru" / "en" / "uk" / "sk"; пусто — английский
    pub lyr_tr: bool,  // перевод текста на язык интерфейса — мелко под каждой строкой
    pub lyr_edge: f32, // строки текста растворяются у краёв колонки на столько px, 0 — выкл
    pub shuffle: bool,  // перемешка помнится между запусками, пока её не выключат
    pub fall_stay: f32,  // сколько секунд упавшие буквы лежат внизу
    pub glow: f32,       // сила свечения поющегося слова 0–2, 0 — выкл
    pub hole_always: bool,
    pub eq: Vec<f32>,     // усиление 10 полос эквалайзера, дБ
    pub speed: f32,       // 0.5–2.0
    pub keep_pitch: bool, // при смене скорости держать тональность
    pub spicy_key: String, // ключ Spicy Lyrics; пусто — источник выключен
    pub dl_dir: String,    // куда качать mp3; пусто — ~/Music/BlackHole
    pub cookies: String,   // браузер для кук YouTube (firefox, chrome...); пусто — без кук
    pub theme: i32,        // цветовая тема, см. Theme в ui/app.slint
    pub setup_done: bool,  // первичная настройка пройдена
    pub xfade: f32,        // кроссфейд, сек; 0 — выкл
    pub autoplay: bool,    // очередь кончилась — дальше похожее
    pub cover_theme: bool, // цвета интерфейса из обложки играющего трека
    // статус «слушает» в Discord. Новое имя в data.json: старое "discord" у многих осталось false после
    // сборки без Application ID, где тумблер выглядел выключенным и клик его выключал
    #[serde(rename = "discord_status")]
    pub discord: bool,
    pub music_dirs: Vec<String>, // папки с локальной музыкой (папка загрузок подхватывается всегда)
    /// свои горячие клавиши: действие → клавиша ("Ctrl+F", "Space", "M"); чего нет — по умолчанию
    pub keys: std::collections::HashMap<String, String>,
    pub offline: bool,  // офлайн-кэш прослушанных треков
    pub cache_gb: f32,  // его предел
}

impl Default for Settings {
    fn default() -> Self {
        Self { volume: 0.7, lite: false, karaoke: true, lyr_anim: 0, lyr_exit: 3, lang: String::new(), lyr_tr: false, lyr_edge: 90.0, shuffle: false, hole_always: true, fall_stay: 2.0, glow: 0.7, eq: vec![0.0; 10], speed: 1.0, keep_pitch: true, spicy_key: String::new(), dl_dir: String::new(), cookies: String::new(), theme: 0, setup_done: false, xfade: 0.0, autoplay: true, cover_theme: false, discord: true, music_dirs: vec![], keys: Default::default(), offline: true, cache_gb: 2.0 }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    /// с аккаунтом плейлисты живут на сервере и в data.json не пишутся (см. CLOUD)
    #[serde(skip_serializing_if = "in_cloud")]
    pub playlists: Vec<Playlist>,
    pub settings: Settings,
    /// свои обложки плейлистов по имени: плейлисты приходят с сервера без них (картинка — файл на этом компьютере)
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub pl_covers: std::collections::HashMap<String, String>,
    /// что играло при закрытии: при запуске появляется в плеере (на паузе) вместо «Тишины»
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<Last>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Last {
    pub track: Track,
    pub pos: f64,
    pub played: Vec<Track>,
    pub upnext: Vec<Track>,
    pub source: Vec<Track>,
}

/// Плеер собран с Firebase: плейлисты хранятся в аккаунте, а не на диске
pub static CLOUD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn in_cloud(_: &Vec<Playlist>) -> bool {
    CLOUD.load(std::sync::atomic::Ordering::Relaxed)
}

impl Store {
    pub fn dir() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| ".".into()).join("BlackHole")
    }

    fn path() -> PathBuf {
        Self::dir().join("data.json")
    }

    /// Перед загрузкой — копия в data.json.bak1 (и сдвиг bak1→bak2→bak3): если этот запуск что-то
    /// испортит, прошлые версии целы. Нечитаемый файл не затираем пустым, а откладываем в .broken
    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    fn load_from(p: &std::path::Path) -> Self {
        let Ok(b) = fs::read(p) else { return Self::default() };
        match serde_json::from_slice(&b) {
            Ok(s) => {
                let bak = |n: u32| p.with_extension(format!("json.bak{n}"));
                let _ = fs::rename(bak(2), bak(3));
                let _ = fs::rename(bak(1), bak(2));
                let _ = fs::write(bak(1), &b);
                s
            }
            Err(e) => {
                let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let broken = p.with_extension(format!("json.broken-{secs}"));
                eprintln!("data.json не читается ({e}), отложил в {}", broken.display());
                let _ = fs::rename(p, broken);
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if let Ok(data) = serde_json::to_vec_pretty(self) {
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(tmp, p);
            }
        }
    }
}

pub fn fmt_dur(s: u64) -> String {
    match s {
        0 => String::new(),
        s if s >= 3600 => format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60),
        s => format!("{}:{:02}", s / 60, s % 60),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn share_url_points_to_source() {
        let t = |src, id: &str, url: &str| super::Track { src, id: id.into(), url: url.into(), ..Default::default() };
        // превью SoundCloud играет с YT Music, но ссылка всё равно на SoundCloud
        let sc = super::Track { snip: true, ..t(super::Src::Sc, "1", "https://soundcloud.com/a/b") };
        assert_eq!(sc.share_url().unwrap(), "https://soundcloud.com/a/b");
        assert_eq!(t(super::Src::Yt, "abc", "").share_url().unwrap(), "https://music.youtube.com/watch?v=abc");
        assert_eq!(t(super::Src::Lc, "/m/a.mp3", "/m/a.mp3").share_url(), None);
        assert_eq!(t(super::Src::Sc, "1", "").share_url(), None);
    }

    use super::*;

    #[test]
    fn load_never_loses_data() {
        let dir = std::env::temp_dir().join(format!("bh-store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("data.json");
        // нормальный файл: грузится, копия в bak1, при следующем запуске сдвигается в bak2
        let pl = Playlist { name: "GOATED".into(), ..Default::default() };
        fs::write(&p, serde_json::to_vec(&Store { playlists: vec![pl], ..Default::default() }).unwrap()).unwrap();
        assert_eq!(Store::load_from(&p).playlists.len(), 1);
        assert_eq!(Store::load_from(&p).playlists.len(), 1);
        assert!(dir.join("data.json.bak1").exists() && dir.join("data.json.bak2").exists());
        // битый файл: пустой Store, но сам файл отложен, а не перезаписан пустым потом
        fs::write(&p, b"{\"playlists\": [{\"name\": null").unwrap();
        assert!(Store::load_from(&p).playlists.is_empty());
        assert!(!p.exists());
        assert!(fs::read_dir(&dir).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("data.json.broken-")));
        let _ = fs::remove_dir_all(&dir);
    }
}

/// Любимые — обычный плейлист с постоянным именем: так он сам синхронизируется между ПК и телефоном
/// (на телефоне то же имя). Показывается как «♥ Любимые» на языке интерфейса
pub const LIKED: &str = "♥";
