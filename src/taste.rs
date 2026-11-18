//! Taste Fingerprint: музыкальный отпечаток человека по настоящей активности — время прослушивания,
//! повторы, пропуски, часы, жанры артистов (MusicBrainz), звучание (Music DNA), кластеры в графе связей.
//! Не подсчёт «кого слушал больше», а характер вкуса: необычные сочетания жанров, разнообразие, динамика.
//! Чего нет в данных — того нет в отпечатке (поле пустое, а не придуманное).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::dna::{self, Dna};
use crate::graph::{Graph, Kind, Rel};
use crate::model::Track;
use crate::stats::Stats;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Share {
    pub name: String,
    /// доля времени 0..1
    pub w: f32,
    pub ms: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Combo {
    pub genres: Vec<String>,
    /// сила сочетания у тебя 0..1 (среднее геометрическое долей)
    pub strength: f32,
    /// насколько редко эти жанры встречаются вместе у одних и тех же артистов, 0..1
    pub rarity: f32,
    /// сколько артистов твоей библиотеки сочетают их, из скольких у них вообще
    pub together: u32,
    pub of: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cluster {
    /// главный жанр или артист кластера
    pub name: String,
    pub artists: Vec<String>,
    pub w: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundPref {
    /// по скольким трекам и какой доле времени посчитано
    pub tracks: u32,
    pub coverage: f32,
    pub tempo: Option<(f32, f32, f32)>, // 25-й, 50-й, 75-й процентили BPM
    pub minor: Option<f32>,             // доля минора по времени
    pub keys: Vec<Share>,
    pub energy: f32,
    pub pulse: f32,
    pub brightness: f32,
    pub bass: f32,
    pub dynamics: f32,
    /// среднее звучание — для сравнения с другими через dna::compare
    pub mean: Option<Dna>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Dynamics {
    pub rising: Vec<Share>,
    pub falling: Vec<Share>,
    pub new_artists: Vec<String>,
    pub stable: Vec<String>,
    /// разнообразие по месяцам: (год·100+месяц, 0..1)
    pub diversity: Vec<(u32, f32)>,
    /// направления, которые впервые появились за период
    pub discovered: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Fingerprint {
    /// период: пусто — всё время, иначе месяцы «с» и «по» (год·100+месяц)
    pub from: u32,
    pub to: u32,
    pub total_ms: u64,
    pub tracks: u32,
    pub artists: Vec<Share>,
    pub genres: Vec<Share>,
    /// доля времени, у которой известны жанры (без MusicBrainz жанров нет)
    pub genre_coverage: f32,
    pub combos: Vec<Combo>,
    pub clusters: Vec<Cluster>,
    pub sound: SoundPref,
    /// разнообразие 0..1: нормированная энтропия долей артистов и жанров
    pub artist_diversity: f32,
    pub genre_diversity: f32,
    /// в среднем раз на трек; доля времени на треки, игравшие больше 5 раз
    pub repeat: f32,
    pub loyal: f32,
    /// доля пропусков среди начатых треков; None — пропуски ещё не копились
    pub skip_rate: Option<f32>,
    /// доля прослушиваний по часам суток (из истории)
    pub hours: Vec<f32>,
    pub dynamics: Dynamics,
}

// ── пропуски: копятся на ПК, отдельным файлом ──

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Skips {
    skipped: HashMap<String, u32>,
    started: HashMap<String, u32>,
}

fn skips_file() -> std::path::PathBuf {
    crate::model::Store::dir().join("skips.json")
}

fn skips() -> &'static Mutex<Skips> {
    static S: std::sync::OnceLock<Mutex<Skips>> = std::sync::OnceLock::new();
    S.get_or_init(|| Mutex::new(std::fs::read(skips_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()))
}

/// Трек сменился: skipped — ушли с него раньше 30 с и раньше трети
pub fn track_left(key: &str, skipped: bool) {
    let mut s = skips().lock().unwrap();
    *s.started.entry(key.to_string()).or_default() += 1;
    if skipped {
        *s.skipped.entry(key.to_string()).or_default() += 1;
    }
    if let Ok(b) = serde_json::to_vec(&*s) {
        let _ = std::fs::write(skips_file(), b);
    }
}

// ── расчёт ──

/// Нормированная энтропия: 0 — всё в одном, 1 — поровну
fn entropy(w: &[f32]) -> f32 {
    let n = w.iter().filter(|x| **x > 0.0).count();
    if n < 2 {
        return 0.0;
    }
    let s: f32 = w.iter().sum();
    -w.iter().filter(|x| **x > 0.0).map(|x| (x / s) * (x / s).ln()).sum::<f32>() / (n as f32).ln()
}

fn shares(m: HashMap<String, u64>, total: u64, limit: usize) -> Vec<Share> {
    let mut v: Vec<Share> = m.into_iter().map(|(name, ms)| Share { w: ms as f32 / total.max(1) as f32, name, ms }).collect();
    v.sort_by(|a, b| b.ms.cmp(&a.ms).then(a.name.cmp(&b.name)));
    v.truncate(limit);
    v
}

/// Время по трекам за период: всё время — из stats.tracks, иначе сумма по месяцам
fn listened(st: &Stats, from: u32, to: u32) -> HashMap<String, (u64, u32)> {
    if from == 0 {
        return st.tracks.iter().map(|(k, s)| (k.clone(), (s.ms, s.plays))).collect();
    }
    let mut out: HashMap<String, (u64, u32)> = HashMap::new();
    for (_, m) in st.months.range(from..=to) {
        for (k, (ms, n)) in m {
            let e = out.entry(k.clone()).or_default();
            e.0 += ms;
            e.1 += n;
        }
    }
    out
}

/// Артисты трека с долями: главный получает всё время, приглашённые — половину
fn artist_time(t: &Track, ms: u64) -> Vec<(String, u64)> {
    crate::graph::track_artists(t).into_iter().enumerate().map(|(i, a)| (a, if i == 0 { ms } else { ms / 2 })).collect()
}

pub fn build(st: &Stats, g: &Graph, history: &[(i64, Track)], from: u32, to: u32) -> Fingerprint {
    let per = listened(st, from, to);
    let track_of = |k: &str| st.tracks.get(k).map(|s| &s.track);
    let total: u64 = per.values().map(|x| x.0).sum();
    let mut fp = Fingerprint { from, to, total_ms: total, tracks: per.values().filter(|x| x.0 > 0).count() as u32, ..Default::default() };
    if total == 0 {
        return fp;
    }

    // артисты
    let mut art: HashMap<String, u64> = HashMap::new();
    for (k, (ms, _)) in &per {
        if let Some(t) = track_of(k) {
            for (a, m) in artist_time(t, *ms) {
                *art.entry(a).or_default() += m;
            }
        }
    }
    let art_total: u64 = art.values().sum();
    fp.artist_diversity = entropy(&art.values().map(|x| *x as f32).collect::<Vec<_>>());
    fp.artists = shares(art.clone(), art_total, 30);

    // жанры: время артиста делится между его жанрами пропорционально голосам MusicBrainz
    let mut genre_ms: HashMap<String, u64> = HashMap::new();
    let mut genres_of: HashMap<String, Vec<(String, f32)>> = HashMap::new();
    for (a, ms) in &art {
        let Some(ai) = g.find(&Graph::artist_id(a)) else { continue };
        let gs: Vec<(String, f32)> = g.adj[ai].iter().filter(|&&e| g.edges[e].rel == Rel::Genre).map(|&e| (g.nodes[g.other(e, ai)].name.clone(), g.edges[e].w)).collect();
        let sum: f32 = gs.iter().map(|x| x.1).sum();
        if sum <= 0.0 {
            continue;
        }
        for (gn, w) in &gs {
            *genre_ms.entry(gn.clone()).or_default() += (*ms as f32 * w / sum) as u64;
        }
        genres_of.insert(a.clone(), gs);
    }
    let gtotal: u64 = genre_ms.values().sum();
    fp.genre_coverage = genres_of.keys().map(|a| art[a]).sum::<u64>() as f32 / art_total.max(1) as f32;
    fp.genre_diversity = entropy(&genre_ms.values().map(|x| *x as f32).collect::<Vec<_>>());
    fp.genres = shares(genre_ms, gtotal, 24);

    // необычные сочетания: жанры, которые ты слушаешь оба (≥ 4 % каждый), но у твоих же артистов они
    // почти не встречаются вместе — значит, это твоё сочетание, а не один жанр под двумя именами
    let top: Vec<&Share> = fp.genres.iter().filter(|s| s.w >= 0.04).take(10).collect();
    let has: HashMap<&str, HashSet<&str>> = top
        .iter()
        .map(|s| (s.name.as_str(), genres_of.iter().filter(|(_, gs)| gs.iter().any(|(n, _)| *n == s.name)).map(|(a, _)| a.as_str()).collect()))
        .collect();
    let mut combos = Vec::new();
    for i in 0..top.len() {
        for j in i + 1..top.len() {
            let (a, b) = (&has[top[i].name.as_str()], &has[top[j].name.as_str()]);
            let both = a.intersection(b).count() as u32;
            let of = a.union(b).count() as u32;
            let rarity = 1.0 - both as f32 / a.len().min(b.len()).max(1) as f32;
            combos.push(Combo { genres: vec![top[i].name.clone(), top[j].name.clone()], strength: (top[i].w * top[j].w).sqrt(), rarity, together: both, of });
            for k in j + 1..top.len() {
                let c = &has[top[k].name.as_str()];
                let all3 = a.iter().filter(|x| b.contains(*x) && c.contains(*x)).count() as u32;
                let pair_max = [a.intersection(b).count(), a.intersection(c).count(), b.intersection(c).count()].into_iter().max().unwrap_or(0) as f32;
                let r3 = 1.0 - pair_max / a.len().min(b.len()).min(c.len()).max(1) as f32;
                let s3 = (top[i].w * top[j].w * top[k].w).cbrt();
                if r3 >= 0.6 && s3 >= 0.06 {
                    combos.push(Combo { genres: vec![top[i].name.clone(), top[j].name.clone(), top[k].name.clone()], strength: s3, rarity: r3, together: all3, of: (a.len() + b.len() + c.len()) as u32 });
                }
            }
        }
    }
    combos.retain(|c| c.rarity >= 0.5);
    combos.sort_by(|x, y| (y.strength * y.rarity).total_cmp(&(x.strength * x.rarity)));
    combos.truncate(6);
    fp.combos = combos;

    // кластеры: распространение меток по графу артистов (фиты, похожие, общие плейлисты, жанры)
    fp.clusters = clusters(g, &art);

    // звучание: DNA треков, взвешенная временем
    let mut with: Vec<(Dna, u64)> = Vec::new();
    for (k, (ms, _)) in &per {
        if *ms > 0 {
            if let Some(d) = dna::get(k) {
                with.push((d, *ms));
            }
        }
    }
    fp.sound = sound(&with, total);

    // повторы и верность
    let plays: Vec<u32> = per.values().map(|x| x.1).filter(|n| *n > 0).collect();
    fp.repeat = plays.iter().sum::<u32>() as f32 / plays.len().max(1) as f32;
    fp.loyal = per.values().filter(|x| x.1 > 5).map(|x| x.0).sum::<u64>() as f32 / total as f32;
    let sk = skips().lock().unwrap();
    let started: u32 = per.keys().filter_map(|k| sk.started.get(k)).sum();
    let skipped: u32 = per.keys().filter_map(|k| sk.skipped.get(k)).sum();
    fp.skip_rate = (started >= 20).then(|| skipped as f32 / started as f32);
    drop(sk);

    // часы суток — из истории (последние 3000 прослушиваний)
    let mut hours = [0f32; 24];
    for (ms, _) in history {
        let (day, h, _) = crate::stats::local(ms / 1000);
        let month = month_of(day);
        if from == 0 || (from..=to).contains(&month) {
            hours[h.clamp(0, 23) as usize] += 1.0;
        }
    }
    let hs: f32 = hours.iter().sum();
    if hs >= 20.0 {
        fp.hours = hours.iter().map(|h| h / hs).collect();
    }

    fp.dynamics = dynamics(st, g, from, to);
    fp
}

/// День (от 1970-01-01) → год·100+месяц
pub fn month_of(day: i64) -> u32 {
    // гражданский календарь из числа дней (алгоритм Хиннанта)
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y * 100 + m) as u32
}

fn sound(with: &[(Dna, u64)], total: u64) -> SoundPref {
    let mut s = SoundPref { tracks: with.len() as u32, ..Default::default() };
    let tw: u64 = with.iter().map(|x| x.1).sum();
    s.coverage = tw as f32 / total.max(1) as f32;
    if with.len() < 3 {
        return s;
    }
    let wavg = |f: &dyn Fn(&Dna) -> f32| with.iter().map(|(d, w)| f(d) * *w as f32).sum::<f32>() / tw as f32;
    s.energy = wavg(&|d| d.energy);
    s.pulse = wavg(&|d| d.pulse);
    s.brightness = wavg(&|d| d.centroid);
    s.bass = wavg(&|d| d.bands[0]);
    s.dynamics = wavg(&|d| d.dyn_db);
    // темп: взвешенные процентили
    let mut bpm: Vec<(f32, u64)> = with.iter().filter_map(|(d, w)| Some((d.bpm?, *w))).collect();
    if bpm.len() >= 3 {
        bpm.sort_by(|a, b| a.0.total_cmp(&b.0));
        let bt: u64 = bpm.iter().map(|x| x.1).sum();
        let at = |p: f32| {
            let mut acc = 0;
            for (b, w) in &bpm {
                acc += w;
                if acc as f32 >= p * bt as f32 {
                    return *b;
                }
            }
            bpm.last().map_or(0.0, |x| x.0)
        };
        s.tempo = Some((at(0.25), at(0.5), at(0.75)));
    }
    let keyed: Vec<&(Dna, u64)> = with.iter().filter(|(d, _)| d.key.is_some()).collect();
    if keyed.len() >= 3 {
        let kt: u64 = keyed.iter().map(|x| x.1).sum();
        s.minor = Some(keyed.iter().filter(|(d, _)| d.minor).map(|x| x.1).sum::<u64>() as f32 / kt as f32);
        let mut km: HashMap<String, u64> = HashMap::new();
        for (d, w) in &keyed {
            *km.entry(d.key_name().unwrap_or_default()).or_default() += w;
        }
        s.keys = shares(km, kt, 5);
    }
    s.mean = dna::weighted_mean(with);
    s
}

/// Распространение меток: каждый артист берёт метку, за которую больше всего «голосуют» его связи.
/// Детерминированно (порядок — по времени прослушивания), без случайности
fn clusters(g: &Graph, art: &HashMap<String, u64>) -> Vec<Cluster> {
    let mut ids: Vec<(usize, u64)> = art.iter().filter_map(|(a, ms)| Some((g.find(&Graph::artist_id(a))?, *ms))).collect();
    ids.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    if ids.is_empty() {
        return Vec::new();
    }
    let on: HashMap<usize, u64> = ids.iter().copied().collect();
    // соседи артиста среди слушаемых артистов: напрямую и через общий жанр
    let mut nb: HashMap<usize, Vec<(usize, f32)>> = HashMap::new();
    for &(a, _) in &ids {
        let mut v: HashMap<usize, f32> = HashMap::new();
        for &e in &g.adj[a] {
            let ed = &g.edges[e];
            let o = g.other(e, a);
            match g.nodes[o].kind {
                Kind::Artist if on.contains_key(&o) => *v.entry(o).or_default() += ed.w,
                Kind::Genre => {
                    for &e2 in &g.adj[o] {
                        let o2 = g.other(e2, o);
                        if o2 != a && on.contains_key(&o2) {
                            *v.entry(o2).or_default() += 0.3 * ed.w * g.edges[e2].w;
                        }
                    }
                }
                _ => {}
            }
        }
        nb.insert(a, v.into_iter().collect());
    }
    let mut label: HashMap<usize, usize> = ids.iter().map(|&(a, _)| (a, a)).collect();
    for _ in 0..12 {
        let mut changed = false;
        for &(a, _) in &ids {
            let mut votes: BTreeMap<usize, f32> = BTreeMap::new();
            for &(o, w) in &nb[&a] {
                *votes.entry(label[&o]).or_default() += w * (1.0 + (on[&o] as f32 / 3_600_000.0).min(5.0));
            }
            if let Some((&best, _)) = votes.iter().max_by(|x, y| x.1.total_cmp(y.1).then(y.0.cmp(x.0))) {
                if label[&a] != best {
                    label.insert(a, best);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let total: u64 = ids.iter().map(|x| x.1).sum::<u64>().max(1);
    let mut groups: HashMap<usize, Vec<(usize, u64)>> = HashMap::new();
    for &(a, ms) in &ids {
        groups.entry(label[&a]).or_default().push((a, ms));
    }
    let mut out: Vec<Cluster> = groups
        .into_values()
        .filter(|m| m.len() >= 2)
        .map(|mut m| {
            m.sort_by(|a, b| b.1.cmp(&a.1));
            // имя — самый частый жанр кластера, если жанры известны; иначе главный артист
            let mut gc: HashMap<usize, f32> = HashMap::new();
            for &(a, ms) in &m {
                for &e in &g.adj[a] {
                    if g.edges[e].rel == Rel::Genre {
                        *gc.entry(g.other(e, a)).or_default() += ms as f32 * g.edges[e].w;
                    }
                }
            }
            let name = gc.into_iter().max_by(|x, y| x.1.total_cmp(&y.1)).map(|(gi, _)| g.nodes[gi].name.clone()).unwrap_or_else(|| g.nodes[m[0].0].name.clone());
            Cluster { name, w: m.iter().map(|x| x.1).sum::<u64>() as f32 / total as f32, artists: m.iter().take(8).map(|x| g.nodes[x.0].name.clone()).collect() }
        })
        .collect();
    out.sort_by(|a, b| b.w.total_cmp(&a.w));
    out.truncate(8);
    out
}

/// Как менялся вкус: последние 3 месяца периода против 3 перед ними
fn dynamics(st: &Stats, g: &Graph, from: u32, to: u32) -> Dynamics {
    let months: Vec<u32> = st.months.keys().copied().filter(|m| from == 0 || (from..=to).contains(m)).collect();
    let mut d = Dynamics::default();
    let track_of = |k: &str| st.tracks.get(k).map(|s| s.track.clone());
    let per_month_artists = |m: u32| -> HashMap<String, u64> {
        let mut a: HashMap<String, u64> = HashMap::new();
        for (k, (ms, _)) in st.months.get(&m).into_iter().flatten() {
            if let Some(t) = track_of(k) {
                for (n, x) in artist_time(&t, *ms) {
                    *a.entry(n).or_default() += x;
                }
            }
        }
        a
    };
    for &m in &months {
        let a = per_month_artists(m);
        d.diversity.push((m, entropy(&a.values().map(|x| *x as f32).collect::<Vec<_>>())));
    }
    if months.len() < 2 {
        return d;
    }
    let split = months.len().saturating_sub(3).max(months.len() / 2);
    let (old, new) = months.split_at(split);
    let old = &old[old.len().saturating_sub(3)..];
    let sum = |ms: &[u32]| -> HashMap<String, u64> {
        let mut out: HashMap<String, u64> = HashMap::new();
        for &m in ms {
            for (a, x) in per_month_artists(m) {
                *out.entry(a).or_default() += x;
            }
        }
        out
    };
    let (a0, a1) = (sum(old), sum(new));
    let (t0, t1) = (a0.values().sum::<u64>().max(1) as f32, a1.values().sum::<u64>().max(1) as f32);
    let mut delta: Vec<(String, f32, u64)> = a0.keys().chain(a1.keys()).collect::<HashSet<_>>().into_iter().map(|a| (a.clone(), a1.get(a).copied().unwrap_or(0) as f32 / t1 - a0.get(a).copied().unwrap_or(0) as f32 / t0, a1.get(a).copied().unwrap_or(0))).collect();
    delta.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(&y.0)));
    d.rising = delta.iter().filter(|x| x.1 > 0.02).take(6).map(|x| Share { name: x.0.clone(), w: x.1, ms: x.2 }).collect();
    d.falling = delta.iter().rev().filter(|x| x.1 < -0.02).take(6).map(|x| Share { name: x.0.clone(), w: x.1, ms: x.2 }).collect();
    // «новые» — не встречались ни в одном месяце до последних трёх
    let before: HashSet<String> = months[..split].iter().flat_map(|&m| per_month_artists(m).into_keys()).collect();
    let mut fresh: Vec<(&String, &u64)> = a1.iter().filter(|(a, _)| !before.contains(*a)).collect();
    fresh.sort_by(|x, y| y.1.cmp(x.1).then(x.0.cmp(y.0)));
    d.new_artists = fresh.iter().take(8).map(|x| x.0.clone()).collect();
    let mut stay: Vec<(&String, f32)> = a0.iter().filter_map(|(a, x)| Some((a, (*x as f32 / t0).min(*a1.get(a)? as f32 / t1)))).filter(|x| x.1 > 0.03).collect();
    stay.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(y.0)));
    d.stable = stay.iter().take(6).map(|x| x.0.clone()).collect();
    // открытые направления: жанры новых артистов, которых не было у старых
    let genres = |names: &mut dyn Iterator<Item = &String>| -> HashSet<String> {
        names
            .filter_map(|a| g.find(&Graph::artist_id(a)))
            .flat_map(|ai| g.adj[ai].iter().filter(|&&e| g.edges[e].rel == Rel::Genre).map(move |&e| g.nodes[g.other(e, ai)].name.clone()).collect::<Vec<_>>())
            .collect()
    };
    let old_g = genres(&mut before.iter());
    d.discovered = genres(&mut a1.keys()).into_iter().filter(|x| !old_g.contains(x)).take(8).collect();
    d.discovered.sort();
    d
}


// ── публичная карточка, код, период, созвездие — общее для ПК и телефона ──

/// Публичная карточка: только то, что человек разрешил показывать
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Public {
    pub name: String,
    pub fp: Fingerprint,
    /// главные треки — чтобы находить, что может понравиться обоим
    pub tracks: Vec<Track>,
    pub shows: [bool; 5],
}

/// Что показывать: жанры, артисты, сочетания, звучание, динамика
pub fn public(fp: &Fingerprint, top: Vec<Track>, shows: [bool; 5], name: &str) -> Public {
    let mut f = fp.clone();
    if !shows[0] {
        f.genres.clear();
        f.genre_coverage = 0.0;
    }
    if !shows[1] {
        f.artists.clear();
        for c in &mut f.clusters {
            c.artists.clear();
        }
    }
    if !shows[2] {
        f.combos.clear();
    }
    if !shows[3] {
        f.sound = Default::default();
    }
    if !shows[4] {
        f.dynamics = Default::default();
    }
    f.hours.clear();
    Public { name: name.into(), tracks: if shows[1] { top } else { Vec::new() }, fp: f, shows }
}

/// Код профиля: из uid, одинаковый на всех устройствах аккаунта (FNV-1a, как у QR плейлистов)
pub fn my_code(uid: &str) -> String {
    let h = format!("{uid}\0taste").bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    (0..8).map(|i| b"ABCDEFGHJKMNPQRSTUVWXYZ23456789"[((h >> (i * 5)) % 31) as usize] as char).collect()
}

/// Период по номеру чипа (0 всё время, 1 год, 2 полгода, 3 три месяца, 4 месяц): (с, по) — год·100+месяц
pub fn period(i: i32) -> (u32, u32) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let (day, _, _) = crate::stats::local(now);
    let to = month_of(day);
    let back = match i {
        1 => 11,
        2 => 5,
        3 => 2,
        4 => 0,
        _ => return (0, 0),
    };
    let (mut y, mut m) = (to / 100, to % 100);
    for _ in 0..back {
        if m == 1 {
            y -= 1;
            m = 12;
        } else {
            m -= 1;
        }
    }
    (y * 100 + m, to)
}

/// Главные треки — для публичной карточки (только сетевые: файл с диска другим не открыть)
pub fn top_tracks(st: &Stats, n: usize) -> Vec<Track> {
    let mut v: Vec<&crate::stats::TrackStat> = st.tracks.values().filter(|s| crate::rooms::shareable(&s.track)).collect();
    v.sort_by(|a, b| b.ms.cmp(&a.ms));
    v.into_iter().take(n).map(|s| Track { cover: String::new(), ..s.track.clone() }).collect()
}

/// Короткая строка вкуса для комнаты — без слов, одинаково понятна на любом языке
pub fn one_liner(fp: &Fingerprint) -> String {
    let mut parts = Vec::new();
    if let Some(c) = fp.combos.first() {
        parts.push(c.genres.join(" + "));
    }
    let g: Vec<String> = fp.genres.iter().take(3).map(|s| s.name.clone()).collect();
    if !g.is_empty() {
        parts.push(g.join(", "));
    } else if !fp.clusters.is_empty() {
        parts.push(fp.clusters.iter().take(3).map(|c| c.name.clone()).collect::<Vec<_>>().join(", "));
    }
    if let Some((_, b, _)) = fp.sound.tempo {
        parts.push(format!("~{b:.0} BPM"));
    }
    format!("🧬 {}", parts.join(" · "))
}

/// Области вкуса для подсветки во Вселенной: артисты кластеров и главные жанры
pub fn regions(fp: &Fingerprint) -> (Vec<String>, Vec<String>) {
    (fp.clusters.iter().flat_map(|c| c.artists.iter().cloned()).chain(fp.artists.iter().take(15).map(|a| a.name.clone())).collect(), fp.genres.iter().take(8).map(|s| s.name.clone()).collect())
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Star {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub c: [u8; 3],
    pub label: String,
    /// 3 — жанр (или кластер), 1 — артист
    pub kind: u8,
    pub lab: bool,
}

/// Созвездие: жанры по кругу, артисты вокруг своих жанров; без жанров — центры кластеров.
/// Координаты 0..1000; пути — связи артистов с жанрами и дуги необычных сочетаний
pub fn stars(fp: &Fingerprint, g: &Graph) -> (Vec<Star>, [String; 2]) {
    use crate::universe::genre_color;
    let mut nodes = Vec::new();
    let mut lines = String::new();
    let mut arcs = String::new();
    let centers: Vec<(String, f32)> = if fp.genres.is_empty() { fp.clusters.iter().map(|c| (c.name.clone(), c.w)).collect() } else { fp.genres.iter().take(12).map(|s| (s.name.clone(), s.w)).collect() };
    if centers.is_empty() {
        return (nodes, [lines, arcs]);
    }
    let maxw = centers.iter().map(|c| c.1).fold(0.0, f32::max).max(1e-3);
    let n = centers.len();
    let mut pos: HashMap<String, (f32, f32)> = HashMap::new();
    for (i, (name, w)) in centers.iter().enumerate() {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 / n as f32 * std::f32::consts::TAU;
        let r = if n == 1 { 0.0 } else { 290.0 };
        let (x, y) = (500.0 + r * a.cos(), 500.0 + r * a.sin());
        pos.insert(name.to_lowercase(), (x, y));
        nodes.push(Star { x, y, r: 16.0 + 34.0 * (w / maxw).sqrt(), c: genre_color(name), label: name.clone(), kind: 3, lab: true });
    }
    let amax = fp.artists.first().map_or(1.0, |a| a.w).max(1e-3);
    let mut around: HashMap<String, usize> = HashMap::new();
    for a in fp.artists.iter().take(36) {
        let home = if fp.genres.is_empty() {
            fp.clusters.iter().find(|c| c.artists.contains(&a.name)).map(|c| c.name.to_lowercase())
        } else {
            g.find(&Graph::artist_id(&a.name)).and_then(|ai| {
                g.adj[ai]
                    .iter()
                    .filter(|&&e| g.edges[e].rel == Rel::Genre && pos.contains_key(&g.nodes[g.other(e, ai)].name.to_lowercase()))
                    .max_by(|&&x, &&y| g.edges[x].w.total_cmp(&g.edges[y].w))
                    .map(|&e| g.nodes[g.other(e, ai)].name.to_lowercase())
            })
        };
        let Some(home) = home else { continue };
        let Some(&(cx, cy)) = pos.get(&home) else { continue };
        let k = around.entry(home.clone()).or_default();
        // наружу от центра, веером
        let base = (cy - 500.0).atan2(cx - 500.0);
        let spread = (*k as f32 - 2.0) * 0.42;
        let d = 95.0 + (*k / 5) as f32 * 45.0;
        let (x, y) = ((cx + d * (base + spread).cos()).clamp(20.0, 980.0), (cy + d * (base + spread).sin()).clamp(20.0, 980.0));
        *k += 1;
        lines += &format!("M{cx:.0} {cy:.0}L{x:.0} {y:.0}");
        nodes.push(Star { x, y, r: 3.0 + 7.0 * (a.w / amax).sqrt(), c: genre_color(&home), label: a.name.clone(), kind: 1, lab: a.w / amax > 0.25 });
    }
    for c in &fp.combos {
        let ps: Vec<(f32, f32)> = c.genres.iter().filter_map(|gn| pos.get(&gn.to_lowercase()).copied()).collect();
        for w in ps.windows(2) {
            arcs += &format!("M{:.0} {:.0}Q500 500 {:.0} {:.0}", w[0].0, w[0].1, w[1].0, w[1].1);
        }
    }
    (nodes, [lines, arcs])
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Discover {
    pub track: Track,
    pub score: i32,
    /// почему: artist — тот же артист; similar — YouTube считает похожим на твоего; genre — твой жанр
    pub why: &'static str,
    pub arg: String,
    pub arg2: String,
}

/// Что может понравиться обоим: любимые треки друга, которых у тебя нет, но которые рядом с твоим вкусом
pub fn discover(me: &Fingerprint, them: &Public, my_lib: &HashSet<String>, g: &Graph) -> Vec<Discover> {
    let my_artists: HashSet<String> = me.artists.iter().map(|a| crate::graph::norm(&a.name)).collect();
    let my_genres: HashSet<String> = me.genres.iter().map(|s| s.name.to_lowercase()).collect();
    let sim = crate::universe::similar_cache();
    let mut out = Vec::new();
    for tr in &them.tracks {
        if my_lib.contains(&tr.key()) {
            continue;
        }
        let arts = crate::graph::track_artists(tr);
        let main = arts.first().cloned().unwrap_or_default();
        let d = if let Some(a) = arts.iter().find(|a| my_artists.contains(&crate::graph::norm(a))) {
            Some((90, "artist", a.clone(), String::new()))
        } else if let Some(mine) = sim.iter().find(|(a, v)| my_artists.contains(&crate::graph::norm(a)) && v.iter().any(|x| crate::graph::norm(x) == crate::graph::norm(&main))).map(|(a, _)| a.clone()) {
            Some((75, "similar", main.clone(), mine))
        } else {
            g.find(&Graph::artist_id(&main))
                .and_then(|ai| g.adj[ai].iter().find(|&&e| g.edges[e].rel == Rel::Genre && my_genres.contains(&g.nodes[g.other(e, ai)].name.to_lowercase())).map(|&e| g.nodes[g.other(e, ai)].name.clone()))
                .map(|gn| (60, "genre", gn, String::new()))
        };
        if let Some((score, why, arg, arg2)) = d {
            out.push(Discover { track: tr.clone(), score, why, arg, arg2 });
        }
    }
    out.sort_by(|a, b| b.score.cmp(&a.score));
    out.truncate(8);
    out
}

// ── что показывать в карточке и опубликована ли она: profile.json в папке настроек ──

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    shows: Option<[bool; 5]>,
    published: bool,
}

fn prefs_file() -> std::path::PathBuf {
    crate::model::Store::dir().join("profile.json")
}

fn prefs() -> Prefs {
    std::fs::read(prefs_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_prefs(p: &Prefs) {
    if let Ok(b) = serde_json::to_vec(p) {
        let _ = std::fs::write(prefs_file(), b);
    }
}

pub fn shows() -> [bool; 5] {
    prefs().shows.unwrap_or([true; 5])
}

pub fn set_shows(s: [bool; 5]) {
    let mut p = prefs();
    p.shows = Some(s);
    save_prefs(&p);
}

pub fn published() -> bool {
    prefs().published
}

pub fn set_published(on: bool) {
    let mut p = prefs();
    p.published = on;
    save_prefs(&p);
}

// ── сравнение двух отпечатков ──

#[derive(Clone, Debug, Default, Serialize)]
pub struct Compat {
    /// общие жанры: (жанр, твоя доля, их доля)
    pub shared: Vec<(String, f32, f32)>,
    pub only_me: Vec<String>,
    pub only_them: Vec<String>,
    pub shared_artists: Vec<String>,
    /// неожиданное: общие артисты не из первой десятки ни у кого и общие редкие сочетания
    pub surprises: Vec<String>,
    /// сходство звучания по составляющим, если у обоих есть DNA
    pub sound: Option<dna::Similarity>,
    /// пересечение долей жанров и артистов (сумма минимумов) — это не «процент совместимости», а одна из цифр
    pub genre_overlap: f32,
    pub artist_overlap: f32,
}

pub fn compare(me: &Fingerprint, them: &Fingerprint) -> Compat {
    let mut c = Compat::default();
    let gm: HashMap<&str, f32> = me.genres.iter().map(|s| (s.name.as_str(), s.w)).collect();
    let gt: HashMap<&str, f32> = them.genres.iter().map(|s| (s.name.as_str(), s.w)).collect();
    for s in &me.genres {
        if let Some(&w) = gt.get(s.name.as_str()) {
            c.shared.push((s.name.clone(), s.w, w));
            c.genre_overlap += s.w.min(w);
        } else if s.w >= 0.05 {
            c.only_me.push(s.name.clone());
        }
    }
    c.shared.sort_by(|a, b| (b.1.min(b.2)).total_cmp(&a.1.min(a.2)));
    c.only_them = them.genres.iter().filter(|s| s.w >= 0.05 && !gm.contains_key(s.name.as_str())).map(|s| s.name.clone()).collect();
    let am: HashMap<String, (usize, f32)> = me.artists.iter().enumerate().map(|(i, s)| (crate::graph::norm(&s.name), (i, s.w))).collect();
    for (j, s) in them.artists.iter().enumerate() {
        if let Some(&(i, w)) = am.get(&crate::graph::norm(&s.name)) {
            c.shared_artists.push(s.name.clone());
            c.artist_overlap += w.min(s.w);
            if i >= 10 && j >= 10 {
                c.surprises.push(s.name.clone());
            }
        }
    }
    for a in &me.combos {
        if them.combos.iter().any(|b| a.genres.iter().all(|g| b.genres.contains(g))) {
            c.surprises.push(a.genres.join(" + "));
        }
    }
    if let (Some(a), Some(b)) = (&me.sound.mean, &them.sound.mean) {
        c.sound = Some(dna::compare(a, b));
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Input;
    use crate::musicbrainz::MbArtist;

    #[test]
    fn month_from_day() {
        assert_eq!(month_of(crate::stats::days(2026, 11, 18)), 202611);
        assert_eq!(month_of(crate::stats::days(2024, 2, 29)), 202402);
        assert_eq!(month_of(0), 197001);
    }

    /// Метал и эмбиент у разных артистов, слушаются оба — это необычное сочетание; рок и метал у одних
    /// и тех же артистов — нет
    #[test]
    fn finds_unusual_combo() {
        let t = |id: &str, a: &str| Track { id: id.into(), title: id.into(), artist: a.into(), ..Default::default() };
        let mut st = Stats::default();
        let day = crate::stats::days(2026, 11, 1);
        for (id, a, sec) in [("1", "Heavy", 3000.0), ("2", "Heavy2", 2000.0), ("3", "Calm", 2500.0), ("4", "Calm2", 1500.0)] {
            st.add(&t(id, a), sec, day);
        }
        let mb = |n: &str, g: &[&str]| MbArtist { id: n.into(), name: n.into(), genres: g.iter().map(|x| (x.to_string(), 5)).collect(), ..Default::default() };
        let inp = Input {
            tracks: st.tracks.values().map(|s| (s.track.clone(), s.ms, s.plays)).collect(),
            mb_artists: [mb("Heavy", &["metal", "rock"]), mb("Heavy2", &["metal", "rock"]), mb("Calm", &["ambient"]), mb("Calm2", &["ambient"])].into_iter().map(|a| (a.name.clone(), a)).collect(),
            ..Default::default()
        };
        let g = Graph::build(&inp);
        let fp = build(&st, &g, &[], 0, 0);
        assert_eq!(fp.genres.len(), 3);
        assert!(fp.genre_coverage > 0.99);
        let names: Vec<String> = fp.combos.iter().map(|c| c.genres.join("+")).collect();
        assert!(names.iter().any(|n| n.contains("ambient") && n.contains("metal")), "{names:?}");
        assert!(!names.iter().any(|n| n == "metal+rock" || n == "rock+metal"), "{names:?}");
        // сам с собой — полное пересечение
        let c = compare(&fp, &fp);
        assert!((c.genre_overlap - 1.0).abs() < 0.01);
        assert!(c.only_me.is_empty());
    }
}
