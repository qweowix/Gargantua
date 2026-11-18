//! Экран «Вселенная» на ПК: что видно, что выбрано, полёт камеры, мосты и панель Music DNA.
//! Раскладка и граф — в universe.rs и graph.rs, тут только состояние экрана и модели для Slint.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::dna::{self, Dna};
use crate::graph::{Kind, Origin, Rel};
use crate::model::Track;
use crate::universe::Universe;
use crate::{AppWindow, DnaPick, DnaRow, RadarLabel, UBridge, UNode, URel, UStep, Uni, LANG};

pub fn kind_of(i: i32) -> Kind {
    match i {
        1 => Kind::Artist,
        2 => Kind::Album,
        3 => Kind::Genre,
        4 => Kind::Person,
        5 => Kind::Label,
        _ => Kind::Track,
    }
}

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
    }
}

pub fn rel_src(r: Rel) -> String {
    match r {
        Rel::Performs | Rel::Feat => tr!("метаданные трека", "track metadata", "метадані треку", "metadáta skladby"),
        Rel::YtSimilar => "YouTube Music".into(),
        Rel::CoPlaylist => tr!("твои плейлисты", "your playlists", "твої плейлисти", "tvoje playlisty"),
        Rel::Sound => "Music DNA".into(),
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
    pub sel: Option<usize>,
    kinds: HashSet<Kind>,
    origins: HashSet<Origin>,
    vis: Vec<usize>,
    nodes: Rc<VecModel<UNode>>,
    row_of: HashMap<usize, usize>,
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
    /// «Мой вкус»: артисты и жанры из отпечатка (по id узлов, чтобы пережить перестройку графа)
    taste_ids: Vec<String>,
    taste_on: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            u: None,
            sel: None,
            kinds: [Kind::Track, Kind::Artist, Kind::Album, Kind::Genre, Kind::Person, Kind::Label].into(),
            origins: [Origin::Confirmed, Origin::External, Origin::Derived].into(),
            vis: Vec::new(),
            nodes: Rc::new(VecModel::default()),
            row_of: HashMap::new(),
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
        }
    }
}

impl View {
    /// Новый граф: выбор и мосты переносим по id узлов (номера в новом графе другие)
    pub fn set(&mut self, ui: &AppWindow, u: Universe) {
        let keep = |old: &Option<Universe>, i: Option<usize>| -> Option<usize> { u.g.find(&old.as_ref()?.g.nodes.get(i?)?.id) };
        self.sel = keep(&self.u, self.sel);
        self.br_a = keep(&self.u, self.br_a);
        self.br_b = keep(&self.u, self.br_b);
        self.dna_node = keep(&self.u, self.dna_node);
        self.dna_cmp = keep(&self.u, self.dna_cmp);
        self.bridges.clear();
        self.br_shown = None;
        self.u = Some(u);
        let uni = ui.global::<Uni>();
        uni.set_nodes(self.nodes.clone().into());
        uni.set_bridges(model(Vec::new()));
        if self.first {
            self.first = false;
            self.fit(ui);
        }
        self.refresh(ui);
        self.panel(ui);
        self.bridge_names(ui);
    }

    fn uni<'a>(&self, ui: &'a AppWindow) -> Uni<'a> {
        ui.global::<Uni>()
    }

    /// Соседи выбранного и узлы показанного моста — их видно при любом приближении
    fn focus(&self) -> (HashSet<usize>, HashSet<usize>) {
        let mut nodes = HashSet::new();
        let mut edges = HashSet::new();
        let Some(u) = &self.u else { return (nodes, edges) };
        if let Some(b) = self.br_shown.and_then(|i| self.bridges.get(i)) {
            let mut cur = self.br_a.unwrap_or(0);
            nodes.insert(cur);
            for &e in b {
                cur = u.g.other(e, cur);
                nodes.insert(cur);
                edges.insert(e);
            }
        } else if self.taste_on && self.sel.is_none() {
            for id in &self.taste_ids {
                if let Some(i) = u.g.find(id) {
                    nodes.insert(i);
                }
            }
        } else if let Some(s) = self.sel {
            // окружение: у «одинокого» узла — на два шага, чтобы было что исследовать
            let allow: HashSet<Rel> = Rel::all().into_iter().filter(|r| self.origins.contains(&r.origin())).collect();
            let depth = if u.g.adj[s].len() < 12 { 2 } else { 1 };
            let (ns, es) = u.g.around(s, depth, &allow, 160);
            nodes.extend(ns);
            edges.extend(es);
        }
        (nodes, edges)
    }

    pub fn refresh(&mut self, ui: &AppWindow) {
        let uni = self.uni(ui);
        let Some(u) = &self.u else {
            uni.set_empty(true);
            return;
        };
        uni.set_empty(u.g.nodes.is_empty());
        let (focus, hi) = self.focus();
        let mut vis = u.visible(uni.get_zoom(), &self.kinds, &focus);
        let shown: HashSet<usize> = vis.iter().copied().collect();
        vis.extend(focus.iter().filter(|i| !shown.contains(i)));
        let dimming = !focus.is_empty();
        let rows: Vec<UNode> = vis
            .iter()
            .map(|&i| {
                let n = &u.g.nodes[i];
                UNode {
                    x: u.pos[i].0,
                    y: u.pos[i].1,
                    r: u.radius[i],
                    c: color(u.color[i]),
                    label: n.name.as_str().into(),
                    kind: kind_num(n.kind),
                    sel: Some(i) == self.sel,
                    dim: dimming && !focus.contains(&i),
                    lab: focus.contains(&i) || n.kind == Kind::Genre,
                    id: i as i32,
                }
            })
            .collect();
        self.row_of = vis.iter().enumerate().map(|(r, &i)| (i, r)).collect();
        self.nodes.set_vec(rows);
        let p = u.paths(&vis, &self.origins, &hi);
        uni.set_paths(model(p.into_iter().map(SharedString::from).collect()));
        let dna_n = u.g.nodes.iter().filter(|n| n.kind == Kind::Track && n.track.as_ref().is_some_and(|t| dna::get(&t.key()).is_some())).count();
        let mb = u.g.edges.iter().filter(|e| e.rel.source() == "MusicBrainz").count();
        uni.set_status(
            tr!(
                "{} узлов · {} связей · Music DNA: {} треков · из MusicBrainz: {} связей",
                "{} nodes · {} links · Music DNA: {} tracks · from MusicBrainz: {} links",
                "{} вузлів · {} зв'язків · Music DNA: {} треків · з MusicBrainz: {} зв'язків",
                "{} uzlov · {} väzieb · Music DNA: {} skladieb · z MusicBrainz: {} väzieb",
                u.g.nodes.len(),
                u.g.edges.len(),
                dna_n,
                mb
            )
            .into(),
        );
        self.vis = vis;
    }

    /// Панель справа: что выбрано и все его связи
    fn panel(&self, ui: &AppWindow) {
        let uni = self.uni(ui);
        let (Some(u), Some(s)) = (&self.u, self.sel) else {
            uni.set_sel(-1);
            return;
        };
        let n = &u.g.nodes[s];
        uni.set_sel(s as i32);
        uni.set_sel_name(n.name.as_str().into());
        uni.set_sel_kind(kind_name(n.kind).into());
        uni.set_sel_color(color(u.color[s]));
        let mut sub = Vec::new();
        if let Some(t) = &n.track {
            sub.push(t.artist.clone());
        }
        if n.listened > 0 {
            sub.push(tr!("слушал {}", "listened {}", "слухав {}", "počúval si {}", dur(n.listened)));
        }
        if n.plays > 0 {
            sub.push(tr!("{} раз", "{} plays", "{} разів", "{}×", n.plays));
        }
        if let Some((d, k)) = self.dna_of(s) {
            let mut bits = Vec::new();
            if let Some(b) = d.bpm {
                bits.push(format!("{:.0} BPM", b));
            }
            if let Some(kn) = d.key_name() {
                bits.push(kn);
            }
            if k > 1 {
                bits.push(tr!("DNA по {} трекам", "DNA from {} tracks", "DNA за {} треками", "DNA z {} skladieb", k));
            }
            if !bits.is_empty() {
                sub.push(bits.join(" · "));
            }
        }
        uni.set_sel_sub(sub.join(" · ").into());
        uni.set_sel_play(!self.tracks_of(s).is_empty());
        uni.set_sel_track(n.track.as_ref().is_some_and(crate::rooms::shareable));
        uni.set_sel_dna(self.dna_of(s).is_some());
        let es = crate::explore::relations(&u.g, s, 300);
        let rels: Vec<URel> = es
            .iter()
            .map(|&e| {
                let ed = &u.g.edges[e];
                let o = u.g.other(e, s);
                URel {
                    name: u.g.nodes[o].name.as_str().into(),
                    rel: rel_name(ed.rel).into(),
                    note: ed.note.as_str().into(),
                    origin: origin_num(ed.rel.origin()),
                    src: rel_src(ed.rel).into(),
                    id: o as i32,
                }
            })
            .collect();
        uni.set_rels(model(rels));
    }

    /// Номер узла из UI — только если такой есть (у строк без узла id = -1)
    fn valid(&self, id: i32) -> Option<usize> {
        let n = self.u.as_ref()?.g.nodes.len();
        (id >= 0 && (id as usize) < n).then_some(id as usize)
    }

    pub fn pick(&mut self, ui: &AppWindow, id: i32) {
        let Some(i) = self.valid(id) else { return };
        if self.cmp_pick {
            self.cmp_pick = false;
            if self.dna_of(i).is_some() {
                self.dna_cmp = Some(i);
            }
            self.show_dna(ui);
            return;
        }
        self.sel = Some(i);
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
        let nz = (z * (dy * 0.0022).exp().clamp(0.6, 1.6)).clamp(0.08, 8.0);
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

    pub fn fly(&mut self, ui: &AppWindow, id: i32, tick: fn()) {
        let Some(i) = self.valid(id) else { return };
        let Some(u) = &self.u else { return };
        let Some(&(x, y)) = u.pos.get(i) else { return };
        let z = match u.g.nodes[i].kind {
            Kind::Track => 2.2,
            Kind::Genre => 0.6,
            _ => 1.3,
        };
        let z = if u.g.nodes[i].kind == Kind::Genre { z } else { self.uni(ui).get_zoom().max(z) };
        self.pick(ui, id);
        self.fly_to(ui, x, y, z, tick);
    }

    /// Вся вселенная в окне
    pub fn fit(&mut self, ui: &AppWindow) {
        let Some(u) = &self.u else { return };
        let (cx, cy, w, h) = u.bounds();
        let uni = self.uni(ui);
        uni.set_cam_x(cx);
        uni.set_cam_y(cy);
        let z = (1100.0 / w).min(700.0 / h).clamp(0.08, 2.0);
        self.set_zoom(ui, z);
    }

    pub fn fit_anim(&mut self, ui: &AppWindow, tick: fn()) {
        let Some(u) = &self.u else { return };
        let (cx, cy, w, h) = u.bounds();
        self.fly_to(ui, cx, cy, (1100.0 / w).min(700.0 / h).clamp(0.08, 2.0), tick);
    }

    /// Перетащили узел: двигаем его (и его треки вместе с ним) и перерисовываем связи
    pub fn drag(&mut self, ui: &AppWindow, id: i32, dx: f32, dy: f32) {
        let Some(i) = self.valid(id) else { return };
        let (_, hi) = self.focus();
        let Some(u) = &mut self.u else { return };
        let mut moved = vec![i];
        if u.g.nodes[i].kind != Kind::Track {
            for &e in &u.g.adj[i] {
                let o = u.g.other(e, i);
                if u.g.nodes[o].kind == Kind::Track && matches!(u.g.edges[e].rel, Rel::Performs | Rel::OnAlbum) && u.g.edges[e].w >= 1.0 {
                    moved.push(o);
                }
            }
        }
        for &m in &moved {
            u.pos[m].0 += dx;
            u.pos[m].1 += dy;
            if let Some(&r) = self.row_of.get(&m) {
                if let Some(mut row) = self.nodes.row_data(r) {
                    row.x = u.pos[m].0;
                    row.y = u.pos[m].1;
                    self.nodes.set_row_data(r, row);
                }
            }
        }
        let p = u.paths(&self.vis, &self.origins, &hi);
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

    pub fn toggle_kind(&mut self, ui: &AppWindow, k: i32) {
        let k = kind_of(k);
        if !self.kinds.remove(&k) {
            self.kinds.insert(k);
        }
        let flags: Vec<bool> = (0..6).map(|i| self.kinds.contains(&kind_of(i))).collect();
        self.uni(ui).set_kinds(model(flags));
        self.refresh(ui);
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

    /// Треки за узлом (см. explore::tracks_of)
    pub fn tracks_of(&self, i: usize) -> Vec<Track> {
        self.u.as_ref().map(|u| crate::explore::tracks_of(&u.g, i)).unwrap_or_default()
    }

    fn dna_of(&self, i: usize) -> Option<(Dna, usize)> {
        crate::explore::dna_of(&self.u.as_ref()?.g, i)
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

    /// Найти узел по имени (для «показать во Вселенной» из комнаты)
    pub fn find_name(&self, name: &str) -> Option<i32> {
        let u = self.u.as_ref()?;
        u.search(name, 1).first().map(|&i| i as i32)
    }

    pub fn node_name(&self, id: i32) -> Option<String> {
        Some(self.u.as_ref()?.g.nodes.get(self.valid(id)?)?.name.clone())
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
        let Some(i) = self.valid(id) else { return };
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
        let Some(i) = self.valid(id) else { return };
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
