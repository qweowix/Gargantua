//! Album Universe: раскладка графа на плоскости и выбор того, что рисовать при данном приближении.
//! Раскладка — силовая: связи тянут (сильнее и короче для крепких связей), все узлы отталкиваются
//! (через сетку — O(n), а не O(n²)), лёгкая тяга к центру. Похожие по звуку, по YouTube и по плейлистам
//! артисты оказываются рядом, жанры собирают вокруг себя свои острова.
//! Треки в симуляции не участвуют: их тысячи, они раскладываются спиралью вокруг альбома или артиста.
//! Издалека видны только жанры и главные артисты, при приближении проявляются остальные — так
//! на экране не бывает больше ~900 узлов при любой библиотеке.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::graph::{Graph, Kind, Origin, Rel};

pub struct Universe {
    pub g: Graph,
    pub pos: Vec<(f32, f32)>,
    pub radius: Vec<f32>,
    pub color: Vec<[u8; 3]>,
    /// узлы по важности: сначала самые слушаемые и связанные
    pub rank: Vec<usize>,
}

fn hash(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

pub fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8]
}

/// Цвет жанра: из названия, всегда один и тот же (рок в любой библиотеке одного цвета)
pub fn genre_color(name: &str) -> [u8; 3] {
    hsv((hash(&name.to_lowercase()) % 360) as f32, 0.6, 0.98)
}

pub fn kind_color(k: Kind) -> [u8; 3] {
    match k {
        Kind::Artist => [167, 139, 250],
        Kind::Album => [244, 114, 182],
        Kind::Track => [148, 163, 184],
        Kind::Genre => [251, 191, 36],
        Kind::Person => [52, 211, 153],
        Kind::Label => [96, 165, 250],
    }
}

/// Какой длины связь в покое: крепкие факты держат ближе
fn rest(rel: Rel, w: f32) -> f32 {
    let base = match rel {
        Rel::Member | Rel::Feat | Rel::Collab | Rel::AlbumBy => 46.0,
        Rel::Producer | Rel::Engineer | Rel::Musician => 60.0,
        Rel::Genre => 90.0,
        Rel::Label => 80.0,
        Rel::Sound | Rel::YtSimilar | Rel::CoPlaylist => 64.0,
        Rel::Performs | Rel::OnAlbum => 30.0,
    };
    base * (1.6 - 0.6 * w)
}

impl Universe {
    pub fn new(g: Graph) -> Universe {
        let n = g.nodes.len();
        let imp: Vec<f64> = (0..n).map(|i| g.nodes[i].listened as f64 / 60_000.0 + g.adj[i].len() as f64 * 2.0).collect();
        let mut rank: Vec<usize> = (0..n).collect();
        rank.sort_by(|&a, &b| imp[b].total_cmp(&imp[a]));
        let mut place = vec![0usize; n];
        for (r, &i) in rank.iter().enumerate() {
            place[i] = r;
        }

        // главный жанр артиста — самая сильная связь Genre
        let mut genre_of = vec![None; n];
        for (i, gi) in genre_of.iter_mut().enumerate() {
            if g.nodes[i].kind == Kind::Artist {
                *gi = g.adj[i]
                    .iter()
                    .filter(|&&e| g.edges[e].rel == Rel::Genre)
                    .max_by(|&&x, &&y| g.edges[x].w.total_cmp(&g.edges[y].w))
                    .map(|&e| g.other(e, i));
            }
        }

        // ── силовая раскладка всего, кроме треков ──
        let core: Vec<usize> = (0..n).filter(|&i| g.nodes[i].kind != Kind::Track).collect();
        let mut pos = vec![(0f32, 0f32); n];
        // старт: важные ближе к центру, угол — из имени (раскладка одна и та же от запуска к запуску)
        for &i in &core {
            let a = (hash(&g.nodes[i].id) % 10_000) as f32 / 10_000.0 * std::f32::consts::TAU;
            let r = (place[i] as f32).sqrt() * 28.0 + 10.0;
            pos[i] = (a.cos() * r, a.sin() * r);
        }
        // пружины между «крупными» узлами; связи через трек (продюсер — трек — артист) — тоже пружина
        let mut springs: Vec<(usize, usize, f32, f32)> = Vec::new();
        let main_of = |t: usize| -> Option<usize> {
            g.adj[t].iter().find(|&&e| g.edges[e].rel == Rel::OnAlbum).map(|&e| g.other(e, t)).or_else(|| {
                g.adj[t].iter().filter(|&&e| g.edges[e].rel == Rel::Performs).max_by(|&&x, &&y| g.edges[x].w.total_cmp(&g.edges[y].w)).map(|&e| g.other(e, t))
            })
        };
        let anchor: Vec<Option<usize>> = (0..n).map(|i| if g.nodes[i].kind == Kind::Track { main_of(i) } else { None }).collect();
        for e in &g.edges {
            let (a, b) = (anchor[e.a].unwrap_or(e.a), anchor[e.b].unwrap_or(e.b));
            if a != b && g.nodes[a].kind != Kind::Track && g.nodes[b].kind != Kind::Track {
                springs.push((a, b, rest(e.rel, e.w), 0.04 + 0.06 * e.w));
            }
        }
        let iters = if core.len() > 3000 { 120 } else { 260 };
        let cell = 90f32;
        for it in 0..iters {
            let temp = 24.0 * (1.0 - it as f32 / iters as f32) + 0.6;
            let mut force = vec![(0f32, 0f32); n];
            // отталкивание по сетке: соседи только из своих и соседних клеток
            let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
            for &i in &core {
                grid.entry(((pos[i].0 / cell).floor() as i32, (pos[i].1 / cell).floor() as i32)).or_default().push(i);
            }
            for (&(cx, cy), list) in &grid {
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        let Some(other) = grid.get(&(cx + dx, cy + dy)) else { continue };
                        for &i in list {
                            for &j in other {
                                if j <= i {
                                    continue;
                                }
                                let (vx, vy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                                let d2 = (vx * vx + vy * vy).max(1.0);
                                if d2 > cell * cell {
                                    continue;
                                }
                                let f = 1800.0 / d2;
                                let d = d2.sqrt();
                                force[i].0 += vx / d * f;
                                force[i].1 += vy / d * f;
                                force[j].0 -= vx / d * f;
                                force[j].1 -= vy / d * f;
                            }
                        }
                    }
                }
            }
            for &(a, b, l, k) in &springs {
                let (vx, vy) = (pos[b].0 - pos[a].0, pos[b].1 - pos[a].1);
                let d = (vx * vx + vy * vy).sqrt().max(0.1);
                let f = k * (d - l);
                force[a].0 += vx / d * f;
                force[a].1 += vy / d * f;
                force[b].0 -= vx / d * f;
                force[b].1 -= vy / d * f;
            }
            for &i in &core {
                force[i].0 -= pos[i].0 * 0.006;
                force[i].1 -= pos[i].1 * 0.006;
                let (fx, fy) = force[i];
                let m = (fx * fx + fy * fy).sqrt();
                if m > 0.0 {
                    let s = m.min(temp) / m;
                    pos[i].0 += fx * s;
                    pos[i].1 += fy * s;
                }
            }
        }

        // ── треки: спираль вокруг альбома или артиста ──
        let mut count: HashMap<usize, usize> = HashMap::new();
        let mut tracks: Vec<usize> = (0..n).filter(|&i| g.nodes[i].kind == Kind::Track).collect();
        tracks.sort_by_key(|&i| place[i]);
        let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());

        let mut radius = vec![0f32; n];
        for i in 0..n {
            let min = (g.nodes[i].listened as f32 / 60_000.0).sqrt();
            radius[i] = match g.nodes[i].kind {
                Kind::Genre => 9.0 + min.min(40.0) * 0.5,
                Kind::Artist => 5.0 + min.min(60.0) * 0.4,
                Kind::Album => 4.5 + min.min(40.0) * 0.3,
                Kind::Track => 2.2 + min.min(20.0) * 0.25,
                Kind::Person | Kind::Label => 4.0 + (g.adj[i].len() as f32).sqrt(),
            };
        }
        for &t in &tracks {
            match anchor[t] {
                Some(a) => {
                    let k = count.entry(a).or_default();
                    let ang = *k as f32 * golden + (hash(&g.nodes[a].id) % 628) as f32 / 100.0;
                    let r = radius[a] + 8.0 + 5.5 * (*k as f32).sqrt();
                    pos[t] = (pos[a].0 + ang.cos() * r, pos[a].1 + ang.sin() * r);
                    *k += 1;
                }
                None => {
                    let a = (hash(&g.nodes[t].id) % 10_000) as f32 / 10_000.0 * std::f32::consts::TAU;
                    pos[t] = (a.cos() * 400.0, a.sin() * 400.0);
                }
            }
        }

        let mut color = vec![[0u8; 3]; n];
        for i in 0..n {
            color[i] = match g.nodes[i].kind {
                Kind::Genre => genre_color(&g.nodes[i].name),
                Kind::Artist => genre_of[i].map_or(kind_color(Kind::Artist), |gi| genre_color(&g.nodes[gi].name)),
                k => kind_color(k),
            };
        }
        for &t in &tracks {
            if let Some(a) = anchor[t] {
                let c = color[a];
                color[t] = [c[0] / 2 + 60, c[1] / 2 + 60, c[2] / 2 + 60];
            }
        }
        Universe { g, pos, radius, color, rank }
    }

    /// Что рисовать при приближении zoom (пикселей на единицу мира). focus — выбранный узел, его
    /// соседи и узлы моста видны всегда
    pub fn visible(&self, zoom: f32, kinds: &HashSet<Kind>, focus: &HashSet<usize>) -> Vec<usize> {
        let (core_cap, track_cap) = match zoom {
            z if z < 0.35 => (90, 0),
            z if z < 0.8 => (350, 0),
            z if z < 1.6 => (600, 300),
            _ => (700, 900),
        };
        let mut out = Vec::new();
        let (mut nc, mut nt) = (0, 0);
        for &i in &self.rank {
            let k = self.g.nodes[i].kind;
            if !kinds.contains(&k) && !focus.contains(&i) {
                continue;
            }
            let take = if focus.contains(&i) {
                true
            } else if k == Kind::Track {
                nt += 1;
                nt <= track_cap
            } else {
                // издалека — только жанры и артисты
                let far_ok = zoom >= 0.35 || matches!(k, Kind::Genre | Kind::Artist);
                if far_ok {
                    nc += 1;
                }
                far_ok && nc <= core_cap
            };
            if take {
                out.push(i);
            }
        }
        out
    }

    /// Рёбра между видимыми узлами — три пути SVG по происхождению связи, в координатах мира
    pub fn paths(&self, vis: &[usize], origins: &HashSet<Origin>, hi: &HashSet<usize>) -> [String; 4] {
        use std::fmt::Write;
        let on: HashSet<usize> = vis.iter().copied().collect();
        let mut out: [String; 4] = Default::default();
        let mut seen = HashSet::new();
        for &i in vis {
            for &e in &self.g.adj[i] {
                if !seen.insert(e) {
                    continue;
                }
                let ed = &self.g.edges[e];
                if !on.contains(&ed.a) || !on.contains(&ed.b) {
                    continue;
                }
                let slot = if hi.contains(&e) {
                    3
                } else {
                    match ed.rel.origin() {
                        o if !origins.contains(&o) => continue,
                        Origin::Confirmed => 0,
                        Origin::External => 1,
                        Origin::Derived => 2,
                    }
                };
                let (a, b) = (self.pos[ed.a], self.pos[ed.b]);
                let _ = write!(out[slot], "M{:.1} {:.1}L{:.1} {:.1}", a.0, a.1, b.0, b.1);
            }
        }
        out
    }

    /// Поиск по имени: сначала начинающиеся с запроса, потом содержащие; важные первыми
    pub fn search(&self, q: &str, limit: usize) -> Vec<usize> {
        let q = q.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut starts = Vec::new();
        let mut has = Vec::new();
        for &i in &self.rank {
            let n = self.g.nodes[i].name.to_lowercase();
            if n.starts_with(&q) {
                starts.push(i);
            } else if n.contains(&q) {
                has.push(i);
            }
            if starts.len() >= limit {
                break;
            }
        }
        starts.extend(has);
        starts.truncate(limit);
        starts
    }

    /// Рамка всех узлов: (центр x, центр y, ширина, высота)
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let core = self.rank.iter().take(400).filter(|&&i| self.g.nodes[i].kind != Kind::Track);
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &i in core {
            x0 = x0.min(self.pos[i].0);
            y0 = y0.min(self.pos[i].1);
            x1 = x1.max(self.pos[i].0);
            y1 = y1.max(self.pos[i].1);
        }
        if x0 > x1 {
            return (0.0, 0.0, 100.0, 100.0);
        }
        ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0).max(50.0), (y1 - y0).max(50.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Input;
    use crate::model::Track;

    fn lib() -> Universe {
        let t = |i: usize, a: &str| (Track { id: format!("{i}"), title: format!("song {i}"), artist: a.into(), ..Default::default() }, (i as u64 + 1) * 60_000, 1);
        let mut tracks = Vec::new();
        for i in 0..30 {
            tracks.push(t(i, ["A", "B", "C", "D & A"][i % 4]));
        }
        Universe::new(Graph::build(&Input { tracks, ..Default::default() }))
    }

    #[test]
    fn layout_is_finite_and_stable() {
        let u = lib();
        assert!(u.pos.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
        let again = lib();
        assert_eq!(u.pos, again.pos, "одна библиотека — одна раскладка");
        // D фитует с A — они ближе друг к другу, чем A к B
        let p = |n: &str| u.pos[u.g.find(&Graph::artist_id(n)).unwrap()];
        let d = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        assert!(d(p("A"), p("D")) < d(p("A"), p("B")), "A–D {} A–B {}", d(p("A"), p("D")), d(p("A"), p("B")));
    }

    #[test]
    fn far_away_shows_only_big_things() {
        let u = lib();
        let all: HashSet<Kind> = [Kind::Track, Kind::Artist, Kind::Album, Kind::Genre, Kind::Person, Kind::Label].into();
        let far = u.visible(0.2, &all, &HashSet::new());
        assert!(far.iter().all(|&i| u.g.nodes[i].kind == Kind::Artist));
        let near = u.visible(2.0, &all, &HashSet::new());
        assert!(near.iter().any(|&i| u.g.nodes[i].kind == Kind::Track));
        let paths = u.paths(&near, &[Origin::Confirmed].into(), &HashSet::new());
        assert!(paths[0].starts_with('M'));
        assert!(!u.search("so", 5).is_empty());
    }
}

// ── «Поклонникам нравится» из YouTube Music: кэш по артистам ──

fn similar_file() -> std::path::PathBuf {
    crate::model::Store::dir().join("similar.json")
}

fn similar_store() -> &'static std::sync::Mutex<HashMap<String, Vec<String>>> {
    static S: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Vec<String>>>> = std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(std::fs::read(similar_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()))
}

pub fn similar_cache() -> HashMap<String, Vec<String>> {
    similar_store().lock().unwrap().clone()
}

/// Страница артиста открылась (или подтянули сами) — запоминаем, кого YouTube считает похожими
pub fn remember_similar(artist: &str, names: Vec<String>) {
    let mut s = similar_store().lock().unwrap();
    if s.get(artist) == Some(&names) {
        return;
    }
    s.insert(artist.to_string(), names);
    if let Ok(b) = serde_json::to_vec(&*s) {
        let _ = std::fs::write(similar_file(), b);
    }
}

/// Подтянуть «похожих» для артистов, которых ещё нет в кэше. По одному запросу страницы в 1.5 с —
/// это обычный просмотр страницы артиста, не поток звука
pub fn fetch_similar(names: &[String], done: &dyn Fn()) {
    for n in names {
        if similar_store().lock().unwrap().contains_key(n) {
            continue;
        }
        let r = crate::ytmusic::find_artist(n).and_then(|id| crate::ytmusic::artist(&id));
        match r {
            Ok(a) => {
                remember_similar(n, a.similar.iter().map(|c| c.title.clone()).collect());
                done();
            }
            Err(e) => crate::logln!("похожие для {n}: {e:#}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }
}
