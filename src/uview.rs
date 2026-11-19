//! Экран «Вселенная» на ПК: небо артистов (universe.rs), выбранная звезда и её панель, перелёт камеры,
//! мосты и Music DNA. Номера: звёзды — индексы в Universe::stars, всё остальное — узлы графа.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::dna::{self, Dna};
use crate::graph::{Kind, Origin, Rel};
use crate::model::Track;
use crate::universe::Universe;
use crate::{AppWindow, DnaPick, DnaRow, RadarLabel, UBridge, UNeb, UNode, URel, UStep, Uni, LANG};

fn kind_num(k: Kind) -> i32 {
    match k {
        Kind::Track => 0,
        Kind::Artist => 1,
        Kind::Album => 2,
        Kind::Genre => 3,
        Kind::Person => 4,
        Kind::Label => 5,
    }
}

pub fn kind_name(k: Kind) -> String {
    match k {
        Kind::Track => tr!("ТРЕК", "TRACK", "ТРЕК", "SKLADBA"),
        Kind::Artist => tr!("АРТИСТ", "ARTIST", "АРТИСТ", "INTERPRET"),
        Kind::Album => tr!("АЛЬБОМ", "ALBUM", "АЛЬБОМ", "ALBUM"),
        Kind::Genre => tr!("ЖАНР", "GENRE", "ЖАНР", "ŽÁNER"),
        Kind::Person => tr!("ЧЕЛОВЕК ЗА ЗАПИСЬЮ", "PERSON BEHIND THE RECORD", "ЛЮДИНА ЗА ЗАПИСОМ", "ČLOVEK ZA NAHRÁVKOU"),
        Kind::Label => tr!("ЛЕЙБЛ", "LABEL", "ЛЕЙБЛ", "VYDAVATEĽSTVO"),
    }
}

pub fn rel_name(r: Rel) -> String {
    match r {
        Rel::Performs => tr!("исполнитель", "performer", "виконавець", "interpret"),
        Rel::OnAlbum => tr!("трек альбома", "album track", "трек альбому", "skladba albumu"),
        Rel::AlbumBy => tr!("альбом артиста", "artist's album", "альбом артиста", "album interpreta"),
        Rel::Feat => tr!("вместе в треке", "together on a track", "разом у треку", "spolu v skladbe"),
        Rel::Member => tr!("участник группы", "band member", "учасник гурту", "člen skupiny"),
        Rel::Collab => tr!("совместный проект", "collaboration", "спільний проєкт", "spolupráca"),
        Rel::Producer => tr!("продюсер", "producer", "продюсер", "producent"),
        Rel::Engineer => tr!("звукорежиссёр", "engineer", "звукорежисер", "zvukový majster"),
        Rel::Musician => tr!("играл на записи", "played on the record", "грав на записі", "hral na nahrávke"),
        Rel::Label => tr!("лейбл", "label", "лейбл", "vydavateľstvo"),
        Rel::Genre => tr!("жанр", "genre", "жанр", "žáner"),
        Rel::YtSimilar => tr!("похожие артисты", "similar artists", "схожі артисти", "podobní interpreti"),
        Rel::CoPlaylist => tr!("вместе в твоих плейлистах", "together in your playlists", "разом у твоїх плейлистах", "spolu v tvojich playlistoch"),
        Rel::Sound => tr!("похожи по звуку", "sound alike", "схожі за звуком", "znejú podobne"),
        Rel::Session => tr!("слушаешь подряд", "played back to back", "слухаєш поспіль", "počúvaš za sebou"),
    }
}

pub fn rel_src(r: Rel) -> String {
    match r {
        Rel::Performs | Rel::Feat => tr!("метаданные трека", "track metadata", "метадані треку", "metadáta skladby"),
        Rel::YtSimilar => "YouTube Music".into(),
        Rel::CoPlaylist => tr!("твои плейлисты", "your playlists", "твої плейлисти", "tvoje playlisty"),
        Rel::Sound => "Music DNA".into(),
        Rel::Session => tr!("твоя история", "your history", "твоя історія", "tvoja história"),
        _ => "MusicBrainz".into(),
    }
}

fn origin_num(o: Origin) -> i32 {
    match o {
        Origin::Confirmed => 0,
        Origin::External => 1,
        Origin::Derived => 2,
    }
}

fn color(c: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

fn dur(ms: u64) -> String {
    let m = ms / 60_000;
    if m >= 60 {
        tr!("{} ч {} мин", "{} h {} min", "{} год {} хв", "{} h {} min", m / 60, m % 60)
    } else {
        tr!("{} мин", "{} min", "{} хв", "{} min", m)
    }
}

/// Уровень подробности: меняется — пересобираем, что видно
fn bucket(z: f32) -> u8 {
    match z {
        z if z < 0.35 => 0,
        z if z < 0.8 => 1,
        z if z < 1.6 => 2,
        _ => 3,
    }
}

pub struct View {
    pub u: Option<Universe>,
    /// выбранная звезда
    sel: Option<usize>,
    origins: HashSet<Origin>,
    vis: Vec<usize>,
    nodes: Rc<VecModel<UNode>>,
    row_of: HashMap<usize, usize>,
    /// треки в панели выбранной звезды (кнопка у строки играет с неё)
    panel_tracks: Vec<Track>,
    br_a: Option<usize>,
    br_b: Option<usize>,
    br_rels: [bool; 4],
    bridges: Vec<Vec<usize>>,
    br_shown: Option<usize>,
    dna_node: Option<usize>,
    dna_cmp: Option<usize>,
    cmp_pick: bool,
    anim: Timer,
    fly: ((f32, f32, f32), (f32, f32, f32), f32),
    pub building: bool,
    first: bool,
    /// «Мой вкус»: id узлов артистов и жанров из отпечатка (переживают перестройку графа)
    taste_ids: Vec<String>,
    taste_on: bool,
    /// фон: таймер 15 кадров/с, пока страница на экране
    bg: Timer,
    bg_t: f32,
}

/// Звёздная пыль: кружки с периодом 2000 по обеим осям (повторены сеткой 4×4, чтобы перенос был без шва)
fn dust() -> String {
    use std::fmt::Write;
    let mut seed = 0x2545F491u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    let dots: Vec<(f32, f32, f32)> = (0..140).map(|_| (rnd() * 2000.0, rnd() * 2000.0, 0.6 + rnd() * 1.1)).collect();
    let mut s = String::new();
    for ox in [-2000.0f32, 0.0, 2000.0, 4000.0] {
        for oy in [-2000.0f32, 0.0, 2000.0, 4000.0] {
            for &(x, y, r) in &dots {
                let _ = write!(s, "M{:.0} {:.0}m-{r:.1} 0a{r:.1} {r:.1} 0 1 0 {:.1} 0a{r:.1} {r:.1} 0 1 0 -{:.1} 0", x + ox, y + oy, r * 2.0, r * 2.0);
            }
        }
    }
    s
}

impl Default for View {
    fn default() -> Self {
        View {
            u: None,
            sel: None,
            origins: [Origin::Confirmed, Origin::External, Origin::Derived].into(),
            vis: Vec::new(),
            nodes: Rc::new(VecModel::default()),
            row_of: HashMap::new(),
            panel_tracks: Vec::new(),
            br_a: None,
            br_b: None,
            br_rels: [true; 4],
            bridges: Vec::new(),
            br_shown: None,
            dna_node: None,
            dna_cmp: None,
            cmp_pick: false,
            anim: Timer::default(),
            fly: ((0.0, 0.0, 1.0), (0.0, 0.0, 1.0), 1.0),
            building: false,
            first: true,
            taste_ids: Vec::new(),
            taste_on: false,
            bg: Timer::default(),
            bg_t: 0.0,
        }
    }
}

/// Сколько сильнейших линий рисовать у каждой звезды
const PER_STAR: usize = 3;

impl View {
    fn uni<'a>(&self, ui: &'a AppWindow) -> Uni<'a> {
        ui.global::<Uni>()
    }

    /// Новое небо: выбор, мосты и DNA переносим по id узлов (номера в новом графе другие)
    pub fn set(&mut self, ui: &AppWindow, u: Universe) {
        let node_id = |i: Option<usize>| -> Option<String> { Some(self.u.as_ref()?.g.nodes.get(i?)?.id.clone()) };
        let sel_id = self.sel.and_then(|s| node_id(self.u.as_ref().and_then(|u| u.stars.get(s).copied())));
        let (a, b, d, c) = (node_id(self.br_a), node_id(self.br_b), node_id(self.dna_node), node_id(self.dna_cmp));
        self.sel = sel_id.and_then(|id| u.star_of.get(&u.g.find(&id)?).copied());
        self.br_a = a.and_then(|id| u.g.find(&id));
        self.br_b = b.and_then(|id| u.g.find(&id));
        self.dna_node = d.and_then(|id| u.g.find(&id));
        self.dna_cmp = c.and_then(|id| u.g.find(&id));
        self.bridges.clear();
        self.br_shown = None;
        let uni = self.uni(ui);
        let nebs: Vec<UNeb> = u.nebulas.iter().map(|n| UNeb { x: n.x, y: n.y, r: n.r, c: color(n.color), label: n.name.as_str().into() }).collect();
        uni.set_nebulas(model(nebs));
        uni.set_nodes(self.nodes.clone().into());
        uni.set_bridges(model(Vec::new()));
        uni.set_br_shown(-1);
        self.u = Some(u);
        if self.first {
            self.first = false;
            self.fit(ui);
        }
        self.refresh(ui);
        self.panel(ui);
        self.bridge_names(ui);
    }

    /// Фон плывёт, пока страница открыта (в лайт-режиме — стоит)
    pub fn start_bg(&mut self, ui: &AppWindow, tick: fn()) {
        let uni = self.uni(ui);
        if uni.get_dust().is_empty() {
            uni.set_dust(dust().into());
        }
        if !ui.get_lite() && !self.bg.running() {
            self.bg.start(TimerMode::Repeated, Duration::from_millis(66), tick);
        }
    }

    pub fn bg_tick(&mut self, ui: &AppWindow) {
        if ui.get_page() != "universe" || ui.get_lite() {
            self.bg.stop();
            return;
        }
        self.bg_t += 0.066;
        self.uni(ui).set_bg_t(self.bg_t);
    }

    /// Номер звезды из UI — только если такая есть
    fn star(&self, id: i32) -> Option<usize> {
        let n = self.u.as_ref()?.stars.len();
        (id >= 0 && (id as usize) < n).then_some(id as usize)
    }

    /// Узел графа из UI — только если такой есть (у строк без узла id = -1)
    fn node(&self, id: i32) -> Option<usize> {
        let n = self.u.as_ref()?.g.nodes.len();
        (id >= 0 && (id as usize) < n).then_some(id as usize)
    }

    /// Что подсвечено: звёзды показанного моста; иначе выбранная и её связи; иначе «мой вкус».
    /// Возвращает звёзды и номера связей для подсветки
    fn focus(&self) -> (HashSet<usize>, HashSet<usize>) {
        let mut stars = HashSet::new();
        let mut links = HashSet::new();
        let Some(u) = &self.u else { return (stars, links) };
        if let Some(b) = self.br_shown.and_then(|i| self.bridges.get(i)) {
            let mut cur = self.br_a.unwrap_or(0);
            let mut prev: Option<usize> = cur.lt(&u.g.nodes.len()).then(|| u.star_for(cur)).flatten();
            if let Some(p) = prev {
                stars.insert(p);
            }
            for &e in b {
                cur = u.g.other(e, cur);
                if let Some(s) = u.star_for(cur) {
                    stars.insert(s);
                    if let Some(p) = prev.filter(|&p| p != s) {
                        if let Some(&li) = u.ladj[p].iter().find(|&&li| u.links[li].a == s || u.links[li].b == s) {
                            links.insert(li);
                        }
                    }
                    prev = Some(s);
                }
            }
        } else if let Some(s) = self.sel {
            stars.insert(s);
            for &li in u.ladj[s].iter().filter(|&&li| self.origins.contains(&u.links[li].origin)).take(60) {
                let l = &u.links[li];
                stars.insert(if l.a == s { l.b } else { l.a });
                links.insert(li);
            }
        } else if self.taste_on {
            for id in &self.taste_ids {
                if let Some(s) = u.g.find(id).and_then(|n| u.star_of.get(&n)) {
                    stars.insert(*s);
                }
            }
        }
        (stars, links)
    }

    pub fn refresh(&mut self, ui: &AppWindow) {
        let uni = self.uni(ui);
        let Some(u) = &self.u else {
            uni.set_empty(true);
            return;
        };
        uni.set_empty(u.stars.is_empty());
        let (focus, hi) = self.focus();
        let zoom = uni.get_zoom();
        let vis = u.visible(zoom, &focus);
        let dimming = !focus.is_empty();
        // подписи: важные первыми, каждая — только если не налезает на уже поставленные (в пикселях
        // при этом приближении: относительное расположение от сдвига карты не зависит)
        let mut placed: Vec<(f32, f32, f32, f32)> = Vec::new();
        let mut lab = std::collections::HashSet::new();
        let rk = zoom.powf(0.55);
        for &s in &vis {
            if dimming && !focus.contains(&s) {
                continue;
            }
            let name = &u.g.nodes[u.stars[s]].name;
            let w = (name.chars().count().min(22) as f32 * 6.4).min(150.0) + 6.0;
            let r = (u.radius[s] * rk).max(2.5);
            let (cx, cy) = (u.pos[s].0 * zoom, u.pos[s].1 * zoom + r + 4.0);
            let rect = (cx - w / 2.0, cy, cx + w / 2.0, cy + 15.0);
            let selected = Some(s) == self.sel;
            if selected || !placed.iter().any(|p| rect.0 < p.2 && rect.2 > p.0 && rect.1 < p.3 && rect.3 > p.1) {
                placed.push(rect);
                lab.insert(s);
            }
            if placed.len() >= 70 {
                break;
            }
        }
        let rows: Vec<UNode> = vis
            .iter()
            .map(|&s| UNode {
                x: u.pos[s].0,
                y: u.pos[s].1,
                r: u.radius[s],
                c: color(u.color[s]),
                label: u.g.nodes[u.stars[s]].name.as_str().into(),
                kind: 1,
                sel: Some(s) == self.sel,
                dim: dimming && !focus.contains(&s),
                lab: lab.contains(&s),
                id: s as i32,
            })
            .collect();
        // те же звёзды — меняем только изменившиеся строки: пересоздание сотен элементов и давало лаг при выделении
        if vis == self.vis && self.nodes.row_count() == rows.len() {
            for (i, r) in rows.into_iter().enumerate() {
                if self.nodes.row_data(i).as_ref() != Some(&r) {
                    self.nodes.set_row_data(i, r);
                }
            }
        } else {
            self.row_of = vis.iter().enumerate().map(|(r, &s)| (s, r)).collect();
            self.nodes.set_vec(rows);
        }
        let p = u.paths(&vis, if dimming { 0 } else { PER_STAR }, &self.origins, &hi);
        uni.set_paths(model(p.into_iter().map(SharedString::from).collect()));
        let dna_n = u.g.nodes.iter().filter(|n| n.track.as_ref().is_some_and(|t| dna::get(&t.key()).is_some())).count();
        let genres = u.g.nodes.iter().filter(|n| n.kind == Kind::Genre).count();
        uni.set_status(
            tr!(
                "{} артистов · {} связей · жанров {} · Music DNA: {} треков",
                "{} artists · {} links · {} genres · Music DNA: {} tracks",
                "{} артистів · {} зв'язків · жанрів {} · Music DNA: {} треків",
                "{} interpretov · {} väzieb · žánrov {} · Music DNA: {} skladieb",
                u.stars.len(),
                u.links.len(),
                genres,
                dna_n
            )
            .into(),
        );
        // связей мало — подсказываем, откуда взять ещё
        let lonely = u.ladj.iter().filter(|l| l.is_empty()).count();
        uni.set_sparse(u.stars.len() >= 8 && (lonely * 2 > u.stars.len() || genres == 0));
        self.vis = vis;
    }

    /// Панель справа: выбранный артист — связи (все причины в одной строке), треки, жанры, альбомы, люди
    fn panel(&mut self, ui: &AppWindow) {
        let uni = self.uni(ui);
        let (Some(u), Some(s)) = (&self.u, self.sel) else {
            uni.set_sel(-1);
            self.panel_tracks.clear();
            return;
        };
        let a = u.stars[s];
        let n = &u.g.nodes[a];
        let g = &u.g;
        uni.set_sel(s as i32);
        uni.set_sel_node(a as i32);
        uni.set_sel_name(n.name.as_str().into());
        uni.set_sel_kind(kind_name(Kind::Artist).into());
        uni.set_sel_color(color(u.color[s]));
        let tracks = crate::explore::tracks_of(g, a);
        let mut sub = Vec::new();
        if n.listened > 0 {
            sub.push(tr!("слушал {}", "listened {}", "слухав {}", "počúval si {}", dur(n.listened)));
        }
        if n.plays > 0 {
            sub.push(tr!("{} раз", "{} plays", "{} разів", "{}×", n.plays));
        }
        if let Some((d, k)) = crate::explore::dna_of(g, a) {
            let mut bits = Vec::new();
            if let Some(b) = d.bpm {
                bits.push(format!("~{:.0} BPM", b));
            }
            bits.push(tr!("DNA по {} трекам", "DNA from {} tracks", "DNA за {} треками", "DNA z {} skladieb", k));
            sub.push(bits.join(" · "));
        }
        uni.set_sel_sub(sub.join(" · ").into());
        uni.set_sel_play(!tracks.is_empty());
        uni.set_sel_dna(crate::explore::dna_of(g, a).is_some());
        uni.set_sel_track(tracks.first().is_some_and(crate::rooms::shareable));
        // связанные артисты: одна строка на пару, причины через точку
        let rels: Vec<URel> = u.ladj[s]
            .iter()
            .take(80)
            .map(|&li| {
                let l = &u.links[li];
                let o = if l.a == s { l.b } else { l.a };
                let why: Vec<String> = l
                    .why
                    .iter()
                    .map(|r| match r.note.as_str() {
                        _ if matches!(r.rel, Rel::Feat) => rel_name(r.rel),
                        "" => rel_name(r.rel),
                        n if n.starts_with('×') => format!("{} {n}", rel_name(r.rel)),
                        n => format!("{}: {n}", rel_name(r.rel)),
                    })
                    .collect();
                let mut srcs: Vec<String> = l.why.iter().map(|r| rel_src(r.rel)).collect();
                srcs.dedup();
                URel { name: g.nodes[u.stars[o]].name.as_str().into(), rel: why.join(" · ").into(), note: SharedString::new(), origin: origin_num(l.origin), src: srcs.join(", ").into(), id: o as i32 }
            })
            .collect();
        uni.set_rels(model(rels));
        let trows: Vec<URel> = tracks
            .iter()
            .take(12)
            .enumerate()
            .map(|(i, t)| {
                let ms = g.find(&format!("t:{}", t.key())).map_or(0, |ti| g.nodes[ti].listened);
                URel { name: t.title.as_str().into(), rel: SharedString::new(), note: if ms > 0 { dur(ms).into() } else { SharedString::new() }, origin: 0, src: SharedString::new(), id: i as i32 }
            })
            .collect();
        uni.set_tracks(model(trows));
        uni.set_tracks_total(tracks.len() as i32);
        self.panel_tracks = tracks;
        // жанры, альбомы, люди за записями
        let near = |rel: Rel, from: usize| -> Vec<usize> { g.adj[from].iter().filter(|&&e| g.edges[e].rel == rel).map(|&e| g.other(e, from)).collect() };
        let genres: Vec<String> = near(Rel::Genre, a).into_iter().map(|x| g.nodes[x].name.clone()).collect();
        let albums: Vec<String> = near(Rel::AlbumBy, a).into_iter().map(|x| g.nodes[x].name.clone()).collect();
        let mut people: Vec<String> = Vec::new();
        for t in near(Rel::Performs, a) {
            for &e in &g.adj[t] {
                if matches!(g.edges[e].rel, Rel::Producer | Rel::Engineer | Rel::Musician) {
                    let p = g.nodes[g.other(e, t)].name.clone();
                    if p != n.name && !people.contains(&p) {
                        people.push(p);
                    }
                }
            }
        }
        let mut facts = Vec::new();
        if !genres.is_empty() {
            facts.push(tr!("Жанры: {}", "Genres: {}", "Жанри: {}", "Žánre: {}", genres.join(", ")));
        }
        if !albums.is_empty() {
            facts.push(tr!("Альбомы: {}", "Albums: {}", "Альбоми: {}", "Albumy: {}", albums.join(", ")));
        }
        if !people.is_empty() {
            facts.push(tr!("Люди за записями: {}", "People behind the records: {}", "Люди за записами: {}", "Ľudia za nahrávkami: {}", people.into_iter().take(12).collect::<Vec<_>>().join(", ")));
        }
        uni.set_sel_facts(facts.join("\n").into());
    }

    pub fn pick(&mut self, ui: &AppWindow, id: i32) {
        let Some(s) = self.star(id) else { return };
        if self.cmp_pick {
            self.cmp_pick = false;
            let a = self.u.as_ref().map(|u| u.stars[s]);
            if a.is_some_and(|a| self.dna_of(a).is_some()) {
                self.dna_cmp = a;
            }
            self.show_dna(ui);
            return;
        }
        self.sel = Some(s);
        self.br_shown = None;
        self.uni(ui).set_br_shown(-1);
        self.panel(ui);
        self.refresh(ui);
    }

    pub fn blank(&mut self, ui: &AppWindow) {
        if self.sel.is_none() && self.br_shown.is_none() {
            return;
        }
        self.sel = None;
        self.br_shown = None;
        self.uni(ui).set_br_shown(-1);
        self.panel(ui);
        self.refresh(ui);
    }

    fn set_zoom(&mut self, ui: &AppWindow, z: f32) {
        let uni = self.uni(ui);
        let old = uni.get_zoom();
        uni.set_zoom(z);
        if bucket(old) != bucket(z) {
            self.refresh(ui);
        }
    }

    /// Колесо: приближаем к точке под курсором — она остаётся под ним
    pub fn wheel(&mut self, ui: &AppWindow, dy: f32, mx: f32, my: f32, w: f32, h: f32) {
        let uni = self.uni(ui);
        let z = uni.get_zoom();
        let nz = (z * (dy * 0.0022).exp().clamp(0.6, 1.6)).clamp(0.08, 6.0);
        let (wx, wy) = (uni.get_cam_x() + (mx - w / 2.0) / z, uni.get_cam_y() + (my - h / 2.0) / z);
        uni.set_cam_x(wx - (mx - w / 2.0) / nz);
        uni.set_cam_y(wy - (my - h / 2.0) / nz);
        self.anim.stop();
        self.set_zoom(ui, nz);
    }

    /// Плавный перелёт камеры: tick вызывается таймером, пока t < 1
    fn fly_to(&mut self, ui: &AppWindow, x: f32, y: f32, z: f32, tick: fn()) {
        let uni = self.uni(ui);
        self.fly = ((uni.get_cam_x(), uni.get_cam_y(), uni.get_zoom()), (x, y, z), 0.0);
        self.anim.start(TimerMode::Repeated, Duration::from_millis(16), tick);
    }

    pub fn tick(&mut self, ui: &AppWindow) {
        let (a, b, t) = self.fly;
        let t = (t + 1.0 / 28.0).min(1.0);
        self.fly.2 = t;
        let e = if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 };
        // зум — по логарифму, иначе издалека камера «ныряет» рывком
        let z = (a.2.ln() + (b.2.ln() - a.2.ln()) * e).exp();
        let uni = self.uni(ui);
        uni.set_cam_x(a.0 + (b.0 - a.0) * e);
        uni.set_cam_y(a.1 + (b.1 - a.1) * e);
        self.set_zoom(ui, z);
        if t >= 1.0 {
            self.anim.stop();
        }
    }

    /// Выбрать звезду узла графа без перелёта (артист, его трек или альбом)
    pub fn select_node(&mut self, ui: &AppWindow, node: i32) {
        if let Some(s) = self.node(node).and_then(|n| self.u.as_ref()?.star_for(n)) {
            self.pick(ui, s as i32);
        }
    }

    /// К звезде: выбрать и долететь
    pub fn fly(&mut self, ui: &AppWindow, id: i32, tick: fn()) {
        let Some(s) = self.star(id) else { return };
        let Some(&(x, y)) = self.u.as_ref().and_then(|u| u.pos.get(s)) else { return };
        let z = self.uni(ui).get_zoom().max(1.1);
        self.pick(ui, id);
        self.fly_to(ui, x, y, z, tick);
    }

    /// К узлу графа (из поиска, DNA, комнаты): артист, трек и альбом — к звезде артиста, жанр — к туманности
    pub fn goto(&mut self, ui: &AppWindow, node: i32, tick: fn()) {
        let Some(n) = self.node(node) else { return };
        let Some(u) = &self.u else { return };
        if u.g.nodes[n].kind == Kind::Genre {
            let name = u.g.nodes[n].name.clone();
            if let Some(nb) = u.nebulas.iter().find(|x| x.name == name) {
                let (x, y, r) = (nb.x, nb.y, nb.r);
                self.sel = None;
                self.panel(ui);
                self.fly_to(ui, x, y, (350.0 / r).clamp(0.3, 2.0), tick);
                self.refresh(ui);
            }
            return;
        }
        if let Some(s) = u.star_for(n) {
            self.fly(ui, s as i32, tick);
        }
    }

    /// Всё небо в окне
    pub fn fit(&mut self, ui: &AppWindow) {
        let Some(u) = &self.u else { return };
        let (cx, cy, w, h) = u.bounds();
        let uni = self.uni(ui);
        uni.set_cam_x(cx);
        uni.set_cam_y(cy);
        self.set_zoom(ui, (1000.0 / w).min(640.0 / h).clamp(0.1, 2.0));
    }

    pub fn fit_anim(&mut self, ui: &AppWindow, tick: fn()) {
        let Some(u) = &self.u else { return };
        let (cx, cy, w, h) = u.bounds();
        self.sel = None;
        self.br_shown = None;
        self.panel(ui);
        self.refresh(ui);
        self.fly_to(ui, cx, cy, (1000.0 / w).min(640.0 / h).clamp(0.1, 2.0), tick);
    }

    /// Перетащили звезду: двигаем её и перерисовываем линии
    pub fn drag(&mut self, ui: &AppWindow, id: i32, dx: f32, dy: f32) {
        let Some(s) = self.star(id) else { return };
        let (_, hi) = self.focus();
        let dimming = !hi.is_empty() || self.sel.is_some();
        let Some(u) = &mut self.u else { return };
        u.pos[s].0 += dx;
        u.pos[s].1 += dy;
        if let Some(&r) = self.row_of.get(&s) {
            if let Some(mut row) = self.nodes.row_data(r) {
                row.x = u.pos[s].0;
                row.y = u.pos[s].1;
                self.nodes.set_row_data(r, row);
            }
        }
        let p = u.paths(&self.vis, if dimming { 0 } else { PER_STAR }, &self.origins, &hi);
        ui.global::<Uni>().set_paths(model(p.into_iter().map(SharedString::from).collect()));
    }

    pub fn search(&self, ui: &AppWindow, q: &str) {
        let Some(u) = &self.u else { return };
        let rows: Vec<URel> = u
            .search(q, 8)
            .into_iter()
            .map(|i| {
                let n = &u.g.nodes[i];
                URel {
                    name: n.name.as_str().into(),
                    rel: kind_name(n.kind).to_lowercase().into(),
                    note: n.track.as_ref().map(|t| t.artist.clone()).unwrap_or_default().into(),
                    origin: 0,
                    src: if n.listened > 0 { dur(n.listened).into() } else { SharedString::new() },
                    id: i as i32,
                }
            })
            .collect();
        self.uni(ui).set_found(model(rows));
    }

    pub fn toggle_origin(&mut self, ui: &AppWindow, i: i32) {
        let o = [Origin::Confirmed, Origin::External, Origin::Derived][i.clamp(0, 2) as usize];
        if !self.origins.remove(&o) {
            self.origins.insert(o);
        }
        let flags: Vec<bool> = [Origin::Confirmed, Origin::External, Origin::Derived].iter().map(|o| self.origins.contains(o)).collect();
        self.uni(ui).set_origins(model(flags));
        self.refresh(ui);
    }

    /// Треки за узлом графа (см. explore::tracks_of)
    pub fn tracks_of(&self, node: usize) -> Vec<Track> {
        self.u.as_ref().map(|u| crate::explore::tracks_of(&u.g, node)).unwrap_or_default()
    }

    /// Треки выбранной звезды с i-го (строка в панели)
    pub fn panel_from(&self, i: usize) -> (Vec<Track>, usize) {
        (self.panel_tracks.clone(), i.min(self.panel_tracks.len().saturating_sub(1)))
    }

    fn dna_of(&self, node: usize) -> Option<(Dna, usize)> {
        crate::explore::dna_of(&self.u.as_ref()?.g, node)
    }

    /// Области вкуса из отпечатка: главные артисты кластеров и жанры
    pub fn set_taste(&mut self, ui: &AppWindow, artists: &[String], genres: &[String]) {
        self.taste_ids = artists.iter().map(|a| crate::graph::Graph::artist_id(a)).chain(genres.iter().map(|g| format!("g:{}", crate::graph::norm(g)))).collect();
        if self.taste_on {
            self.refresh(ui);
        }
    }

    pub fn toggle_taste(&mut self, ui: &AppWindow, on: bool) {
        self.taste_on = on;
        self.uni(ui).set_taste_on(on);
        if on {
            self.sel = None;
            self.br_shown = None;
            self.panel(ui);
        }
        self.refresh(ui);
    }

    pub fn has_taste(&self) -> bool {
        !self.taste_ids.is_empty()
    }

    /// Узел по имени (из комнаты: «смотрите во Вселенной»)
    pub fn find_name(&self, name: &str) -> Option<i32> {
        self.u.as_ref()?.search(name, 1).first().map(|&i| i as i32)
    }

    pub fn node_name(&self, node: i32) -> Option<String> {
        Some(self.u.as_ref()?.g.nodes.get(self.node(node)?)?.name.clone())
    }

    // ── мосты ──

    fn bridge_names(&self, ui: &AppWindow) {
        let Some(u) = &self.u else { return };
        let name = |i: Option<usize>| i.and_then(|i| u.g.nodes.get(i)).map(|n| SharedString::from(n.name.as_str())).unwrap_or_default();
        let uni = self.uni(ui);
        uni.set_br_a(name(self.br_a));
        uni.set_br_b(name(self.br_b));
    }

    pub fn bridge_end(&mut self, ui: &AppWindow, id: i32, from: bool) {
        let Some(i) = self.node(id) else { return };
        if from {
            self.br_a = Some(i);
        } else {
            self.br_b = Some(i);
        }
        let uni = self.uni(ui);
        uni.set_br_open(true);
        uni.set_br_status(SharedString::new());
        self.bridge_names(ui);
        if self.br_a.is_some() && self.br_b.is_some() {
            self.bridge_run(ui);
        }
    }

    pub fn bridge_rel(&mut self, ui: &AppWindow, i: i32) {
        if let Some(f) = self.br_rels.get_mut(i as usize) {
            *f = !*f;
        }
        self.uni(ui).set_br_rels(model(self.br_rels.to_vec()));
    }

    pub fn bridge_run(&mut self, ui: &AppWindow) {
        let uni = self.uni(ui);
        let (Some(u), Some(a), Some(b)) = (&self.u, self.br_a, self.br_b) else { return };
        let allow = crate::explore::bridge_rels(self.br_rels);
        let depth = (2.0 + uni.get_br_depth() * 6.0).round() as usize;
        self.bridges = if a == b { Vec::new() } else { u.g.bridges(a, b, depth, &allow, 3) };
        let rows: Vec<UBridge> = self
            .bridges
            .iter()
            .map(|p| {
                let st = crate::explore::steps(&u.g, a, p);
                let facts = st.iter().filter(|x| x.origin == Some(Origin::Confirmed)).count() as i32;
                let steps: Vec<UStep> = st
                    .iter()
                    .map(|x| UStep {
                        name: x.name.as_str().into(),
                        kind: kind_num(x.kind),
                        rel: x.rel.map(rel_name).unwrap_or_default().into(),
                        note: x.note.as_str().into(),
                        origin: x.origin.map_or(0, origin_num),
                        src: x.rel.map(rel_src).unwrap_or_default().into(),
                    })
                    .collect();
                UBridge { steps: model(steps), facts }
            })
            .collect();
        uni.set_br_status(if rows.is_empty() {
            tr!(
                "Мост не нашёлся за {} шагов. Попробуй больше шагов, включи другие виды связей или нажми «Подтянуть связи» — из MusicBrainz придут продюсеры, участники и лейблы.",
                "No bridge within {} steps. Allow more steps, enable other link types or press “Fetch links” — MusicBrainz adds producers, members and labels.",
                "Міст не знайшовся за {} кроків. Спробуй більше кроків, увімкни інші види зв'язків або натисни «Підтягнути зв'язки» — з MusicBrainz прийдуть продюсери, учасники й лейбли.",
                "Most sa nenašiel do {} krokov. Skús viac krokov, zapni iné druhy väzieb alebo stlač „Doplniť väzby“ — z MusicBrainz prídu producenti, členovia a vydavateľstvá.",
                depth
            )
            .into()
        } else {
            SharedString::new()
        });
        uni.set_bridges(model(rows));
        self.bridge_show(ui, 0);
    }

    pub fn bridge_show(&mut self, ui: &AppWindow, i: i32) {
        if self.bridges.get(i as usize).is_none() {
            self.br_shown = None;
            self.uni(ui).set_br_shown(-1);
            self.refresh(ui);
            return;
        }
        self.br_shown = Some(i as usize);
        self.uni(ui).set_br_shown(i);
        self.refresh(ui);
    }

    // ── Music DNA ──

    pub fn dna_open(&mut self, ui: &AppWindow, id: i32) {
        let Some(i) = self.node(id) else { return };
        self.dna_node = Some(i);
        self.dna_cmp = None;
        self.cmp_pick = false;
        self.show_dna(ui);
    }

    pub fn dna_compare(&mut self, ui: &AppWindow) {
        // выбор второго — на карте: окно прячем, следующий клик по узлу вернёт его со сравнением
        self.cmp_pick = !self.cmp_pick;
        let uni = self.uni(ui);
        uni.set_dna_cmp_pick(self.cmp_pick);
        uni.set_dna_open(!self.cmp_pick);
    }

    pub fn dna_close(&mut self, ui: &AppWindow) {
        self.cmp_pick = false;
        let uni = self.uni(ui);
        uni.set_dna_open(false);
        uni.set_dna_cmp_pick(false);
    }

    fn show_dna(&mut self, ui: &AppWindow) {
        let uni = self.uni(ui);
        let (Some(u), Some(i)) = (&self.u, self.dna_node) else { return };
        let Some((d, n)) = self.dna_of(i) else { return };
        let node = &u.g.nodes[i];
        uni.set_dna_title(node.name.as_str().into());
        let total = self.tracks_of(i).len();
        uni.set_dna_sub(
            if node.kind == Kind::Track {
                tr!("Разобрано {:.0} с звука", "{:.0} s of audio analysed", "Розібрано {:.0} с звуку", "Analyzovaných {:.0} s zvuku", d.secs)
            } else {
                tr!("Среднее по {} из {} треков, у которых есть DNA", "Average of {} of {} tracks that have DNA", "Середнє за {} з {} треків, що мають DNA", "Priemer {} z {} skladieb s DNA", n, total)
            }
            .into(),
        );
        let row = |label: String, value: String, frac: f32, known: bool| DnaRow { label: label.into(), value: value.into(), frac, known };
        let mut rows = vec![
            row(tr!("Темп", "Tempo", "Темп", "Tempo"), d.bpm.map(|b| format!("{b:.0} BPM")).unwrap_or_default(), d.bpm.map_or(0.0, |b| (b - 60.0) / 140.0), d.bpm.is_some()),
            row(
                tr!("Тональность", "Key", "Тональність", "Tónina"),
                d.key_name().map(|k| format!("{k} · {:.0} %", d.key_conf * 100.0)).unwrap_or_default(),
                d.key_conf,
                d.key.is_some(),
            ),
            row(tr!("Чёткость бита", "Beat clarity", "Чіткість біту", "Zreteľnosť beatu"), format!("{:.0} %", d.pulse * 100.0), d.pulse, true),
            row(tr!("Энергия", "Energy", "Енергія", "Energia"), format!("{:.0} %", d.energy * 100.0), d.energy, true),
            row(tr!("Громкость", "Loudness", "Гучність", "Hlasitosť"), format!("{:.1} dBFS", d.rms_db), (d.rms_db + 30.0) / 24.0, true),
            row(tr!("Динамика", "Dynamics", "Динаміка", "Dynamika"), format!("{:.1} dB", d.dyn_db), d.dyn_db / 18.0, true),
            row(tr!("Яркость", "Brightness", "Яскравість", "Jas"), format!("{:.0} Hz", d.centroid), d.centroid / 4000.0, true),
            row(tr!("Шумность", "Noisiness", "Шумність", "Šumivosť"), format!("{:.0} %", d.flatness * 100.0), d.flatness * 3.0, true),
            row(tr!("Низ · середина · верх", "Low · mid · high", "Низ · середина · верх", "Basy · stredy · výšky"), format!("{:.0}/{:.0}/{:.0} %", d.bands[0] * 100.0, d.bands[1] * 100.0, d.bands[2] * 100.0), d.bands[0], true),
            row(tr!("Атак в секунду", "Onsets per second", "Атак за секунду", "Nástupov za sekundu"), format!("{:.1}", d.onsets), d.onsets / 8.0, true),
        ];
        if node.kind == Kind::Track {
            let secs: Vec<String> = d.sections.iter().map(|s| format!("{}:{:02}", *s as u32 / 60, *s as u32 % 60)).collect();
            rows.push(row(tr!("Части", "Sections", "Частини", "Časti"), if secs.is_empty() { "1".into() } else { format!("{} · {}", secs.len() + 1, secs.join(" ")) }, (secs.len() as f32 + 1.0) / 8.0, true));
        }
        // этого по спектру не понять — честно пусто, пока нет ML-модели
        for l in [
            tr!("Танцевальность", "Danceability", "Танцювальність", "Tanečnosť"),
            tr!("Акустичность", "Acousticness", "Акустичність", "Akustickosť"),
            tr!("Инструментальность", "Instrumentalness", "Інструментальність", "Inštrumentálnosť"),
            tr!("Речь", "Speechiness", "Мовлення", "Reč"),
            tr!("Настроение (valence)", "Mood (valence)", "Настрій (valence)", "Nálada (valence)"),
            tr!("Инструменты и вокал", "Instruments and vocals", "Інструменти й вокал", "Nástroje a vokál"),
        ] {
            rows.push(row(l, String::new(), 0.0, false));
        }
        uni.set_dna_rows(model(rows));
        uni.set_dna_note(
            tr!(
                "Всё выше посчитано по самому звуку на твоём компьютере. Танцевальность, акустичность, речь, настроение, инструменты и вокал по спектру честно не определить — для них нужна локальная ML-модель, она ещё не подключена, поэтому там пусто.",
                "Everything above is computed from the audio itself on your computer. Danceability, acousticness, speech, mood, instruments and vocals can't be honestly read off the spectrum — that needs a local ML model, not connected yet, so those stay empty.",
                "Усе вище пораховано за самим звуком на твоєму комп'ютері. Танцювальність, акустичність, мовлення, настрій, інструменти й вокал за спектром чесно не визначити — для них потрібна локальна ML-модель, її ще не підключено, тому там порожньо.",
                "Všetko vyššie je vypočítané zo samotného zvuku na tvojom počítači. Tanečnosť, akustickosť, reč, náladu, nástroje a vokál zo spektra poctivo neurčíš — na to treba lokálny ML model, ešte nie je pripojený, preto je to prázdne."
            )
            .into(),
        );

        // радар: восемь осей, в каждой — нормированный настоящий признак
        let axes = [
            tr!("Энергия", "Energy", "Енергія", "Energia"),
            tr!("Бит", "Beat", "Біт", "Beat"),
            tr!("Темп", "Tempo", "Темп", "Tempo"),
            tr!("Яркость", "Bright", "Яскравість", "Jas"),
            tr!("Шум", "Noise", "Шум", "Šum"),
            tr!("Динамика", "Dynamics", "Динаміка", "Dynamika"),
            tr!("Атаки", "Onsets", "Атаки", "Nástupy"),
            tr!("Низ", "Bass", "Низ", "Basy"),
        ];
        let ang = |k: usize| -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::FRAC_PI_4;
        let poly = |d: &Dna| -> SharedString { crate::explore::radar_path(&crate::explore::radar(d)).into() };
        let mut polys = vec![poly(&d)];
        let labels: Vec<RadarLabel> = axes.iter().enumerate().map(|(k, t)| RadarLabel { text: t.as_str().into(), x: 100.0 + 92.0 * ang(k).cos(), y: 100.0 + 92.0 * ang(k).sin() }).collect();
        uni.set_dna_axes(model(labels));

        // сравнение со вторым
        let mut sim_rows = Vec::new();
        let mut cmp_name = SharedString::new();
        if let Some((c, _)) = self.dna_cmp.filter(|&c| c != i).and_then(|c| self.dna_of(c)) {
            polys.push(poly(&c));
            let s = dna::compare(&d, &c);
            for (l, v) in [
                (tr!("Ритм", "Rhythm", "Ритм", "Rytmus"), s.rhythm),
                (tr!("Гармония", "Harmony", "Гармонія", "Harmónia"), s.harmony),
                (tr!("Тембр", "Timbre", "Тембр", "Farba zvuku"), s.timbre),
                (tr!("Энергетика", "Energy", "Енергетика", "Energia"), s.energy),
                (tr!("Спектр", "Spectrum", "Спектр", "Spektrum"), s.spectrum),
                (tr!("Итого", "Overall", "Разом", "Spolu"), s.total),
            ] {
                sim_rows.push(row(l, format!("{:.0} %", v * 100.0), v, true));
            }
            cmp_name = u.g.nodes[self.dna_cmp.unwrap()].name.as_str().into();
        }
        uni.set_dna_cmp(cmp_name);
        uni.set_dna_sim(model(sim_rows));
        uni.set_dna_radar(model(polys));

        // похожие и непохожие — среди узлов того же вида (explore::neighbors)
        let names = [
            tr!("ритм", "rhythm", "ритм", "rytmus"),
            tr!("гармония", "harmony", "гармонія", "harmónia"),
            tr!("тембр", "timbre", "тембр", "farba"),
            tr!("энергия", "energy", "енергія", "energia"),
            tr!("спектр", "spectrum", "спектр", "spektrum"),
        ];
        let row = |p: &crate::explore::Pick| {
            let parts = [p.sim.rhythm, p.sim.harmony, p.sim.timbre, p.sim.energy, p.sim.spectrum];
            let n = &u.g.nodes[p.node];
            DnaPick {
                name: n.name.as_str().into(),
                sub: n.track.as_ref().map(|t| t.artist.clone()).unwrap_or_else(|| kind_name(n.kind).to_lowercase()).into(),
                pct: (p.sim.total * 100.0).round() as i32,
                why: p.why.iter().map(|&k| format!("{} {:.0} %", names[k], parts[k] * 100.0)).collect::<Vec<_>>().join(", ").into(),
                id: p.node as i32,
            }
        };
        let (similar, far) = crate::explore::neighbors(&u.g, i, &d);
        uni.set_dna_similar(model(similar.iter().map(row).collect()));
        let far: Vec<DnaPick> = far.iter().map(row).collect();
        uni.set_dna_far(model(far));
        uni.set_dna_cmp_pick(false);
        uni.set_dna_open(true);
    }
}
