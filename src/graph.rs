//! Общий граф музыкальных связей — на нём стоят Album Universe, Music Graph, мосты и Taste Fingerprint.
//! Узлы: треки, артисты, альбомы, жанры, люди (продюсеры, музыканты), лейблы.
//! У каждой связи — тип, сила 0..1 и происхождение: подтверждённый факт из метаданных, чужой алгоритм
//! (YouTube «Похожие») или наш расчёт на настоящих данных (звук, плейлисты). Случайных связей нет:
//! чего нет в данных — того нет и в графе.

use std::collections::{BinaryHeap, HashMap, HashSet};

use serde::Serialize;

use crate::dna::{self, Dna};
use crate::model::Track;
use crate::musicbrainz::{MbArtist, MbRec};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Kind {
    Track,
    Artist,
    Album,
    Genre,
    Person,
    Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Rel {
    /// артист исполняет трек
    Performs,
    /// трек из альбома
    OnAlbum,
    /// альбом артиста
    AlbumBy,
    /// артисты вместе в одном треке
    Feat,
    /// участник группы
    Member,
    /// совместный проект, сессионный музыкант
    Collab,
    /// продюсер записи
    Producer,
    /// звукорежиссёр, сведение
    Engineer,
    /// играл или пел на записи
    Musician,
    /// альбом вышел на лейбле
    Label,
    /// жанр артиста
    Genre,
    /// YouTube Music: «Поклонникам нравится»
    YtSimilar,
    /// часто вместе в плейлистах юзера
    CoPlaylist,
    /// похожи по звуку (Music DNA)
    Sound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Origin {
    /// факт из метаданных: YouTube Music (исполнитель), MusicBrainz (кредиты, участники, лейблы)
    Confirmed,
    /// чужой алгоритм рекомендаций
    External,
    /// посчитано нами по настоящим данным
    Derived,
}

impl Rel {
    pub fn origin(self) -> Origin {
        match self {
            Rel::YtSimilar => Origin::External,
            Rel::CoPlaylist | Rel::Sound => Origin::Derived,
            _ => Origin::Confirmed,
        }
    }

    pub fn source(self) -> &'static str {
        match self {
            Rel::Performs | Rel::Feat => "метаданные трека",
            Rel::YtSimilar => "YouTube Music",
            Rel::CoPlaylist => "твои плейлисты",
            Rel::Sound => "Music DNA",
            _ => "MusicBrainz",
        }
    }

    pub fn all() -> [Rel; 14] {
        use Rel::*;
        [Performs, OnAlbum, AlbumBy, Feat, Member, Collab, Producer, Engineer, Musician, Label, Genre, YtSimilar, CoPlaylist, Sound]
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub cover: String,
    /// сколько прослушано, мс (для артиста — сумма по трекам; для жанра — по артистам)
    pub listened: u64,
    pub plays: u32,
    /// трек, если узел — трек
    #[serde(skip)]
    pub track: Option<Track>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub rel: Rel,
    /// сила 0..1
    pub w: f32,
    /// пояснение для человека: «продюсер», «guitar», «в 3 общих плейлистах», «ритм 91 %»…
    pub note: String,
}

#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub adj: Vec<Vec<usize>>,
    idx: HashMap<String, usize>,
    pair: HashSet<(usize, usize, Rel)>,
}

/// Всё, из чего строится граф. Собирается на UI-потоке из Store и Stats, строится в фоне
#[derive(Default, Clone)]
pub struct Input {
    /// трек, прослушано мс, раз
    pub tracks: Vec<(Track, u64, u32)>,
    /// ключи треков каждого плейлиста
    pub playlists: Vec<Vec<String>>,
    pub dna: HashMap<String, Dna>,
    pub mb_artists: HashMap<String, MbArtist>,
    pub mb_recs: HashMap<String, MbRec>,
    /// артист → «Поклонникам нравится» (имена) из кэша страниц артистов
    pub yt_similar: HashMap<String, Vec<String>>,
}

pub fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Все исполнители трека: из поля артиста и из «(feat. X)» в названии
pub fn track_artists(t: &Track) -> Vec<String> {
    let mut v: Vec<String> = crate::stats::artists(&t.artist).into_iter().map(String::from).collect();
    let low = t.title.to_lowercase();
    for tag in ["feat. ", "ft. ", "featuring ", "with "] {
        for open in ['(', '['] {
            if let Some(i) = low.find(&format!("{open}{tag}")) {
                let rest = &t.title[i + 1 + tag.len()..];
                let end = rest.find([')', ']']).unwrap_or(rest.len());
                v.extend(crate::stats::artists(&rest[..end]).into_iter().map(String::from));
            }
        }
    }
    let mut seen = HashSet::new();
    v.retain(|a| seen.insert(norm(a)));
    v
}

impl Graph {
    fn node(&mut self, id: String, kind: Kind, name: &str) -> usize {
        if let Some(&i) = self.idx.get(&id) {
            return i;
        }
        let i = self.nodes.len();
        self.nodes.push(Node { id: id.clone(), kind, name: name.into(), cover: String::new(), listened: 0, plays: 0, track: None });
        self.adj.push(Vec::new());
        self.idx.insert(id, i);
        i
    }

    /// Связь a–b данного типа одна; повтор только усиливает её
    fn link(&mut self, a: usize, b: usize, rel: Rel, w: f32, note: impl Into<String>) {
        if a == b {
            return;
        }
        let k = (a.min(b), a.max(b), rel);
        if !self.pair.insert(k) {
            if let Some(&e) = self.adj[a].iter().find(|&&e| self.edges[e].rel == rel && (self.edges[e].a == b || self.edges[e].b == b)) {
                self.edges[e].w = self.edges[e].w.max(w);
            }
            return;
        }
        let e = self.edges.len();
        self.edges.push(Edge { a, b, rel, w: w.clamp(0.0, 1.0), note: note.into() });
        self.adj[a].push(e);
        self.adj[b].push(e);
    }

    pub fn find(&self, id: &str) -> Option<usize> {
        self.idx.get(id).copied()
    }

    pub fn artist_id(name: &str) -> String {
        format!("a:{}", norm(name))
    }

    pub fn other(&self, e: usize, from: usize) -> usize {
        let e = &self.edges[e];
        if e.a == from { e.b } else { e.a }
    }

    pub fn build(inp: &Input) -> Graph {
        let mut g = Graph::default();
        let artist = |g: &mut Graph, name: &str| g.node(Graph::artist_id(name), Kind::Artist, name);

        // треки и их исполнители — факт из метаданных
        for (t, ms, plays) in &inp.tracks {
            let ti = g.node(format!("t:{}", t.key()), Kind::Track, &t.title);
            g.nodes[ti].cover = t.cover.clone();
            g.nodes[ti].listened += ms;
            g.nodes[ti].plays += plays;
            g.nodes[ti].track = Some(t.clone());
            let names = track_artists(t);
            let ids: Vec<usize> = names.iter().map(|n| artist(&mut g, n)).collect();
            for (k, &ai) in ids.iter().enumerate() {
                g.nodes[ai].listened += ms;
                g.nodes[ai].plays += plays;
                if g.nodes[ai].cover.is_empty() && k == 0 {
                    g.nodes[ai].cover = t.cover.clone();
                }
                g.link(ai, ti, Rel::Performs, if k == 0 { 1.0 } else { 0.7 }, if k == 0 { "исполнитель" } else { "приглашённый артист" });
            }
            for i in 0..ids.len() {
                for j in i + 1..ids.len() {
                    g.link(ids[i], ids[j], Rel::Feat, 0.9, format!("вместе в «{}»", t.title));
                }
            }
        }

        // MusicBrainz: артисты — жанры, участники, проекты
        for a in inp.mb_artists.values().filter(|a| !a.id.is_empty()) {
            let Some(ai) = g.find(&Graph::artist_id(&a.name)) else { continue };
            let top = a.genres.first().map_or(1, |g| g.1.max(1)) as f32;
            for (gname, n) in a.genres.iter().take(6) {
                let gi = g.node(format!("g:{}", norm(gname)), Kind::Genre, gname);
                g.link(ai, gi, Rel::Genre, (*n as f32 / top).max(0.2), format!("{n} голосов редакторов"));
            }
            for p in &a.members {
                let pi = artist(&mut g, &p.name);
                g.link(ai, pi, Rel::Member, 1.0, "участник");
            }
            for p in &a.collabs {
                let pi = artist(&mut g, &p.name);
                g.link(ai, pi, Rel::Collab, 0.8, "совместный проект");
            }
        }
        // жанр слушается столько, сколько его артисты
        for i in 0..g.nodes.len() {
            if g.nodes[i].kind != Kind::Genre {
                continue;
            }
            let (ms, plays) = g.adj[i].iter().map(|&e| g.other(e, i)).fold((0, 0), |(m, p), a| (m + g.nodes[a].listened, p + g.nodes[a].plays));
            g.nodes[i].listened = ms;
            g.nodes[i].plays = plays;
        }

        // MusicBrainz: записи — альбом, лейбл, продюсеры и музыканты
        for (key, r) in inp.mb_recs.iter().filter(|(_, r)| !r.id.is_empty()) {
            let Some(ti) = g.find(&format!("t:{key}")) else { continue };
            let main = g.adj[ti].iter().find(|&&e| g.edges[e].rel == Rel::Performs && g.edges[e].w >= 1.0).map(|&e| g.other(e, ti));
            if let Some((gid, title)) = &r.album {
                let al = g.node(format!("al:{gid}"), Kind::Album, title);
                if g.nodes[al].cover.is_empty() {
                    g.nodes[al].cover = g.nodes[ti].cover.clone();
                }
                let (ms, plays) = (g.nodes[ti].listened, g.nodes[ti].plays);
                g.nodes[al].listened += ms;
                g.nodes[al].plays += plays;
                g.link(ti, al, Rel::OnAlbum, 1.0, r.album_kind.clone());
                if let Some(m) = main {
                    g.link(al, m, Rel::AlbumBy, 1.0, "альбом");
                }
                if let Some(l) = &r.label {
                    let li = g.node(format!("l:{}", l.id), Kind::Label, &l.name);
                    g.link(al, li, Rel::Label, 1.0, "лейбл");
                }
            }
            for c in &r.credits {
                let pi = g.find(&Graph::artist_id(&c.who.name)).unwrap_or_else(|| g.node(format!("p:{}", c.who.id), Kind::Person, &c.who.name));
                let (rel, note) = match c.role.as_str() {
                    "producer" | "co-producer" => (Rel::Producer, c.role.clone()),
                    "mix" | "recording" | "engineer" => (Rel::Engineer, c.role.clone()),
                    _ => (Rel::Musician, if c.detail.is_empty() { c.role.clone() } else { c.detail.clone() }),
                };
                g.link(pi, ti, rel, 1.0, note);
            }
        }

        // YouTube «Поклонникам нравится» — только между артистами, которые уже есть в графе
        for (a, sim) in &inp.yt_similar {
            let Some(ai) = g.find(&Graph::artist_id(a)) else { continue };
            for (rank, s) in sim.iter().enumerate() {
                if let Some(bi) = g.find(&Graph::artist_id(s)) {
                    g.link(ai, bi, Rel::YtSimilar, (1.0 - rank as f32 * 0.06).max(0.4), "похожие по YouTube Music");
                }
            }
        }

        // общие плейлисты: мера Жаккара по наборам плейлистов, где артист встречается
        let mut in_pl: HashMap<usize, HashSet<usize>> = HashMap::new();
        for (p, keys) in inp.playlists.iter().enumerate() {
            for k in keys {
                let Some(ti) = g.find(&format!("t:{k}")) else { continue };
                for &e in &g.adj[ti] {
                    if g.edges[e].rel == Rel::Performs {
                        in_pl.entry(g.other(e, ti)).or_default().insert(p);
                    }
                }
            }
        }
        let arts: Vec<(&usize, &HashSet<usize>)> = in_pl.iter().filter(|(_, s)| s.len() >= 2).collect();
        let mut co = Vec::new();
        for i in 0..arts.len() {
            for j in i + 1..arts.len() {
                let both = arts[i].1.intersection(arts[j].1).count();
                if both >= 2 {
                    let jac = both as f32 / arts[i].1.union(arts[j].1).count() as f32;
                    if jac >= 0.3 {
                        co.push((*arts[i].0, *arts[j].0, jac, both));
                    }
                }
            }
        }
        for (a, b, w, n) in co {
            g.link(a, b, Rel::CoPlaylist, w, format!("вместе в {n} плейлистах"));
        }

        // звук: каждому треку — до трёх самых похожих по DNA (если похожи всерьёз)
        let with: Vec<(usize, &Dna)> = inp.dna.iter().filter_map(|(k, d)| Some((g.find(&format!("t:{k}"))?, d))).collect();
        let mut sound = Vec::new();
        for (i, (ti, a)) in with.iter().enumerate() {
            let mut best: Vec<(f32, usize, dna::Similarity)> = with
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, (tj, b))| {
                    let s = dna::compare(a, b);
                    (s.total, *tj, s)
                })
                .filter(|x| x.0 >= 0.8)
                .collect();
            best.sort_by(|x, y| y.0.total_cmp(&x.0));
            for (w, tj, s) in best.into_iter().take(3) {
                sound.push((*ti, tj, w, format!("ритм {:.0} %, гармония {:.0} %, тембр {:.0} %", s.rhythm * 100.0, s.harmony * 100.0, s.timbre * 100.0)));
            }
        }
        for (a, b, w, n) in sound {
            g.link(a, b, Rel::Sound, w, n);
        }
        g
    }

    /// Окружение узла: связи до depth шагов, только разрешённых типов. Возвращает узлы и рёбра
    pub fn around(&self, from: usize, depth: usize, allow: &HashSet<Rel>, limit: usize) -> (Vec<usize>, Vec<usize>) {
        let mut seen = HashSet::from([from]);
        let mut nodes = vec![from];
        let mut edges = Vec::new();
        let mut front = vec![from];
        for _ in 0..depth {
            let mut next = Vec::new();
            for &n in &front {
                // сильные связи первыми — при обрезке по limit остаются они
                let mut es: Vec<usize> = self.adj[n].iter().copied().filter(|&e| allow.contains(&self.edges[e].rel)).collect();
                es.sort_by(|&x, &y| self.edges[y].w.total_cmp(&self.edges[x].w));
                for e in es {
                    let o = self.other(e, n);
                    if seen.insert(o) {
                        if nodes.len() >= limit {
                            continue;
                        }
                        nodes.push(o);
                        next.push(o);
                    }
                    if seen.contains(&o) && nodes.contains(&o) {
                        edges.push(e);
                    }
                }
            }
            front = next;
        }
        edges.sort_unstable();
        edges.dedup();
        (nodes, edges)
    }

    /// Мост между двумя узлами: самая «сильная» цепочка не длиннее max_depth шагов.
    /// Цена шага = 1 + (1 − сила), так что короткие и крепкие пути выигрывают.
    /// Подтверждённые связи дешевле вычисленных: мост из фактов важнее моста из догадок
    pub fn bridge(&self, from: usize, to: usize, max_depth: usize, allow: &HashSet<Rel>, avoid: &HashSet<usize>) -> Option<Vec<usize>> {
        #[derive(PartialEq)]
        struct St(f32, usize, usize);
        impl Eq for St {}
        impl PartialOrd for St {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for St {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.total_cmp(&self.0)
            }
        }
        // состояние — (узел, глубина): без глубины в состоянии ограничение длины ломало бы Дейкстру
        let mut best: HashMap<(usize, usize), (f32, Option<(usize, usize, usize)>)> = HashMap::new();
        let mut heap = BinaryHeap::from([St(0.0, from, 0)]);
        best.insert((from, 0), (0.0, None));
        while let Some(St(cost, n, d)) = heap.pop() {
            if n == to {
                let mut path = Vec::new();
                let mut cur = (n, d);
                while let Some((pn, pd, e)) = best[&cur].1 {
                    path.push(e);
                    cur = (pn, pd);
                }
                path.reverse();
                return Some(path);
            }
            if d >= max_depth || best.get(&(n, d)).is_some_and(|b| b.0 < cost) {
                continue;
            }
            for &e in &self.adj[n] {
                let ed = &self.edges[e];
                if !allow.contains(&ed.rel) {
                    continue;
                }
                let o = self.other(e, n);
                if avoid.contains(&o) && o != to {
                    continue;
                }
                // через трек или жанр проходить можно, но это «шаг вбок» — чуть дороже
                let side = if matches!(self.nodes[o].kind, Kind::Genre) && o != to { 0.6 } else { 0.0 };
                let trust = match ed.rel.origin() {
                    Origin::Confirmed => 0.0,
                    Origin::Derived => 1.0,
                    Origin::External => 1.5,
                };
                let c = cost + 1.0 + (1.0 - ed.w) + side + trust;
                let k = (o, d + 1);
                if best.get(&k).is_none_or(|b| c < b.0) {
                    best.insert(k, (c, Some((n, d, e))));
                    heap.push(St(c, o, d + 1));
                }
            }
        }
        None
    }

    /// Несколько разных мостов: каждый следующий обходит промежуточные узлы предыдущих
    pub fn bridges(&self, from: usize, to: usize, max_depth: usize, allow: &HashSet<Rel>, n: usize) -> Vec<Vec<usize>> {
        let mut avoid = HashSet::new();
        let mut out = Vec::new();
        while out.len() < n {
            let Some(p) = self.bridge(from, to, max_depth, allow, &avoid) else { break };
            let mut cur = from;
            for &e in &p {
                cur = self.other(e, cur);
                if cur != to {
                    avoid.insert(cur);
                }
            }
            let direct = p.len() == 1;
            out.push(p);
            if direct {
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::musicbrainz::{Credit, Person};

    fn t(id: &str, title: &str, artist: &str) -> Track {
        Track { id: id.into(), title: title.into(), artist: artist.into(), ..Default::default() }
    }

    #[test]
    fn feats_from_artist_and_title() {
        assert_eq!(track_artists(&t("1", "Song (feat. Bob & Eve)", "Alice")), ["Alice", "Bob", "Eve"]);
        assert_eq!(track_artists(&t("2", "Song", "Alice, Bob feat. Alice")), ["Alice", "Bob"]);
        assert_eq!(track_artists(&t("3", "Song [with Zed]", "Alice")), ["Alice", "Zed"]);
    }

    /// A и B никогда не пересекались, но у обоих один продюсер — мост A → трек → продюсер → трек → B,
    /// а не «похожи по YouTube», хотя такая связь короче — она не факт
    #[test]
    fn bridge_prefers_confirmed() {
        let (ta, tb, tc) = (t("a", "One", "Alpha"), t("b", "Two", "Beta"), t("c", "Three", "Gamma"));
        let prod = Person { id: "p1".into(), name: "Rick".into() };
        let rec = |p: &Person| MbRec { id: "r".into(), credits: vec![Credit { role: "producer".into(), detail: String::new(), who: p.clone() }], ..Default::default() };
        let inp = Input {
            tracks: vec![(ta.clone(), 1000, 1), (tb.clone(), 1000, 1), (tc.clone(), 0, 0)],
            mb_recs: HashMap::from([(ta.key(), rec(&prod)), (tb.key(), rec(&prod))]),
            yt_similar: HashMap::from([("Alpha".to_string(), vec!["Gamma".to_string()]), ("Gamma".to_string(), vec!["Beta".to_string()])]),
            ..Default::default()
        };
        let g = Graph::build(&inp);
        let (a, b) = (g.find("a:alpha").unwrap(), g.find("a:beta").unwrap());
        let all: HashSet<Rel> = Rel::all().into();
        let p = g.bridge(a, b, 6, &all, &HashSet::new()).expect("мост есть");
        let rels: Vec<Rel> = p.iter().map(|&e| g.edges[e].rel).collect();
        assert_eq!(rels, [Rel::Performs, Rel::Producer, Rel::Producer, Rel::Performs], "{rels:?}");
        // без связей MusicBrainz остаётся только путь через «похожих» YouTube
        let only_yt: HashSet<Rel> = [Rel::YtSimilar].into();
        let p = g.bridge(a, b, 6, &only_yt, &HashSet::new()).unwrap();
        assert_eq!(p.len(), 2);
        // глубина 1 — не дотянуться
        assert!(g.bridge(a, b, 1, &all, &HashSet::new()).is_none());
        // второй мост обходит продюсера
        let two = g.bridges(a, b, 6, &all, 3);
        assert_eq!(two.len(), 2);
    }

    #[test]
    fn coplaylist_needs_real_overlap() {
        let tr: Vec<Track> = (0..4).map(|i| t(&i.to_string(), "x", ["A", "B", "C", "D"][i])).collect();
        let k = |i: usize| tr[i].key();
        let inp = Input {
            tracks: tr.iter().map(|t| (t.clone(), 0, 0)).collect(),
            // A и B вместе в трёх плейлистах, C с ними только в одном, D один
            playlists: vec![vec![k(0), k(1)], vec![k(0), k(1), k(2)], vec![k(0), k(1)], vec![k(3)], vec![k(2)], vec![k(2)]],
            ..Default::default()
        };
        let g = Graph::build(&inp);
        let co: Vec<(String, String)> = g.edges.iter().filter(|e| e.rel == Rel::CoPlaylist).map(|e| (g.nodes[e.a].name.clone(), g.nodes[e.b].name.clone())).collect();
        assert_eq!(co.len(), 1, "{co:?}");
        assert!(co[0] == ("A".into(), "B".into()) || co[0] == ("B".into(), "A".into()));
    }
}
