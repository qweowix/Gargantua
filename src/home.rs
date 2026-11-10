//! Главный экран: миксы под тебя (по статистике) и похожие артисты. Собирается раз в день в фоне —
//! это несколько HTTP-запросов к YT Music, без yt-dlp, — и лежит в home.json до завтра.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::model::{Store, Track};
use crate::stats::{self, Stats};
use crate::ytmusic;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Mix {
    pub title: String,
    pub sub: String,
    pub cover: String,
    pub tracks: Vec<Track>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Home {
    pub day: i64,
    pub seeds: Vec<String>, // по чему собрано — поменялся топ, пересобираем не дожидаясь завтра
    pub mixes: Vec<Mix>,
    pub similar_to: String,
    pub similar: Vec<ytmusic::Card>,
}

fn path() -> std::path::PathBuf {
    Store::dir().join("home.json")
}

pub fn load() -> Home {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(h: &Home) {
    if let Ok(b) = serde_json::to_vec(h) {
        let _ = std::fs::write(path(), b);
    }
}

/// Что брать за основу: самые слушаемые треки и любимые артисты (их самый слушаемый трек)
pub fn seeds(st: &Stats) -> (Vec<Track>, Vec<(String, Track)>) {
    let tracks: Vec<Track> = st.top_tracks(3).into_iter().map(|t| t.track.clone()).collect();
    let artists = st
        .top_artists(3)
        .into_iter()
        .filter_map(|(name, _)| {
            let best = st.tracks.values().filter(|t| stats::artists(&t.track.artist).contains(&name)).max_by_key(|t| (t.plays, t.ms))?;
            Some((name.to_string(), best.track.clone()))
        })
        .collect();
    (tracks, artists)
}

pub fn seed_key(st: &Stats) -> Vec<String> {
    let (t, a) = seeds(st);
    t.iter().map(Track::key).chain(a.iter().map(|(n, _)| n.clone())).collect()
}

fn song(t: &Track) -> String {
    format!("{}|{}", t.title.to_lowercase(), t.artist.to_lowercase())
}

/// По кругу из нескольких списков, без повторов — чтобы в миксе не шли подряд 15 треков одного радио
fn weave(lists: Vec<Vec<Track>>, skip: &HashSet<String>, n: usize) -> Vec<Track> {
    let mut seen = skip.clone();
    let mut out = vec![];
    let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
    for i in 0..longest {
        for l in &lists {
            if let Some(t) = l.get(i) {
                if seen.insert(song(t)) {
                    out.push(t.clone());
                }
            }
        }
    }
    out.truncate(n);
    out
}

/// Собирает всё (блокирующе). names — подписи на языке интерфейса: (твой микс, открытия, "Микс: {}")
/// similar — похожие артисты по имени (в тестах без сети)
pub fn build(
    st: &Stats,
    radio: impl Fn(&Track) -> anyhow::Result<Vec<Track>>,
    similar: impl Fn(&str) -> Vec<ytmusic::Card>,
    names: (String, String, String),
) -> Home {
    let (top, artists) = seeds(st);
    let known: HashSet<String> = st.artists.keys().map(|a| a.to_lowercase()).collect();
    let mut radios: Vec<Vec<Track>> = vec![];
    let mut mixes = vec![];
    for t in &top {
        radios.push(radio(t).unwrap_or_else(|e| {
            crate::logln!("микс по {}: {e:#}", t.title);
            vec![]
        }));
    }
    let main = weave(radios.clone(), &HashSet::new(), 50);
    if !main.is_empty() {
        let who: Vec<String> = main.iter().map(|t| t.artist.clone()).fold(vec![], |mut v, a| {
            if !v.contains(&a) {
                v.push(a);
            }
            v
        });
        mixes.push(Mix { title: names.0, sub: who.iter().take(3).cloned().collect::<Vec<_>>().join(", "), cover: main[0].cover.clone(), tracks: main.clone() });
    }
    for (name, t) in &artists {
        let r = radio(t).unwrap_or_default();
        let m = weave(vec![r], &HashSet::new(), 40);
        if m.len() > 5 {
            let others: Vec<String> = m.iter().map(|t| t.artist.clone()).filter(|a| a != name).take(2).collect();
            mixes.push(Mix { title: names.2.replace("{}", name), sub: format!("{name}, {}", others.join(", ")), cover: t.cover.clone(), tracks: m.clone() });
            radios.push(m);
        }
    }
    // открытия: из всех радио только артисты, которых ты ещё не слушал
    let fresh: Vec<Vec<Track>> =
        radios.iter().map(|r| r.iter().filter(|t| !stats::artists(&t.artist).iter().any(|a| known.contains(&a.to_lowercase()))).cloned().collect()).collect();
    let disc = weave(fresh, &HashSet::new(), 40);
    if disc.len() > 5 {
        mixes.push(Mix { title: names.1, sub: disc.iter().take(3).map(|t| t.artist.clone()).collect::<Vec<_>>().join(", "), cover: disc[0].cover.clone(), tracks: disc });
    }
    let (similar_to, similar) = match artists.first() {
        Some((name, _)) => (name.clone(), similar(name)),
        None => (String::new(), vec![]),
    };
    Home { day: stats::today(), seeds: seed_key(st), mixes, similar_to, similar }
}

/// Умные подборки из статистики — без сети, считаются на лету при каждом открытии главной.
/// names: (давно не слушал, заслушано в этом месяце, лайкнул и забыл). Подборка меньше 5 треков не показывается
pub fn smart(st: &Stats, liked: &[Track], names: (String, String, String)) -> Vec<Mix> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    smart_at(st, liked, names, now, stats::today())
}

fn smart_at(st: &Stats, liked: &[Track], names: (String, String, String), now_ms: i64, today: i64) -> Vec<Mix> {
    let mut out = vec![];
    let mut push = |title: String, tracks: Vec<Track>| {
        if tracks.len() >= 5 {
            let who = tracks.iter().map(|t| t.artist.clone()).fold(vec![], |mut v: Vec<String>, a| {
                if !v.contains(&a) && v.len() < 3 {
                    v.push(a);
                }
                v
            });
            out.push(Mix { title, sub: who.join(", "), cover: tracks[0].cover.clone(), tracks });
        }
    };
    // давно не слушал: когда-то включал не раз, но больше месяца не трогал
    let month_ms = 30 * 86_400_000i64;
    let mut old: Vec<&stats::TrackStat> = st.tracks.values().filter(|t| t.plays >= 3 && t.last > 0 && now_ms - t.last > month_ms).collect();
    old.sort_by(|a, b| b.plays.cmp(&a.plays).then(a.last.cmp(&b.last)).then(a.track.title.cmp(&b.track.title)));
    push(names.0, old.into_iter().take(50).map(|t| t.track.clone()).collect());
    // заслушано в этом месяце
    let (y, m, _) = stats::ymd(today);
    let mut hot: Vec<(&String, &(u64, u32))> = st.months.get(&(y as u32 * 100 + m)).into_iter().flatten().filter(|(_, v)| v.1 >= 2).collect();
    hot.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then(b.1 .0.cmp(&a.1 .0)).then(a.0.cmp(b.0)));
    push(names.1, hot.into_iter().take(50).filter_map(|(k, _)| st.tracks.get(k).map(|t| t.track.clone())).collect());
    // лайкнул и забыл: в любимых, а включал меньше двух раз
    push(names.2, liked.iter().filter(|t| st.tracks.get(&t.key()).is_none_or(|s| s.plays < 2)).take(50).cloned().collect());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_lists() {
        let t = |id: &str| Track { id: id.into(), title: format!("song {id}"), artist: format!("A{id}"), ..Default::default() };
        let day = 86_400_000i64;
        let (now, today) = (1000 * day, 20_000);
        let mut st = Stats::default();
        let (y, m, _) = stats::ymd(today);
        for i in 0..12 {
            let tr = t(&i.to_string());
            // первые шесть — старые и заслушанные, остальные — свежие, из этого месяца
            let e = st.tracks.entry(tr.key()).or_default();
            e.track = tr.clone();
            e.plays = 4;
            e.last = if i < 6 { now - 40 * day } else { now - day };
            if i >= 6 {
                st.months.entry(y as u32 * 100 + m).or_default().insert(tr.key(), (1000, 3 + i as u32));
            }
        }
        let liked: Vec<Track> = (20..26).map(|i| t(&i.to_string())).chain([t("0")]).collect();
        let s = smart_at(&st, &liked, ("old".into(), "hot".into(), "forgot".into()), now, today);
        let get = |n: &str| s.iter().find(|m| m.title == n).unwrap_or_else(|| panic!("нет подборки {n}"));
        assert_eq!(get("old").tracks.len(), 6);
        assert!(get("old").tracks.iter().all(|t| t.id.parse::<u32>().unwrap() < 6));
        // самый заслушанный в месяце — первым
        assert_eq!(get("hot").tracks[0].id, "11");
        assert_eq!(get("hot").tracks.len(), 6);
        // «0» лайкнут, но заслушан — в забытые не попадает
        assert_eq!(get("forgot").tracks.len(), 6);
        assert!(get("forgot").tracks.iter().all(|t| t.id != "0"));
        // мало треков — подборки нет
        assert!(smart_at(&Stats::default(), &liked[..3], ("a".into(), "b".into(), "c".into()), now, today).is_empty());
    }

    #[test]
    fn mixes_from_stats() {
        let t = |id: &str, a: &str| Track { id: id.into(), title: format!("song {id}"), artist: a.into(), ..Default::default() };
        let mut st = Stats::default();
        for (tr, n) in [(t("1", "TV Girl"), 5), (t("2", "Скриптонит"), 3)] {
            for _ in 0..n {
                st.play(&tr);
            }
            st.add(&tr, 60.0 * n as f64, 1);
        }
        // «радио» по каждому треку: свой артист + один новый
        let radio = |s: &Track| Ok(vec![s.clone(), t(&format!("{}a", s.id), &s.artist), t(&format!("{}n", s.id), "Новый"), t("x1", "X"), t("x2", "Y"), t("x3", "Z"), t("x4", "W")]);
        let h = build(&st, radio, |_| vec![], ("Твой микс".into(), "Открытия".into(), "Микс: {}".into()));
        assert_eq!(h.mixes[0].title, "Твой микс");
        assert!(h.mixes.iter().any(|m| m.title == "Микс: TV Girl"));
        let disc = h.mixes.iter().find(|m| m.title == "Открытия").expect("нет открытий");
        assert!(disc.tracks.iter().all(|t| t.artist != "TV Girl" && t.artist != "Скриптонит"));
        // в миксе без повторов
        let mut seen = HashSet::new();
        assert!(h.mixes[0].tracks.iter().all(|t| seen.insert(song(t))));
    }
}
