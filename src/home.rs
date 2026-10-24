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
            eprintln!("микс по {}: {e:#}", t.title);
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

#[cfg(test)]
mod tests {
    use super::*;

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
