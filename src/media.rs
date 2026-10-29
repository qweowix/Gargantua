//! Системные медиа-контролы через souvlaki: MPRIS на Linux (панели вроде quickshell/waybar, playerctl,
//! медиа-клавиши). На Windows — SMTC, ему нужен HWND уже показанного окна.

use std::time::{Duration, Instant};

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection};

pub enum Action {
    Play,
    Pause,
    Toggle,
    Next,
    Prev,
    Stop,
    SeekTo(f64),
    SeekBy(f64),
}

pub struct Media {
    c: MediaControls,
    playing: Option<bool>,
    last_pos: Instant,
}

impl Media {
    pub fn new(hwnd: Option<*mut std::ffi::c_void>, on: impl Fn(Action) + Send + 'static) -> Option<Self> {
        // имя уникально на процесс: если открыт второй BlackHole, "blackhole" уже занято, и souvlaki
        // падал с NameTaken. Спецификация MPRIS разрешает суффикс .instanceN
        let name = format!("blackhole.instance{}", std::process::id());
        let cfg = PlatformConfig { display_name: "Gargantua", dbus_name: &name, hwnd };
        // souvlaki на винде без HWND не ошибку возвращает, а паникует — и плеер молча закрывался
        if cfg!(windows) && hwnd.is_none() {
            return None;
        }
        let mut c = MediaControls::new(cfg).map_err(|e| eprintln!("медиа-контролы: {e:?}")).ok()?;
        c.attach(move |e| {
            let a = match e {
                MediaControlEvent::Play => Action::Play,
                MediaControlEvent::Pause => Action::Pause,
                MediaControlEvent::Toggle => Action::Toggle,
                MediaControlEvent::Next => Action::Next,
                MediaControlEvent::Previous => Action::Prev,
                MediaControlEvent::Stop => Action::Stop,
                MediaControlEvent::Seek(SeekDirection::Forward) => Action::SeekBy(10.0),
                MediaControlEvent::Seek(SeekDirection::Backward) => Action::SeekBy(-10.0),
                MediaControlEvent::SeekBy(SeekDirection::Forward, d) => Action::SeekBy(d.as_secs_f64()),
                MediaControlEvent::SeekBy(SeekDirection::Backward, d) => Action::SeekBy(-d.as_secs_f64()),
                MediaControlEvent::SetPosition(MediaPosition(d)) => Action::SeekTo(d.as_secs_f64()),
                _ => return,
            };
            on(a);
        })
        .map_err(|e| eprintln!("медиа-контролы: {e:?}"))
        .ok()?;
        Some(Self { c, playing: None, last_pos: Instant::now() })
    }

    pub fn metadata(&mut self, title: &str, artist: &str, cover: Option<&str>, dur: f64) {
        let _ = self.c.set_metadata(MediaMetadata {
            title: Some(title),
            artist: Some(artist),
            album: None,
            cover_url: cover,
            duration: (dur > 0.0).then(|| Duration::from_secs_f64(dur)),
        });
    }

    /// Статус и позиция. Шлём при смене статуса и раз в 2 с (или сразу, если force — после перемотки):
    /// панели сами досчитывают позицию между обновлениями, спамить D-Bus 3 раза в секунду незачем
    pub fn playback(&mut self, playing: bool, pos: f64, force: bool) {
        if !force && self.playing == Some(playing) && self.last_pos.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.playing = Some(playing);
        self.last_pos = Instant::now();
        let progress = Some(MediaPosition(Duration::from_secs_f64(pos.max(0.0))));
        let _ = self.c.set_playback(if playing { MediaPlayback::Playing { progress } } else { MediaPlayback::Paused { progress } });
    }

    pub fn stopped(&mut self) {
        self.playing = None;
        let _ = self.c.set_playback(MediaPlayback::Stopped);
    }
}
