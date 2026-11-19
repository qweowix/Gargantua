//! «Угадай песню» на ПК: соло или 1 на 1 (duel.rs), раунды, отдельный mpv для фрагментов (музыка,
//! статистика, Discord и медиаклавиши его не видят), поиск-ответ как обычный, проверка — guess::same.

use std::sync::atomic::Ordering;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::guess::{self, LEVELS};
use crate::model::{Src, Track};
use crate::player::{Cmd, Player};
use crate::{cache, duel, key, post, App, GItem, Guess, LANG};

#[derive(Default)]
pub struct Game {
    pool: Vec<Track>,
    next: usize,
    cur: Option<Track>,
    level: usize,
    clips: Vec<std::path::PathBuf>,
    player: Option<Player>,
    /// номер раунда: устаревшие ответы фоновых задач отбрасываются
    gen: u64,
    search_gen: u64,
    results: Vec<Track>,
    artists: Vec<(String, String)>,
    score: u32,
    streak: u32,
    best: u32,
    rounds: u32,
    hits: u32,
    typing: Timer,
    /// 1 на 1: какой раунд дуэли сейчас (номер и серверное время начала), сколько раз я нажал «дальше»,
    /// заявил ли я ответ, раскрыт ли раунд
    duel: bool,
    duel_round: Option<(usize, i64)>,
    my_want: usize,
    claimed: bool,
    revealed: bool,
    /// готовые фрагменты по ключу трека (и откуда звук — для полного прослушивания на раскрытии);
    /// следующие песни готовятся заранее, пока идёт раунд
    prepared: std::collections::HashMap<String, (Vec<std::path::PathBuf>, String)>,
    preparing: std::collections::HashSet<String>,
    src: Option<String>,
}

/// Откуда брать звук и нарезать фрагменты — в фоне (это и есть «Готовлю фрагмент»: ссылка на поток + ffmpeg)
fn fetch_clips(t: &Track, start: f64, trim: bool, local: Option<String>) -> anyhow::Result<(Vec<std::path::PathBuf>, String)> {
    let src = match local {
        Some(s) => s,
        None => crate::resolve(t).and_then(|u| crate::ytmusic::stream_url(&u))?,
    };
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key(t).hash(&mut h);
    let clips = guess::make_clips(&src, start, trim, &dir().join(format!("{:016x}", h.finish())))?;
    Ok((clips, src))
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(std::rc::Rc::new(VecModel::from(v)))
}

fn dir() -> std::path::PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("BlackHole").join("guess")
}

impl App {
    fn g(&self) -> Guess<'_> {
        self.ui.global::<Guess>()
    }

    fn gitem(&self, title: &str, sub: &str, cover: &str, id: usize) -> GItem {
        let img = self.covers.get(cover).cloned();
        GItem { title: title.into(), sub: sub.into(), has_cover: img.is_some(), cover: img.unwrap_or_default(), cover_url: cover.into(), id: id as i32 }
    }

    /// Открыли вкладку: список плейлистов; музыку — на паузу, чтобы не мешала
    pub(crate) fn guess_opened(&mut self) {
        let list: Vec<(usize, String, usize, String)> = self
            .store
            .playlists
            .iter()
            .enumerate()
            .filter(|(_, p)| p.tracks.len() >= 3)
            .map(|(i, p)| (i, p.name.clone(), p.tracks.len(), p.tracks.iter().find(|t| !t.cover.is_empty()).map(|t| t.cover.clone()).unwrap_or_default()))
            .collect();
        let mut pls = Vec::new();
        for (i, name, n, cover) in list {
            self.want_cover(&cover);
            pls.push(self.gitem(&name, &tr!("{} треков", "{} tracks", "{} треків", "{} skladieb", n), &cover, i));
        }
        self.g().set_playlists(model(pls));
    }

    fn guess_pause_music(&mut self) {
        if self.ui.get_playing() && self.mirror.is_none() {
            self.toggle();
        }
    }

    pub(crate) fn guess_pick_playlist(&mut self, i: usize) {
        let Some(p) = self.store.playlists.get(i) else { return };
        let mut pool = guess::dedupe(p.tracks.iter().filter(|t| t.src != Src::Lc || std::path::Path::new(&t.url).is_file()).cloned().collect());
        if pool.len() < 3 {
            self.g().set_error(tr!("В плейлисте слишком мало песен", "Too few songs in this playlist", "У плейлисті замало пісень", "V playliste je primálo skladieb").into());
            return;
        }
        guess::shuffle(&mut pool);
        let name = p.name.clone();
        self.guess_begin(pool, tr!("Плейлист «{}»", "Playlist “{}”", "Плейлист «{}»", "Playlist „{}“", name));
    }

    pub(crate) fn guess_search_artist(&mut self, q: String) {
        self.game.search_gen += 1;
        let gen = self.game.search_gen;
        if q.trim().len() < 2 {
            self.g().set_artists(model(Vec::new()));
            return;
        }
        // ждём, пока допечатают: не дёргаем поиск на каждую букву
        self.game.typing.start(TimerMode::SingleShot, Duration::from_millis(350), move || {
            let q = q.clone();
            std::thread::spawn(move || {
                let r = crate::ytmusic::search_cards(&q, crate::ytmusic::ARTISTS, 6);
                post(move |a| {
                    if a.game.search_gen != gen {
                        return;
                    }
                    let cards = r.unwrap_or_default();
                    a.game.artists = cards.iter().map(|c| (c.id.clone(), c.title.clone())).collect();
                    let rows: Vec<GItem> = cards
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            a.want_cover(&c.cover);
                            a.gitem(&c.title, &c.sub, &c.cover, i)
                        })
                        .collect();
                    a.g().set_artists(model(rows));
                });
            });
        });
    }

    pub(crate) fn guess_pick_artist(&mut self, i: usize) {
        let Some((id, name)) = self.game.artists.get(i).cloned() else { return };
        self.g().set_busy(true);
        self.g().set_error(SharedString::new());
        std::thread::spawn(move || {
            let r = guess::artist_pool(&id, &name);
            post(move |a| {
                a.g().set_busy(false);
                match r {
                    Ok(pool) => a.guess_begin(pool, name.clone()),
                    Err(e) => a.g().set_error(tr!("Не вышло собрать песни артиста: {}", "Couldn't get the artist's songs: {}", "Не вийшло зібрати пісні артиста: {}", "Nepodarilo sa získať skladby: {}", format!("{e:#}")).into()),
                }
            });
        });
    }

    fn guess_begin(&mut self, pool: Vec<Track>, source: String) {
        if self.g().get_kind() == 1 {
            return self.duel_create(pool, source);
        }
        self.game.duel = false;
        self.guess_pause_music();
        self.game.pool = pool;
        self.game.next = 0;
        let g = self.g();
        g.set_source(source.into());
        g.set_error(SharedString::new());
        self.guess_round();
    }

    /// Новый раунд соло: следующая песня из перемешанного списка
    fn guess_round(&mut self) {
        if self.game.pool.is_empty() {
            return;
        }
        if self.game.next >= self.game.pool.len() {
            guess::shuffle(&mut self.game.pool);
            self.game.next = 0;
        }
        let t = self.game.pool[self.game.next].clone();
        self.game.next += 1;
        let start = guess::clip_start(&t, t.dur as f64);
        self.guess_prepare(t, start);
    }

    /// Раунд с песней t: фрагменты с секунды start режутся в фоне
    fn guess_prepare(&mut self, t: Track, start: f64) {
        self.game.cur = Some(t.clone());
        self.game.level = 0;
        self.game.clips.clear();
        self.game.my_want = 0;
        self.game.claimed = false;
        self.game.revealed = false;
        self.game.gen += 1;
        let gen = self.game.gen;
        let g = self.g();
        g.set_stage(1);
        g.set_level(0);
        g.set_ready(false);
        g.set_wrong(SharedString::new());
        g.set_q(SharedString::new());
        g.set_results(model(Vec::new()));
        g.set_me_skip(false);
        g.set_opp_skip(false);
        self.game.src = None;
        if let Some((clips, src)) = self.game.prepared.remove(&key(&t)) {
            // заготовлено заранее — играем сразу
            self.game.clips = clips;
            self.game.src = Some(src);
            self.g().set_ready(true);
            self.guess_play();
        } else {
            let job = self.guess_job(&t, start);
            let duel_mode = self.game.duel;
            std::thread::spawn(move || {
                let r = job();
                post(move |a| {
                    if a.game.gen != gen {
                        return;
                    }
                    match r {
                        Ok((clips, src)) => {
                            a.game.clips = clips;
                            a.game.src = Some(src);
                            a.g().set_ready(true);
                            a.guess_play();
                        }
                        Err(e) => {
                            crate::logln!("угадайка: {e:#}");
                            if !duel_mode {
                                // песня не достаётся — молча берём следующую, раунд не засчитываем
                                a.guess_round();
                            } else {
                                a.g().set_wrong(tr!("Эта песня у меня не грузится — жди соперника или жми «дальше»", "This song won't load here — wait for your opponent or press “next”", "Ця пісня в мене не вантажиться — чекай суперника або тисни «далі»", "Táto skladba sa nenačíta — počkaj na súpera alebo stlač „ďalej“").into());
                            }
                        }
                    }
                });
            });
        }
        self.guess_prefetch();
    }

    /// Задача «достать звук и нарезать» для трека: файл с диска, если есть, иначе поток
    fn guess_job(&self, t: &Track, start: f64) -> impl FnOnce() -> anyhow::Result<(Vec<std::path::PathBuf>, String)> + Send + 'static {
        let k = key(t);
        let local = if t.src == Src::Lc { Some(t.url.clone()) } else { None }
            .or_else(|| Some(self.dl_dir().join(crate::download::file_name(t))).filter(|p| p.is_file()).map(|p| p.display().to_string()))
            .or_else(|| cache::path_of(&k).map(|p| p.display().to_string()))
            .or_else(|| self.stream_cache.get(&k).map(|(u, _)| u.clone()));
        let t = t.clone();
        // в дуэли — ровно с начала у обоих; соло — тишина в начале пропускается
        let trim = !self.game.duel;
        move || fetch_clips(&t, start, trim, local)
    }

    /// Следующие две песни — заранее, по одной за раз
    fn guess_prefetch(&mut self) {
        let next: Vec<Track> = if self.game.duel {
            match (duel::state(), self.game.duel_round) {
                (Some(d), Some((n, _))) if !d.pool.is_empty() => (1..=2).map(|k| d.pool[(n + k) % d.pool.len()].clone()).collect(),
                _ => Vec::new(),
            }
        } else {
            self.game.pool.iter().skip(self.game.next).take(2).cloned().collect()
        };
        // старые заготовки, которые уже не понадобятся, — прочь (места на диске немного, но всё же)
        let keep: std::collections::HashSet<String> = next.iter().map(key).collect();
        self.game.prepared.retain(|k, _| keep.contains(k));
        let Some(t) = next.into_iter().find(|t| !self.game.prepared.contains_key(&key(t)) && !self.game.preparing.contains(&key(t))) else { return };
        let k = key(&t);
        self.game.preparing.insert(k.clone());
        let job = self.guess_job(&t, guess::clip_start(&t, t.dur as f64));
        std::thread::spawn(move || {
            #[cfg(target_os = "linux")]
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 10);
            }
            let r = job();
            post(move |a| {
                a.game.preparing.remove(&k);
                match r {
                    Ok(v) => {
                        a.game.prepared.insert(k, v);
                        a.guess_prefetch();
                    }
                    Err(e) => crate::logln!("угадайка, заранее: {e:#}"),
                }
            });
        });
    }

    /// Сыграть фрагмент текущего уровня (отдельным mpv)
    pub(crate) fn guess_play(&mut self) {
        let Some(clip) = self.game.clips.get(self.game.level).cloned() else { return };
        if self.game.player.is_none() {
            let vol = self.vol_at(1.0);
            self.game.player = Player::spawn(vol, |_| {}).map_err(|e| crate::logln!("{e:#}")).ok();
        }
        if let Some(p) = &self.game.player {
            p.send(Cmd::Volume(self.vol_at(1.0)));
            p.send(Cmd::Load(clip.display().to_string(), 1));
        }
    }

    /// «Не знаю» или неверный ответ: длиннее фрагмент; после 5 с — проигрыш
    pub(crate) fn guess_skip(&mut self) {
        if self.g().get_stage() != 1 {
            return;
        }
        if self.game.duel {
            // в дуэли длиннее становится, только когда нажали оба: отмечаем свой голос
            if self.game.my_want <= self.game.level && self.game.my_want < LEVELS.len() {
                self.game.my_want = self.game.level + 1;
                let w = self.game.my_want;
                self.g().set_me_skip(true);
                std::thread::spawn(move || {
                    if let Err(e) = duel::want(w) {
                        crate::logln!("дуэль: {e:#}");
                    }
                });
            }
            return;
        }
        if self.game.level + 1 >= LEVELS.len() {
            return self.guess_finish(false);
        }
        self.game.level += 1;
        self.g().set_level(self.game.level as i32);
        if self.game.clips.is_empty() {
            return;
        }
        self.guess_play();
    }

    fn guess_finish(&mut self, won: bool) {
        if self.game.duel {
            // угадал — заявка с серверным временем; кто взял раунд, решит дуэль (duel_changed)
            if won && !self.game.claimed && !self.game.revealed {
                self.game.claimed = true;
                let lvl = self.game.level;
                self.g().set_wrong(tr!("Верно! Сверяю, кто был первым…", "Correct! Checking who was first…", "Правильно! Звіряю, хто був першим…", "Správne! Overujem, kto bol prvý…").into());
                std::thread::spawn(move || {
                    if let Err(e) = duel::claim(lvl) {
                        crate::logln!("дуэль: {e:#}");
                    }
                });
            }
            return;
        }
        let Some(t) = self.game.cur.clone() else { return };
        let gm = &mut self.game;
        gm.rounds += 1;
        let gained = if won { guess::points(gm.level) } else { 0 };
        if won {
            gm.hits += 1;
            gm.streak += 1;
            gm.score += gained;
            gm.best = gm.best.max(gm.streak);
        } else {
            gm.streak = 0;
        }
        let (score, streak, best, rounds, hits) = (gm.score, gm.streak, gm.best, gm.rounds, gm.hits);
        self.want_cover(&t.cover);
        let img = self.covers.get(&t.cover).cloned();
        let g = self.g();
        g.set_won(won);
        g.set_gained(gained as i32);
        g.set_score(score as i32);
        g.set_streak(streak as i32);
        g.set_best(best as i32);
        g.set_rounds(rounds as i32);
        g.set_hits(hits as i32);
        g.set_ans_title(t.title.as_str().into());
        g.set_ans_artist(t.artist.as_str().into());
        g.set_ans_has_cover(img.is_some());
        g.set_ans_cover(img.unwrap_or_default());
        g.set_ans_url(t.cover.as_str().into());
        g.set_stage(2);
        g.set_level(LEVELS.len() as i32 - 1);
        self.game.level = LEVELS.len() - 1;
        self.guess_full();
    }

    /// Поиск-ответ: только среди песен игры (плейлиста или артиста) — мгновенно, без сети
    pub(crate) fn guess_search(&mut self, q: String) {
        let pool: Vec<Track> = if self.game.duel { duel::state().map(|d| d.pool).unwrap_or_default() } else { self.game.pool.clone() };
        let found: Vec<Track> = guess::search_pool(&pool, &q, 12).into_iter().map(|i| pool[i].clone()).collect();
        let mut rows = Vec::new();
        for (i, t) in found.iter().enumerate() {
            self.want_cover(&t.cover);
            rows.push(self.gitem(&t.title, &t.artist, &t.cover, i));
        }
        self.game.results = found;
        self.g().set_searching(false);
        self.g().set_results(model(rows));
    }

    pub(crate) fn guess_pick(&mut self, i: usize) {
        let (Some(ans), Some(g)) = (self.game.cur.clone(), self.game.results.get(i).cloned()) else { return };
        if guess::same(&ans, &g) {
            self.guess_finish(true);
        } else {
            self.guess_wrong(&g.title);
        }
    }

    pub(crate) fn guess_typed(&mut self, text: String) {
        let Some(ans) = self.game.cur.clone() else { return };
        if text.trim().is_empty() {
            return;
        }
        if guess::typed(&ans, &text) {
            self.guess_finish(true);
        } else if let Some(first) = self.game.results.first().cloned().filter(|g| guess::same(&ans, g)) {
            // Enter при первом найденном — то же, что клик по нему
            let _ = first;
            self.guess_finish(true);
        } else {
            self.guess_wrong(&text);
        }
    }

    fn guess_wrong(&mut self, what: &str) {
        let msg = tr!("Не то: {}", "Not it: {}", "Не те: {}", "Nie je to: {}", what);
        self.g().set_q(SharedString::new());
        self.g().set_results(model(Vec::new()));
        if self.game.duel {
            self.g().set_wrong(msg.into());
            return;
        }
        self.guess_skip();
        if self.g().get_stage() == 1 {
            self.g().set_wrong(msg.into());
        }
    }

    pub(crate) fn guess_next(&mut self) {
        if !self.game.duel {
            return self.guess_round();
        }
        // следующую песню дуэли выбирает хозяин
        let Some(d) = duel::state() else { return };
        if d.meta.host != duel::uid() || d.pool.is_empty() {
            return;
        }
        let n = d.round.as_ref().map_or(0, |r| (r.n + 1) % d.pool.len());
        let start = guess::clip_start(&d.pool[n], d.pool[n].dur as f64);
        std::thread::spawn(move || {
            if let Err(e) = duel::start_round(n, start) {
                crate::logln!("дуэль: {e:#}");
            }
        });
    }

    /// Раунд раскрыт: песня играет целиком с начала (плеером игры), пока не нажмёшь «Следующая»
    fn guess_full(&mut self) {
        let Some(src) = self.game.src.clone() else { return self.guess_play() };
        if let Some(p) = &self.game.player {
            p.send(Cmd::Volume(self.vol_at(1.0)));
            p.send(Cmd::Load(src, 1));
        }
    }

    pub(crate) fn guess_change(&mut self) {
        if self.game.duel {
            duel::leave();
            self.game.duel = false;
            self.game.duel_round = None;
        }
        self.game.gen += 1;
        if let Some(p) = &self.game.player {
            p.send(Cmd::Stop);
        }
        self.g().set_stage(0);
        self.guess_opened();
    }

    // ── 1 на 1 ──

    /// Хозяин: дуэль с перемешанным списком, код для соперника
    fn duel_create(&mut self, pool: Vec<Track>, source: String) {
        self.g().set_busy(true);
        std::thread::spawn(move || {
            let r = duel::create(&source, &pool, || post(|a| a.duel_changed()));
            post(move |a| {
                a.g().set_busy(false);
                match r {
                    Ok(code) => a.duel_enter(code, true),
                    Err(e) => a.g().set_error(crate::roomsync::room_err(&e).into()),
                }
            });
        });
    }

    pub(crate) fn duel_join(&mut self, code: String) {
        if code.trim().is_empty() {
            return;
        }
        self.g().set_busy(true);
        std::thread::spawn(move || {
            let r = duel::join(&code, || post(|a| a.duel_changed()));
            post(move |a| {
                a.g().set_busy(false);
                match r {
                    Ok(()) => a.duel_enter(code.trim().to_uppercase(), false),
                    Err(e) => {
                        let s = format!("{e:#}");
                        a.g().set_error(if s.contains("NO_DUEL") {
                            tr!("Дуэли с таким кодом нет", "No duel with this code", "Дуелі з таким кодом немає", "Duel s týmto kódom neexistuje")
                        } else if s.contains("DUEL_FULL") {
                            tr!("В этой дуэли уже двое", "This duel already has two players", "У цій дуелі вже двоє", "V tomto dueli sú už dvaja")
                        } else {
                            crate::roomsync::room_err(&e)
                        }
                        .into());
                    }
                }
            });
        });
    }

    fn duel_enter(&mut self, code: String, host: bool) {
        self.guess_pause_music();
        self.game.duel = true;
        self.game.duel_round = None;
        self.game.score = 0;
        let g = self.g();
        g.set_duel_code(code.into());
        g.set_host(host);
        g.set_error(SharedString::new());
        g.set_stage(3);
        self.duel_changed();
    }

    /// Пришло изменение дуэли: соперник вошёл, новый раунд, нажали «дальше», кто-то угадал
    pub(crate) fn duel_changed(&mut self) {
        if !self.game.duel {
            return;
        }
        let Some(d) = duel::state() else { return };
        if d.gone {
            self.game.duel = false;
            duel::leave();
            self.g().set_stage(0);
            self.toast(&tr!("Дуэль закрыта", "The duel was closed", "Дуель закрито", "Duel bol zatvorený"));
            return;
        }
        let me = duel::uid();
        let opp = d.opponent(&me);
        let g = self.g();
        g.set_source(d.meta.source.as_str().into());
        g.set_host(d.meta.host == me);
        g.set_me_name(d.name_of(&me).into());
        g.set_opp_name(if opp.is_empty() { SharedString::new() } else { d.name_of(&opp).into() });
        g.set_me_score(d.score.get(&me).copied().unwrap_or(0) as i32);
        g.set_opp_score(d.score.get(&opp).copied().unwrap_or(0) as i32);
        if !d.ready() {
            g.set_stage(3);
            return;
        }
        // хозяин начинает первый раунд, как только соперник вошёл
        if d.round.is_none() {
            if d.meta.host == me && !d.pool.is_empty() {
                let start = guess::clip_start(&d.pool[0], d.pool[0].dur as f64);
                std::thread::spawn(move || {
                    let _ = duel::start_round(0, start);
                });
            }
            return;
        }
        let r = d.round.clone().unwrap();
        if self.game.duel_round != Some((r.n, r.at)) {
            let Some(t) = d.pool.get(r.n).cloned() else { return };
            self.game.duel_round = Some((r.n, r.at));
            self.guess_prepare(t, r.start);
        }
        if self.game.revealed {
            return;
        }
        // кто-то угадал раньше всех — раунд его
        if let Some((w, c)) = d.winner() {
            self.game.revealed = true;
            let won = w == me;
            if won {
                let total = d.score.get(&me).copied().unwrap_or(0) + guess::points(c.level);
                std::thread::spawn(move || {
                    let _ = duel::set_score(total);
                });
            }
            self.duel_reveal(won, d.name_of(&w));
            return;
        }
        if d.given_up() {
            self.game.revealed = true;
            self.duel_reveal(false, String::new());
            return;
        }
        // фрагмент длиннее — когда «дальше» нажали оба
        let lvl = d.level().min(LEVELS.len() - 1);
        let opp_want = d.want.get(&opp).copied().unwrap_or(0);
        self.g().set_opp_skip(opp_want > lvl);
        let mine = self.game.my_want > lvl;
        self.g().set_me_skip(mine);
        if lvl > self.game.level {
            self.game.level = lvl;
            self.g().set_level(lvl as i32);
            self.g().set_wrong(SharedString::new());
            self.guess_play();
        }
    }

    fn duel_reveal(&mut self, won: bool, winner: String) {
        let Some(t) = self.game.cur.clone() else { return };
        self.game.rounds += 1;
        if won {
            self.game.hits += 1;
        }
        self.want_cover(&t.cover);
        let img = self.covers.get(&t.cover).cloned();
        let g = self.g();
        g.set_won(won);
        g.set_winner(winner.into());
        g.set_ans_title(t.title.as_str().into());
        g.set_ans_artist(t.artist.as_str().into());
        g.set_ans_has_cover(img.is_some());
        g.set_ans_cover(img.unwrap_or_default());
        g.set_ans_url(t.cover.as_str().into());
        g.set_rounds(self.game.rounds as i32);
        g.set_hits(self.game.hits as i32);
        g.set_stage(2);
        g.set_level(LEVELS.len() as i32 - 1);
        self.game.level = LEVELS.len() - 1;
        self.guess_full();
    }

    /// Обложка загрузилась — в строки игры
    pub(crate) fn guess_cover(&mut self, url: &str, img: &slint::Image) {
        let g = self.g();
        if g.get_ans_url() == url {
            g.set_ans_cover(img.clone());
            g.set_ans_has_cover(true);
        }
        for m in [g.get_playlists(), g.get_artists(), g.get_results()] {
            for i in 0..m.row_count() {
                if let Some(mut r) = m.row_data(i) {
                    if r.cover_url == url && !r.has_cover {
                        r.cover = img.clone();
                        r.has_cover = true;
                        m.set_row_data(i, r);
                    }
                }
            }
        }
    }
}

