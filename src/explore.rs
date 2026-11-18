//! Исследование графа без интерфейса — общее для ПК (uview.rs) и телефона (mobile/core/src/api.rs):
//! треки за узлом, его Music DNA, связи по порядку, похожее и непохожее по звучанию, шаги мостов.
//! Здесь только данные; подписи на нужном языке делает тот, кто рисует.

use std::collections::HashSet;

use serde::Serialize;

use crate::dna::{self, Dna, Similarity};
use crate::graph::{Graph, Kind, Origin, Rel};
use crate::model::Track;

/// Треки за узлом: сам трек, треки артиста, альбома, человека; жанра — через его артистов, лейбла — через альбомы.
/// Самые слушаемые первыми
pub fn tracks_of(g: &Graph, i: usize) -> Vec<Track> {
    if i >= g.nodes.len() {
        return Vec::new();
    }
    let mut out: Vec<(u64, Track)> = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |t: usize, out: &mut Vec<(u64, Track)>| {
        if let Some(tr) = &g.nodes[t].track {
            if seen.insert(t) {
                out.push((g.nodes[t].listened, tr.clone()));
            }
        }
    };
    let near = |n: usize| -> Vec<usize> { g.adj[n].iter().map(|&e| g.other(e, n)).filter(|&o| g.nodes[o].kind == Kind::Track).collect() };
    match g.nodes[i].kind {
        Kind::Track => add(i, &mut out),
        Kind::Genre | Kind::Label => {
            for &e in &g.adj[i] {
                for t in near(g.other(e, i)) {
                    add(t, &mut out);
                }
            }
        }
        _ => {
            for t in near(i) {
                add(t, &mut out);
            }
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, t)| t).take(200).collect()
}

/// DNA узла и по скольким трекам он посчитан: трек — свой, остальное — среднее по трекам с DNA
pub fn dna_of(g: &Graph, i: usize) -> Option<(Dna, usize)> {
    let ds: Vec<Dna> = tracks_of(g, i).iter().filter_map(|t| dna::get(&t.key())).collect();
    match ds.len() {
        0 => None,
        1 => Some((ds[0].clone(), 1)),
        n => Some((dna::mean(&ds.iter().collect::<Vec<_>>())?, n)),
    }
}

/// Восемь осей отпечатка 0..1 (нормировка настоящих признаков для рисования):
/// энергия, бит, темп, яркость, шум, динамика, атаки, низ
pub fn radar(d: &Dna) -> [f32; 8] {
    [d.energy, d.pulse, d.bpm.map_or(0.0, |b| (b - 60.0) / 140.0), d.centroid / 4000.0, d.flatness * 3.0, d.dyn_db / 18.0, d.onsets / 8.0, d.bands[0] * 2.0].map(|x| x.clamp(0.0, 1.0))
}

/// Многоугольник радара в квадрате 0..200 (путь SVG)
pub fn radar_path(v: &[f32; 8]) -> String {
    let mut s = String::new();
    for (k, x) in v.iter().enumerate() {
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::FRAC_PI_4;
        let r = 80.0 * x.clamp(0.04, 1.0);
        s += &format!("{}{:.1} {:.1}", if k == 0 { "M" } else { "L" }, 100.0 + r * a.cos(), 100.0 + r * a.sin());
    }
    s + "Z"
}

#[derive(Clone, Debug, Serialize)]
pub struct Pick {
    pub node: usize,
    pub sim: Similarity,
    /// две составляющие, которые лучше всего объясняют выбор (номера: ритм, гармония, тембр, энергия, спектр)
    pub why: [usize; 2],
}

fn parts(s: &Similarity) -> [f32; 5] {
    [s.rhythm, s.harmony, s.timbre, s.energy, s.spectrum]
}

fn pick(node: usize, sim: Similarity, best: bool) -> Pick {
    let p = parts(&sim);
    let mut idx: Vec<usize> = (0..5).collect();
    idx.sort_by(|&a, &b| if best { p[b].total_cmp(&p[a]) } else { p[a].total_cmp(&p[b]) });
    Pick { node, sim, why: [idx[0], idx[1]] }
}

/// Похожие по звучанию (6) и «совсем не как обычно» (4) среди узлов того же вида, у которых есть DNA.
/// «Обычно» — твоё среднее звучание, взвешенное временем прослушивания
pub fn neighbors(g: &Graph, i: usize, d: &Dna) -> (Vec<Pick>, Vec<Pick>) {
    let kind = g.nodes[i].kind;
    let pool: Vec<(usize, Dna)> = match kind {
        Kind::Track => g.nodes.iter().enumerate().filter_map(|(j, n)| Some((j, dna::get(&n.track.as_ref()?.key())?))).collect(),
        _ => (0..g.nodes.len()).filter(|&j| g.nodes[j].kind == kind).take(800).filter_map(|j| Some((j, dna_of(g, j)?.0))).collect(),
    };
    let mut sims: Vec<(usize, Similarity)> = pool.iter().filter(|(j, _)| *j != i).map(|(j, o)| (*j, dna::compare(d, o))).collect();
    sims.sort_by(|a, b| b.1.total.total_cmp(&a.1.total));
    let similar = sims.into_iter().take(6).map(|(j, s)| pick(j, s, true)).collect();
    let taste: Vec<(Dna, u64)> = pool.iter().map(|(j, d)| (d.clone(), g.nodes[*j].listened.max(1))).collect();
    let far = match dna::weighted_mean(&taste) {
        Some(t) => {
            let mut v: Vec<(usize, Similarity)> = pool.iter().filter(|(j, _)| *j != i).map(|(j, o)| (*j, dna::compare(&t, o))).collect();
            v.sort_by(|a, b| a.1.total.total_cmp(&b.1.total));
            v.into_iter().take(4).map(|(j, s)| pick(j, s, false)).collect()
        }
        None => Vec::new(),
    };
    (similar, far)
}

/// Какие связи можно брать в мост: факты, YouTube, звук, плейлисты
pub fn bridge_rels(groups: [bool; 4]) -> HashSet<Rel> {
    Rel::all()
        .into_iter()
        .filter(|r| match r {
            Rel::YtSimilar => groups[1],
            Rel::Sound => groups[2],
            Rel::CoPlaylist | Rel::Session => groups[3],
            _ => groups[0],
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
pub struct Step {
    pub node: usize,
    pub name: String,
    pub kind: Kind,
    /// как пришли сюда из прошлого шага (у первого — нет)
    pub rel: Option<Rel>,
    pub note: String,
    pub origin: Option<Origin>,
}

/// Мост по рёбрам → шаги с узлами
pub fn steps(g: &Graph, from: usize, path: &[usize]) -> Vec<Step> {
    let mut cur = from;
    let mut out = vec![Step { node: from, name: g.nodes[from].name.clone(), kind: g.nodes[from].kind, rel: None, note: String::new(), origin: None }];
    for &e in path {
        let ed = &g.edges[e];
        cur = g.other(e, cur);
        out.push(Step { node: cur, name: g.nodes[cur].name.clone(), kind: g.nodes[cur].kind, rel: Some(ed.rel), note: ed.note.clone(), origin: Some(ed.rel.origin()) });
    }
    out
}
