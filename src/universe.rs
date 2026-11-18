//! Album Universe: небо артистов. Звезда — артист (размер — сколько слушал), туманность — жанр (из MusicBrainz),
//! линия — связь между артистами, собранная из всего графа: фиты, участники, общий продюсер или лейбл
//! (факты), «похожие» YouTube (чужой алгоритм), слушаешь подряд, общие плейлисты, похожи по звуку (вычислено).
//! Несколько связей одной пары сливаются в одну линию с перечнем причин. Треки и альбомы на карте не висят —
//! они в панели артиста: так у звезды не бывает трёхсот лучей к своим же трекам.
//! Раскладка силовая (связи тянут, все отталкиваются через сетку, артисты одного жанра тянутся к общему
//! центру), детерминированная. Рисуется только видимое и только самые сильные связи каждой звезды.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use serde::Serialize;

use crate::dna::{self, Dna};
use crate::graph::{Graph, Kind, Origin, Rel};

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

/// Артист без жанра — мягкий сиреневый
const NO_GENRE: [u8; 3] = [196, 181, 253];

#[derive(Clone, Debug, Serialize)]
pub struct Reason {
    pub rel: Rel,
    pub note: String,
}

/// Связь двух звёзд: все причины сразу. w — общая сила (шумное ИЛИ с поправкой на доверие к источнику)
#[derive(Clone, Debug, Serialize)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    pub w: f32,
    /// самое надёжное происхождение среди причин
    pub origin: Origin,
    pub why: Vec<Reason>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Nebula {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub color: [u8; 3],
    /// сколько слушал этот жанр, мс
    pub listened: u64,
    pub stars: usize,
}

pub struct Universe {
    pub g: Graph,
    /// узлы графа-артисты, самые слушаемые первыми
    pub stars: Vec<usize>,
    pub pos: Vec<(f32, f32)>,
    pub radius: Vec<f32>,
    pub color: Vec<[u8; 3]>,
    pub links: Vec<Link>,
    /// связи звезды (номера в links), сильные первыми
    pub ladj: Vec<Vec<usize>>,
    pub nebulas: Vec<Nebula>,
    /// узел графа → звезда
    pub star_of: HashMap<usize, usize>,
}

fn trust(o: Origin) -> f32 {
    match o {
        Origin::Confirmed => 1.0,
        Origin::External => 0.8,
        Origin::Derived => 0.7,
    }
}

fn rank(o: Origin) -> u8 {
    match o {
        Origin::Confirmed => 0,
        Origin::External => 1,
        Origin::Derived => 2,
    }
}

impl Universe {
    pub fn new(g: Graph) -> Universe {
        // ── звёзды ──
        let mut stars: Vec<usize> = (0..g.nodes.len()).filter(|&i| g.nodes[i].kind == Kind::Artist).collect();
        let deg = |i: usize| g.adj[i].len();
        stars.sort_by(|&a, &b| g.nodes[b].listened.cmp(&g.nodes[a].listened).then(deg(b).cmp(&deg(a))).then(a.cmp(&b)));
        let star_of: HashMap<usize, usize> = stars.iter().enumerate().map(|(s, &n)| (n, s)).collect();
        let n = stars.len();

        // ── связи между артистами, собранные из всего графа ──
        let mut pair: HashMap<(usize, usize), Vec<(Rel, f32, String)>> = HashMap::new();
        let mut add = |a: usize, b: usize, rel: Rel, w: f32, note: String| {
            if a != b {
                pair.entry((a.min(b), a.max(b))).or_default().push((rel, w, note));
            }
        };
        let artists_of_track = |t: usize| -> Vec<usize> { g.adj[t].iter().filter(|&&e| g.edges[e].rel == Rel::Performs).filter_map(|&e| star_of.get(&g.other(e, t)).copied()).collect() };
        for e in &g.edges {
            if let (Some(&a), Some(&b)) = (star_of.get(&e.a), star_of.get(&e.b)) {
                // фит, участник, совместный проект, похожие, общие плейлисты, слушаешь подряд
                add(a, b, e.rel, e.w, e.note.clone());
            }
        }
        // люди за записями и лейблы: одни и те же у двух артистов — факт из MusicBrainz
        for (pi, p) in g.nodes.iter().enumerate() {
            match p.kind {
                Kind::Person | Kind::Artist => {
                    let mut by: HashMap<usize, Rel> = HashMap::new();
                    for &e in &g.adj[pi] {
                        let ed = &g.edges[e];
                        if matches!(ed.rel, Rel::Producer | Rel::Engineer | Rel::Musician) {
                            for a in artists_of_track(g.other(e, pi)) {
                                by.entry(a).or_insert(ed.rel);
                            }
                        }
                    }
                    let mut list: Vec<(usize, Rel)> = by.into_iter().collect();
                    list.sort_by_key(|x| x.0);
                    if list.len() > 12 {
                        continue; // штатный звукорежиссёр лейбла связывает всех — это не сходство
                    }
                    for i in 0..list.len() {
                        for j in i + 1..list.len() {
                            add(list[i].0, list[j].0, list[i].1, 0.8, p.name.clone());
                        }
                    }
                }
                Kind::Label => {
                    let mut arts: Vec<usize> = Vec::new();
                    for &e in &g.adj[pi] {
                        let al = g.other(e, pi);
                        for &e2 in &g.adj[al] {
                            if g.edges[e2].rel == Rel::AlbumBy {
                                if let Some(&s) = star_of.get(&g.other(e2, al)) {
                                    arts.push(s);
                                }
                            }
                        }
                    }
                    arts.sort_unstable();
                    arts.dedup();
                    if arts.len() <= 12 {
                        for i in 0..arts.len() {
                            for j in i + 1..arts.len() {
                                add(arts[i], arts[j], Rel::Label, 0.35, p.name.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // звук: средний отпечаток артиста, у каждого — до трёх самых похожих (если похожи всерьёз)
        let mean: Vec<Option<Dna>> = stars
            .iter()
            .map(|&a| {
                let ds: Vec<Dna> = g.adj[a].iter().filter(|&&e| g.edges[e].rel == Rel::Performs).filter_map(|&e| dna::get(&g.nodes[g.other(e, a)].track.as_ref()?.key())).collect();
                match ds.len() {
                    0 => None,
                    1 => Some(ds[0].clone()),
                    _ => dna::mean(&ds.iter().collect::<Vec<_>>()),
                }
            })
            .collect();
        let with: Vec<usize> = (0..n).filter(|&s| mean[s].is_some()).collect();
        for &a in &with {
            let mut best: Vec<(f32, usize, dna::Similarity)> = with
                .iter()
                .filter(|&&b| b != a)
                .map(|&b| {
                    let s = dna::compare(mean[a].as_ref().unwrap(), mean[b].as_ref().unwrap());
                    (s.total, b, s)
                })
                .filter(|x| x.0 >= 0.82)
                .collect();
            best.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)));
            for (w, b, s) in best.into_iter().take(3) {
                add(a, b, Rel::Sound, w, format!("{:.0} %: ритм {:.0}, тембр {:.0}, гармония {:.0}", w * 100.0, s.rhythm * 100.0, s.timbre * 100.0, s.harmony * 100.0));
            }
        }
        let mut links: Vec<Link> = pair
            .into_iter()
            .map(|((a, b), v)| {
                let mut why: Vec<Reason> = Vec::new();
                let mut miss = 1.0f32;
                let mut origin = Origin::Derived;
                let mut seen = HashSet::new();
                for (rel, w, note) in v {
                    if !seen.insert((rel, note.clone())) {
                        continue;
                    }
                    miss *= 1.0 - (w * trust(rel.origin())).clamp(0.0, 0.95);
                    if rank(rel.origin()) < rank(origin) {
                        origin = rel.origin();
                    }
                    why.push(Reason { rel, note });
                }
                why.sort_by_key(|r| (rank(r.rel.origin()), format!("{:?}", r.rel)));
                Link { a, b, w: 1.0 - miss, origin, why }
            })
            .collect();
        links.sort_by(|x, y| (x.a, x.b).cmp(&(y.a, y.b)));
        let mut ladj = vec![Vec::new(); n];
        for (li, l) in links.iter().enumerate() {
            ladj[l.a].push(li);
            ladj[l.b].push(li);
        }
        for v in &mut ladj {
            v.sort_by(|&x, &y| links[y].w.total_cmp(&links[x].w).then(x.cmp(&y)));
        }

        // ── жанры: главный жанр звезды, туманности — жанры хотя бы с двумя звёздами ──
        let main_genre: Vec<Option<usize>> = stars
            .iter()
            .map(|&a| g.adj[a].iter().filter(|&&e| g.edges[e].rel == Rel::Genre).max_by(|&&x, &&y| g.edges[x].w.total_cmp(&g.edges[y].w).then(y.cmp(&x))).map(|&e| g.other(e, a)))
            .collect();
        let mut count: HashMap<usize, usize> = HashMap::new();
        for gn in main_genre.iter().flatten() {
            *count.entry(*gn).or_default() += 1;
        }
        let mut genres: Vec<usize> = count.iter().filter(|(_, c)| **c >= 2).map(|(g, _)| *g).collect();
        genres.sort_by(|&a, &b| g.nodes[b].listened.cmp(&g.nodes[a].listened).then(a.cmp(&b)));
        genres.truncate(16);
        let neb_index: HashMap<usize, usize> = genres.iter().enumerate().map(|(i, &gn)| (gn, i)).collect();
        let nebula_of: Vec<Option<usize>> = main_genre.iter().map(|m| m.and_then(|gn| neb_index.get(&gn).copied())).collect();

        // ── раскладка ──
        let mut pos = vec![(0f32, 0f32); n];
        // старт: туманности — по кругу, их звёзды — рядом; остальные — по спирали от центра, важные ближе
        let k = genres.len().max(1) as f32;
        for s in 0..n {
            let a = (hash(&g.nodes[stars[s]].id) % 10_000) as f32 / 10_000.0 * std::f32::consts::TAU;
            pos[s] = match nebula_of[s] {
                Some(ni) => {
                    let ga = ni as f32 / k * std::f32::consts::TAU;
                    (ga.cos() * 260.0 + a.cos() * 60.0, ga.sin() * 260.0 + a.sin() * 60.0)
                }
                None => {
                    let r = (s as f32).sqrt() * 26.0 + 20.0;
                    (a.cos() * r, a.sin() * r)
                }
            };
        }
        let iters = if n > 2000 { 150 } else { 300 };
        let cell = 80f32;
        for it in 0..iters {
            let temp = 22.0 * (1.0 - it as f32 / iters as f32) + 0.5;
            let mut f = vec![(0f32, 0f32); n];
            let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
            for (s, p) in pos.iter().enumerate() {
                grid.entry(((p.0 / cell).floor() as i32, (p.1 / cell).floor() as i32)).or_default().push(s);
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
                                let d2 = (vx * vx + vy * vy).max(4.0);
                                if d2 > cell * cell {
                                    continue;
                                }
                                let d = d2.sqrt();
                                let push = 1400.0 / d2;
                                f[i].0 += vx / d * push;
                                f[i].1 += vy / d * push;
                                f[j].0 -= vx / d * push;
                                f[j].1 -= vy / d * push;
                            }
                        }
                    }
                }
            }
            for l in &links {
                let (vx, vy) = (pos[l.b].0 - pos[l.a].0, pos[l.b].1 - pos[l.a].1);
                let d = (vx * vx + vy * vy).sqrt().max(0.1);
                let rest = 40.0 + 60.0 * (1.0 - l.w);
                let pull = (0.03 + 0.07 * l.w) * (d - rest);
                f[l.a].0 += vx / d * pull;
                f[l.a].1 += vy / d * pull;
                f[l.b].0 -= vx / d * pull;
                f[l.b].1 -= vy / d * pull;
            }
            // туманности держатся вместе: каждая звезда тянется к центру своего жанра
            let mut cen = vec![(0f32, 0f32, 0f32); genres.len()];
            for s in 0..n {
                if let Some(ni) = nebula_of[s] {
                    cen[ni].0 += pos[s].0;
                    cen[ni].1 += pos[s].1;
                    cen[ni].2 += 1.0;
                }
            }
            for s in 0..n {
                if let Some(ni) = nebula_of[s] {
                    let (cx, cy) = (cen[ni].0 / cen[ni].2, cen[ni].1 / cen[ni].2);
                    f[s].0 += (cx - pos[s].0) * 0.02;
                    f[s].1 += (cy - pos[s].1) * 0.02;
                }
                f[s].0 -= pos[s].0 * 0.004;
                f[s].1 -= pos[s].1 * 0.004;
                let m = (f[s].0 * f[s].0 + f[s].1 * f[s].1).sqrt();
                if m > 0.0 {
                    let k = m.min(temp) / m;
                    pos[s].0 += f[s].0 * k;
                    pos[s].1 += f[s].1 * k;
                }
            }
        }

        let maxl = stars.first().map_or(1, |&a| g.nodes[a].listened).max(1) as f32;
        let radius: Vec<f32> = stars.iter().map(|&a| 4.0 + 18.0 * (g.nodes[a].listened as f32 / maxl).sqrt() + (ladj_len(&ladj, star_of[&a]) as f32).sqrt() * 0.6).collect();
        let color: Vec<[u8; 3]> = (0..n).map(|s| main_genre[s].map_or(NO_GENRE, |gn| genre_color(&g.nodes[gn].name))).collect();
        let nebulas: Vec<Nebula> = genres
            .iter()
            .enumerate()
            .map(|(ni, &gn)| {
                let members: Vec<usize> = (0..n).filter(|&s| nebula_of[s] == Some(ni)).collect();
                let (cx, cy) = members.iter().fold((0.0, 0.0), |a, &s| (a.0 + pos[s].0, a.1 + pos[s].1));
                let (cx, cy) = (cx / members.len() as f32, cy / members.len() as f32);
                let mut d: Vec<f32> = members.iter().map(|&s| ((pos[s].0 - cx).powi(2) + (pos[s].1 - cy).powi(2)).sqrt()).collect();
                d.sort_by(f32::total_cmp);
                let r = d[(d.len() - 1) * 4 / 5] + 40.0;
                Nebula { name: g.nodes[gn].name.clone(), x: cx, y: cy, r, color: genre_color(&g.nodes[gn].name), listened: g.nodes[gn].listened, stars: members.len() }
            })
            .collect();
        Universe { g, stars, pos, radius, color, links, ladj, nebulas, star_of }
    }

    /// Звезда узла: сам артист; у трека и альбома — главный артист
    pub fn star_for(&self, node: usize) -> Option<usize> {
        if let Some(&s) = self.star_of.get(&node) {
            return Some(s);
        }
        let g = &self.g;
        let n = g.nodes.get(node)?;
        let via = |i: usize, rel: Rel| g.adj[i].iter().filter(|&&e| g.edges[e].rel == rel).max_by(|&&x, &&y| g.edges[x].w.total_cmp(&g.edges[y].w)).map(|&e| g.other(e, i));
        match n.kind {
            Kind::Track => via(node, Rel::Performs).and_then(|a| self.star_of.get(&a).copied()),
            Kind::Album => via(node, Rel::AlbumBy).and_then(|a| self.star_of.get(&a).copied()),
            _ => None,
        }
    }

    /// Что рисовать при приближении zoom: самые слушаемые звёзды; focus — всегда
    pub fn visible(&self, zoom: f32, focus: &HashSet<usize>) -> Vec<usize> {
        let cap = match zoom {
            z if z < 0.4 => 150,
            z if z < 0.9 => 400,
            _ => 1200,
        };
        let mut v: Vec<usize> = (0..self.stars.len().min(cap)).collect();
        v.extend(focus.iter().copied().filter(|&s| s >= cap && s < self.stars.len()));
        v
    }

    /// Линии между видимыми звёздами: у каждой — её самые сильные (до per), разрешённого происхождения.
    /// Четыре пути SVG: факты, чужой алгоритм, вычислено, подсвеченные (hi)
    pub fn paths(&self, vis: &[usize], per: usize, origins: &HashSet<Origin>, hi: &HashSet<usize>) -> [String; 4] {
        use std::fmt::Write;
        let on: HashSet<usize> = vis.iter().copied().collect();
        let mut drawn = HashSet::new();
        let mut out: [String; 4] = Default::default();
        let put = |li: usize, slot: usize, out: &mut [String; 4]| {
            let l = &self.links[li];
            let (a, b) = (self.pos[l.a], self.pos[l.b]);
            let _ = write!(out[slot], "M{:.1} {:.1}L{:.1} {:.1}", a.0, a.1, b.0, b.1);
        };
        for &li in hi {
            if drawn.insert(li) {
                put(li, 3, &mut out);
            }
        }
        for &s in vis {
            for &li in self.ladj[s].iter().filter(|&&li| origins.contains(&self.links[li].origin)).take(per) {
                let l = &self.links[li];
                if !on.contains(&l.a) || !on.contains(&l.b) || !drawn.insert(li) {
                    continue;
                }
                put(li, rank(l.origin) as usize, &mut out);
            }
        }
        out
    }

    /// Поиск: артисты, жанры и треки по имени — сначала начинающиеся с запроса, слушаемые первыми
    pub fn search(&self, q: &str, limit: usize) -> Vec<usize> {
        let q = q.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u8, u64, usize)> = Vec::new();
        for (i, n) in self.g.nodes.iter().enumerate() {
            if !matches!(n.kind, Kind::Artist | Kind::Genre | Kind::Track) {
                continue;
            }
            let name = n.name.to_lowercase();
            let k = if name == q {
                0
            } else if name.starts_with(&q) {
                1
            } else if name.contains(&q) {
                2
            } else {
                continue;
            };
            hits.push((k, u64::MAX - n.listened, i));
        }
        hits.sort();
        hits.into_iter().take(limit).map(|h| h.2).collect()
    }

    /// Рамка неба: (центр x, центр y, ширина, высота) по главным звёздам
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let take = self.pos.len().min(300);
        if take == 0 {
            return (0.0, 0.0, 400.0, 400.0);
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &self.pos[..take] {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0).max(200.0) + 80.0, (y1 - y0).max(200.0) + 80.0)
    }
}

fn ladj_len(l: &[Vec<usize>], s: usize) -> usize {
    l.get(s).map_or(0, |v| v.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Input;
    use crate::model::Track;
    use crate::musicbrainz::{Credit, MbArtist, MbRec, Person};

    fn lib() -> Universe {
        let t = |i: usize, a: &str| (Track { id: format!("{i}"), title: format!("song {i}"), artist: a.into(), ..Default::default() }, (i as u64 + 1) * 60_000, 1);
        let mut tracks = Vec::new();
        for i in 0..40 {
            tracks.push(t(i, ["A", "B", "C", "D & A", "E"][i % 5]));
        }
        let mb = |n: &str, g: &str| (n.to_string(), MbArtist { id: n.into(), name: n.into(), genres: vec![(g.into(), 5)], ..Default::default() });
        let prod = Person { id: "p".into(), name: "Rick".into() };
        let rec = MbRec { id: "r".into(), credits: vec![Credit { role: "producer".into(), detail: String::new(), who: prod }], ..Default::default() };
        let inp = Input {
            mb_artists: [mb("A", "rock"), mb("B", "rock"), mb("C", "jazz"), mb("E", "jazz")].into(),
            mb_recs: [(tracks[1].0.key(), rec.clone()), (tracks[2].0.key(), rec)].into(),
            tracks,
            ..Default::default()
        };
        Universe::new(Graph::build(&inp))
    }

    #[test]
    fn sky_of_artists() {
        let u = lib();
        assert_eq!(u.stars.len(), 5, "звёзды — только артисты");
        assert!(u.pos.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
        assert_eq!(lib().pos, u.pos, "одна библиотека — одна раскладка");
        let s = |n: &str| u.star_of[&u.g.find(&Graph::artist_id(n)).unwrap()];
        // фит A и D — одна линия; B и C — общий продюсер
        let link = |a: usize, b: usize| u.links.iter().find(|l| (l.a, l.b) == (a.min(b), a.max(b)));
        assert!(link(s("A"), s("D")).unwrap().why.iter().any(|r| r.rel == Rel::Feat));
        let bc = link(s("B"), s("C")).expect("общий продюсер");
        assert!(bc.why.iter().any(|r| r.rel == Rel::Producer && r.note == "Rick"));
        assert_eq!(bc.origin, Origin::Confirmed);
        // туманности: rock (A, B) и jazz (C, E)
        assert_eq!(u.nebulas.len(), 2);
        // трек ведёт к звезде своего артиста
        let t0 = u.g.find("t:yt:0").unwrap();
        assert_eq!(u.star_for(t0), Some(s("A")));
        assert!(!u.search("so", 5).is_empty());
        let vis = u.visible(0.2, &HashSet::new());
        let p = u.paths(&vis, 3, &[Origin::Confirmed, Origin::External, Origin::Derived].into(), &HashSet::new());
        assert!(p[0].starts_with('M'));
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
