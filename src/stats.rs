//! Статистика прослушиваний: секунды по дням, по трекам и по артистам. Отдельный stats.json —
//! пишется раз в минуту и при выходе, большой data.json ради неё не перезаписываем.

use std::collections::{BTreeMap, HashMap};
use std::fs;

use serde::{Deserialize, Serialize};

use crate::model::{Store, Track};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackStat {
    pub track: Track,
    pub ms: u64,
    pub plays: u32,
    #[serde(default)]
    pub last: i64, // когда последний раз звучал (мс от эпохи) — для «Недавно играло»
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    /// номер дня (по местному времени, от 1970-01-01) → миллисекунд прослушано
    pub days: BTreeMap<i64, u64>,
    pub tracks: HashMap<String, TrackStat>,
    pub artists: HashMap<String, u64>,
}

/// Сегодняшний день по местному времени
pub fn today() -> i64 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = now as libc::time_t;
    unsafe { libc::localtime_r(&t, &mut tm) };
    (now + tm.tm_gmtoff as i64).div_euclid(86400)
}

/// Который час по местному времени — для приветствия на главной
pub fn hour() -> i32 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&(now as libc::time_t), &mut tm) };
    tm.tm_hour
}

/// (год, месяц, день) по номеру дня — для подписей
pub fn ymd(day: i64) -> (i64, u32, u32) {
    // алгоритм Хиннанта (days_from_civil наоборот)
    let z = day + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// 0 — понедельник
pub fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7) // 1970-01-01 был четвергом
}

impl Stats {
    fn path() -> std::path::PathBuf {
        Store::dir().join("stats.json")
    }

    pub fn load() -> Self {
        fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let p = Self::path();
        let tmp = p.with_extension("json.tmp");
        if let Ok(data) = serde_json::to_vec(self) {
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(tmp, p);
            }
        }
    }

    /// Дослушали ещё secs секунд трека (храним миллисекунды: опрос раз в треть секунды)
    pub fn add(&mut self, t: &Track, secs: f64, day: i64) {
        let ms = (secs * 1000.0) as u64;
        *self.days.entry(day).or_default() += ms;
        let e = self.tracks.entry(t.key()).or_default();
        e.track = t.clone();
        e.ms += ms;
        e.last = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        for a in artists(&t.artist) {
            *self.artists.entry(a.to_string()).or_default() += ms;
        }
    }

    pub fn play(&mut self, t: &Track) {
        let e = self.tracks.entry(t.key()).or_default();
        e.track = t.clone();
        e.plays += 1;
    }

    /// Миллисекунд за последние n дней, включая сегодня
    pub fn last(&self, today: i64, n: i64) -> u64 {
        self.days.range(today - n + 1..=today).map(|(_, v)| v).sum()
    }

    pub fn top_tracks(&self, n: usize) -> Vec<&TrackStat> {
        let mut v: Vec<&TrackStat> = self.tracks.values().filter(|t| t.plays > 0).collect();
        v.sort_by(|a, b| b.plays.cmp(&a.plays).then(b.ms.cmp(&a.ms)));
        v.truncate(n);
        v
    }

    /// Последние звучавшие треки, свежие первыми
    pub fn recent(&self, n: usize) -> Vec<&TrackStat> {
        let mut v: Vec<&TrackStat> = self.tracks.values().filter(|t| t.last > 0).collect();
        v.sort_by(|a, b| b.last.cmp(&a.last));
        v.truncate(n);
        v
    }

    pub fn top_artists(&self, n: usize) -> Vec<(&str, u64)> {
        let mut v: Vec<(&str, u64)> = self.artists.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(n);
        v
    }
}

/// "A, B & C feat. D" → каждый артист отдельно: фит не должен делать отдельного «любимого артиста»
pub fn artists(s: &str) -> Vec<&str> {
    let mut out = vec![s];
    for sep in [", ", " & ", " feat. ", " feat ", " ft. ", " x ", " и ", "; "] {
        out = out.iter().flat_map(|p| p.split(sep)).collect();
    }
    out.into_iter().map(str::trim).filter(|a| !a.is_empty()).collect()
}

/// Уровень клетки как на GitHub: 0 пусто, дальше 1–4 по минутам за день
pub fn level(ms: u64) -> i32 {
    match ms / 60_000 {
        _ if ms == 0 => 0,
        0..=19 => 1,
        20..=59 => 2,
        60..=119 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        assert_eq!(ymd(0), (1970, 1, 1));
        assert_eq!(ymd(20384), (2025, 10, 23));
        assert_eq!(weekday(20384), 3); // четверг
        let t = Track { title: "Это любовь".into(), artist: "Скриптонит & Charusha".into(), id: "x".into(), ..Default::default() };
        let mut s = Stats::default();
        s.add(&t, 90.0, 100);
        s.add(&t, 30.0, 94);
        s.play(&t);
        assert_eq!(s.last(100, 1), 90_000);
        assert_eq!(s.last(100, 7), 120_000);
        assert_eq!(s.top_artists(5).len(), 2);
        assert_eq!(s.top_tracks(5)[0].plays, 1);
        assert_eq!((level(0), level(60_000), level(3_600_000 * 3)), (0, 1, 4));
    }
}
