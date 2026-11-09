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
    /// чья это доля статистики аккаунта (поле st.<device> в облаке). Живёт в самом файле: копия файла
    /// на другой машине пишет в ту же долю, а не считается дважды
    pub device: String,
}

/// Сейчас по местному времени (поля C-шного tm: год от 1900, месяц с 0)
fn local_now() -> libc::tm {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let t = now as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    #[cfg(unix)]
    unsafe {
        libc::localtime_r(&t, &mut tm)
    };
    #[cfg(windows)]
    unsafe {
        libc::localtime_s(&mut tm, &t)
    };
    tm
}

/// Номер дня от 1970-01-01 по дате (алгоритм Хиннанта, days_from_civil)
fn days(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Сегодняшний день по местному времени
pub fn today() -> i64 {
    let tm = local_now();
    days(tm.tm_year as i64 + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
}

/// Который час по местному времени — для приветствия на главной
pub fn hour() -> i32 {
    local_now().tm_hour
}

/// Местное время «ЧЧ:ММ»
pub fn hm() -> String {
    let tm = local_now();
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
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
        let mut s: Self = fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if s.device.is_empty() {
            use std::hash::{BuildHasher, Hasher};
            // RandomState засеян случайно на каждый процесс — хватает, чтобы у устройств не совпало
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
            s.device = format!("d{:016x}", h.finish());
        }
        s
    }

    /// Доложить чужую статистику к своей: время и прослушивания складываются, «последний раз» — свежее
    pub fn absorb(&mut self, o: &Stats) {
        for (d, ms) in &o.days {
            *self.days.entry(*d).or_default() += ms;
        }
        for (k, t) in &o.tracks {
            let e = self.tracks.entry(k.clone()).or_insert_with(|| TrackStat { track: t.track.clone(), ..Default::default() });
            e.ms += t.ms;
            e.plays += t.plays;
            if t.last > e.last {
                e.last = t.last;
                e.track = t.track.clone();
            }
        }
        for (a, ms) in &o.artists {
            *self.artists.entry(a.clone()).or_default() += ms;
        }
    }

    /// Своя статистика вместе с долями других устройств аккаунта
    pub fn with(&self, others: &Stats) -> Stats {
        let mut s = self.clone();
        s.absorb(others);
        s
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

    /// Миллисекунд за текущий календарный месяц: 9 ноября — это 9 дней, октябрь не считается
    pub fn this_month(&self, today: i64) -> u64 {
        self.last(today, ymd(today).2 as i64)
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

    /// Любимый артист: (имя, сколько слушали, его трек, который включали чаще всех)
    pub fn favorite(&self) -> Option<(&str, u64, Option<&TrackStat>)> {
        let (name, ms) = self.top_artists(1).into_iter().next()?;
        let track = self.tracks.values().filter(|t| artists(&t.track.artist).contains(&name)).max_by(|a, b| a.plays.cmp(&b.plays).then(a.ms.cmp(&b.ms)));
        Some((name, ms, track))
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
    fn absorb() {
        let t = Track { title: "a".into(), artist: "X, Y".into(), ..Default::default() };
        let (mut pc, mut phone) = (Stats::default(), Stats::default());
        pc.add(&t, 10.0, 5);
        pc.play(&t);
        phone.add(&t, 4.0, 5);
        phone.add(&t, 1.0, 6);
        phone.play(&t);
        let all = pc.with(&phone);
        assert_eq!(all.days[&5], 14_000);
        assert_eq!(all.days[&6], 1_000);
        assert_eq!(all.tracks[&t.key()].plays, 2);
        assert_eq!(all.tracks[&t.key()].ms, 15_000);
        assert_eq!(all.artists["Y"], 15_000);
        assert_eq!(pc.tracks[&t.key()].plays, 1); // своя не тронута
    }

    #[test]
    fn counts() {
        assert_eq!(ymd(0), (1970, 1, 1));
        assert_eq!(ymd(20384), (2025, 10, 23));
        assert_eq!(weekday(20384), 3); // четверг
        assert_eq!(days(2025, 10, 23), 20384);
        assert_eq!(days(1970, 1, 1), 0);
        assert_eq!(days(2024, 2, 29), 19782);
        assert!((0..24).contains(&hour()) && today() > 20000);
        let t = Track { title: "Это любовь".into(), artist: "Скриптонит & Charusha".into(), id: "x".into(), ..Default::default() };
        let mut s = Stats::default();
        s.add(&t, 90.0, 100);
        s.add(&t, 30.0, 94);
        s.play(&t);
        assert_eq!(s.last(100, 1), 90_000);
        assert_eq!(s.last(100, 7), 120_000);
        let mut m = Stats::default();
        m.add(&t, 10.0, days(2026, 10, 31));
        m.add(&t, 20.0, days(2026, 11, 1));
        m.add(&t, 40.0, days(2026, 11, 9));
        assert_eq!(m.this_month(days(2026, 11, 9)), 60_000);
        assert_eq!(m.this_month(days(2026, 10, 31)), 10_000);
        assert_eq!(s.top_artists(5).len(), 2);
        let (name, _, fav) = s.favorite().unwrap();
        assert!(fav.unwrap().track.artist.contains(name));
        assert_eq!(s.top_tracks(5)[0].plays, 1);
        assert_eq!((level(0), level(60_000), level(3_600_000 * 3)), (0, 1, 4));
    }
}
