//! Плеер в комнате: публикуем свои действия (если нам можно управлять) и следуем за комнатой.
//! Расхождение до 0.15 с не трогаем, до 2 с догоняем скоростью ±3 % (на слух незаметно), больше — перематываем.
//! Очередь — очередь управляющего: она едет вместе с состоянием, у всех видна и у всех заранее грузится,
//! так что смена трека происходит сразу, без ожидания загрузки. Участники в конце трека сами включают
//! следующий из этой очереди. Предложения участников (с голосами) управляющий за полминуты до конца трека
//! ставит следующим — самое популярное.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, SharedString, TimerMode, VecModel};

use crate::model::Track;
use crate::player::Cmd;
use crate::rooms::{self, Room};
use crate::{key, post, with, App, RoomMember, RoomMsg, RoomQ, Rooms, LANG};

#[derive(Default)]
pub struct RoomLink {
    pub live: bool,
    /// что мы последний раз записали или приняли: ключ|играет, позиция, когда, номер
    sig: String,
    pos: f64,
    at: Option<Instant>,
    seq: u64,
    /// после того как сами подстроились под комнату — не публикуем это как своё действие
    quiet: Option<Instant>,
    nudge: Option<Instant>,
    beat: Option<Instant>,
    warned: Option<Instant>,
    last_chat: usize,
    /// предложения, которые уже поставили в очередь (чтобы не взять дважды, пока база не удалила)
    taken: std::collections::HashSet<String>,
}

/// Отпечаток очереди: меняется — публикуем заново
fn qsig(q: &[Track]) -> u64 {
    q.iter().take(30).flat_map(|t| key(t).into_bytes()).fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(std::rc::Rc::new(VecModel::from(v)))
}

pub fn room_err(e: &anyhow::Error) -> String {
    let s = format!("{e:#}");
    if s.contains("PERMISSION") {
        tr!(
            "База не пускает: правила для комнат не опубликованы (packaging/database.rules.json, см. docs/firebase.md)",
            "The database refused: room rules aren't published (packaging/database.rules.json, see docs/firebase.md)",
            "База не пускає: правила для кімнат не опубліковано (packaging/database.rules.json, див. docs/firebase.md)",
            "Databáza odmietla: pravidlá pre miestnosti nie sú zverejnené (packaging/database.rules.json, pozri docs/firebase.md)"
        )
    } else if s.contains("NO_ROOM") {
        tr!("Комнаты с таким кодом нет", "No room with this code", "Кімнати з таким кодом немає", "Miestnosť s takým kódom neexistuje")
    } else if s.contains("NOT_VERIFIED") {
        tr!("Нужен вход в аккаунт с подтверждённой почтой", "Sign in with a verified email first", "Потрібен вхід в акаунт з підтвердженою поштою", "Najprv sa prihlás s overeným e-mailom")
    } else if s.contains("LOCAL") {
        tr!("Файл с компьютера в комнату не добавить — у других его нет", "A local file can't go into a room — others don't have it", "Файл з комп'ютера в кімнату не додати — в інших його немає", "Lokálny súbor do miestnosti nepridáš — ostatní ho nemajú")
    } else if s.contains("NETWORK") {
        tr!("Нет связи с базой", "Can't reach the database", "Немає зв'язку з базою", "Databáza je nedostupná")
    } else {
        s
    }
}

impl App {
    pub(crate) fn room_rooms(&self) -> Rooms<'_> {
        self.ui.global::<Rooms>()
    }

    /// Вошли (или создали) — подписка на изменения и тик синхронизации
    pub(crate) fn room_enter(&mut self, code: String) {
        self.room_rooms().set_busy(true);
        std::thread::spawn(move || {
            let r = rooms::join(&code, || post(|a| a.room_changed()));
            post(move |a| {
                a.room_rooms().set_busy(false);
                match r {
                    Ok(()) => {
                        a.room = RoomLink { live: true, ..Default::default() };
                        a.room_rooms().set_error(SharedString::new());
                        a.room_rooms().set_inside(true);
                        a.ui.global::<crate::Uni>().set_in_room(true);
                        a.room_timer.start(TimerMode::Repeated, Duration::from_millis(500), || {
                            with(|a| a.room_tick());
                        });
                    }
                    Err(e) => a.room_rooms().set_error(room_err(&e).into()),
                }
            });
        });
    }

    pub(crate) fn room_leave(&mut self) {
        rooms::leave();
        self.room = RoomLink::default();
        self.room_timer.stop();
        self.room_unnudge();
        let r = self.room_rooms();
        r.set_inside(false);
        self.ui.global::<crate::Uni>().set_in_room(false);
        r.set_members(model(Vec::new()));
        r.set_queue(model(Vec::new()));
        r.set_chat(model(Vec::new()));
    }

    pub(crate) fn room_on(&self) -> bool {
        self.room.live
    }

    /// Пришло изменение из базы — перерисовать комнату; подстройка плеера — в тике
    pub(crate) fn room_changed(&mut self) {
        let Some(r) = rooms::room() else { return };
        if r.meta.owner.is_empty() && self.room.at.is_some() {
            // комнату закрыл владелец
            self.room_leave();
            self.toast(&tr!("Комнату закрыли", "The room was closed", "Кімнату закрили", "Miestnosť bola zatvorená"));
            return;
        }
        self.room_show(&r);
        self.room_follow(&r);
    }

    fn room_show(&mut self, r: &Room) {
        let me = rooms::uid();
        let ui = self.ui.global::<Rooms>();
        ui.set_code(r.code.as_str().into());
        ui.set_name(r.meta.name.as_str().into());
        ui.set_owner(r.is_owner(&me));
        ui.set_can_control(r.can_control(&me));
        ui.set_can_queue(r.can_queue(&me));
        ui.set_public(r.meta.public);
        ui.set_queue_all(r.meta.queue == "all");
        ui.set_control_all(r.meta.control == "all");
        let now = crate::connect::now();
        let state_key = r.state.as_ref().and_then(|s| s.track.as_ref()).map(key).unwrap_or_default();
        let members: Vec<RoomMember> = r
            .members
            .iter()
            .map(|m| RoomMember {
                uid: m.uid.as_str().into(),
                name: m.name.as_str().into(),
                owner: m.role == "owner",
                me: m.uid == me,
                online: now - m.seen < rooms::ONLINE_MS,
                status: if now - m.seen >= rooms::ONLINE_MS {
                    tr!("не на связи", "offline", "не на зв'язку", "offline")
                } else if !m.key.is_empty() && m.key != state_key {
                    tr!("играет другое", "playing something else", "грає інше", "hrá niečo iné")
                } else if m.playing {
                    tr!("слушает · {}", "listening · {}", "слухає · {}", "počúva · {}", crate::model::fmt_dur(m.pos as u64))
                } else {
                    tr!("на паузе", "paused", "на паузі", "pozastavené")
                }
                .into(),
            })
            .collect();
        ui.set_members(model(members));
        let next_id = r.next().map(|q| q.id.clone()).unwrap_or_default();
        let row = |a: &Self, t: &Track| -> RoomQ {
            let img = a.covers.get(&t.cover).cloned();
            RoomQ { title: t.title.as_str().into(), artist: t.artist.as_str().into(), has_cover: img.is_some(), cover: img.unwrap_or_default(), cover_url: t.cover.as_str().into(), ..Default::default() }
        };
        let q: Vec<RoomQ> = r
            .queue
            .iter()
            .map(|q| RoomQ {
                id: q.id.as_str().into(),
                by: q.by_name.as_str().into(),
                votes: q.votes as i32,
                mine: q.mine,
                next: q.id == next_id,
                removable: r.is_owner(&me) || q.by == me,
                ..row(self, &q.track)
            })
            .collect();
        ui.set_queue(model(q));
        // очередь управляющего — то, что реально заиграет дальше
        let up: Vec<Track> = r.state.as_ref().map(|s| s.upnext.clone()).unwrap_or_default();
        ui.set_upnext(model(up.iter().map(|t| row(self, t)).collect()));
        let mut want: Vec<String> = r.queue.iter().map(|q| q.track.cover.clone()).chain(up.iter().map(|t| t.cover.clone())).collect();
        let cur = self.current.as_ref().map(key).unwrap_or_default();
        let chat: Vec<RoomMsg> = r
            .chat
            .iter()
            .rev()
            .take(80)
            .rev()
            .map(|m| RoomMsg {
                name: m.name.as_str().into(),
                text: m.text.as_str().into(),
                emoji: m.emoji.as_str().into(),
                me: m.uid == me,
                about_now: !m.key.is_empty() && m.key == cur,
                focus: m.emoji == "🪐",
            })
            .collect();
        // новое сообщение от другого — короткий тост, если комната не на экране
        let mut note = None;
        if r.chat.len() > self.room.last_chat && self.room.last_chat > 0 && self.ui.get_page() != "rooms" {
            if let Some(m) = r.chat.last().filter(|m| m.uid != me) {
                let t = if m.text.is_empty() { m.emoji.clone() } else { m.text.clone() };
                note = Some(format!("{}: {}", m.name, t));
            }
        }
        self.room.last_chat = r.chat.len();
        ui.set_chat(model(chat));
        match r.state.as_ref().and_then(|s| s.track.as_ref()) {
            Some(t) => {
                ui.set_now_title(t.title.as_str().into());
                ui.set_now_artist(t.artist.as_str().into());
                let img = self.covers.get(&t.cover).cloned();
                ui.set_now_has_cover(img.is_some());
                ui.set_now_cover(img.unwrap_or_default());
                ui.set_now_cover_url(t.cover.as_str().into());
                want.push(t.cover.clone());
            }
            None => {
                ui.set_now_title(SharedString::new());
                ui.set_now_artist(SharedString::new());
                ui.set_now_has_cover(false);
            }
        }
        for u in want {
            self.want_cover(&u);
        }
        if let Some(n) = note {
            self.toast(&n);
        }
    }

    /// Подстроиться под состояние комнаты, если его записал не я
    fn room_follow(&mut self, r: &Room) {
        let me = rooms::uid();
        let Some(st) = r.state.clone() else { return };
        if st.by == me || self.mirror.is_some() {
            return;
        }
        let Some(t) = st.track.clone() else { return };
        let target = st.pos_now();
        let same = self.current.as_ref().is_some_and(|c| key(c) == key(&t));
        // очередь управляющего — к себе: плеер заранее грузит следующие, смена трека — без ожидания
        if qsig(&st.upnext) != qsig(&self.upnext) {
            self.upnext = st.upnext.clone();
            self.queue_changed();
        }
        let sig = format!("{}|{}|{}", key(&t), qsig(&st.upnext), st.playing);
        self.room.sig = sig;
        self.room.pos = target;
        self.room.at = Some(Instant::now());
        self.room.seq = self.room.seq.max(st.seq);
        self.room.quiet = Some(Instant::now());
        if !same {
            if let Some(c) = self.current.take() {
                self.played.push(c);
            }
            self.play_now(t);
            // play_now сбрасывает resume_at — ставим после: поток откроется и встанет на позицию комнаты
            self.resume_at = Some(target).filter(|p| *p > 1.0);
            if !st.playing {
                // трек встаёт на место, но на паузе — как у всех
                self.room_pause_after_load = true;
            }
            return;
        }
        if self.ui.get_playing() != st.playing && !self.ui.get_loading() {
            self.toggle();
        }
        if st.playing {
            self.room_drift(target);
        } else if (self.pos - target).abs() > 1.0 {
            self.seek_abs(target);
        }
    }

    /// Расхождение с комнатой: перемотка или лёгкая подстройка скорости
    fn room_drift(&mut self, target: f64) {
        if self.ui.get_loading() {
            return;
        }
        let local = self.pos + if self.ui.get_playing() { self.pos_at.elapsed().as_secs_f64() } else { 0.0 };
        let drift = local - target;
        self.room_rooms().set_drift((drift * 1000.0).round() as i32);
        if drift.abs() > 2.0 {
            self.seek_abs(target);
            self.room_unnudge();
        } else if let Some(k) = rooms::nudge(drift) {
            let base = self.store.settings.speed as f64;
            if let Some(p) = &self.player {
                p.send(Cmd::Speed(base * k));
            }
            // 3 % скорости догоняют 0.03 с за секунду
            self.room.nudge = Some(Instant::now() + Duration::from_secs_f64((drift.abs() / 0.03).min(30.0)));
        } else {
            self.room_unnudge();
        }
    }

    fn room_unnudge(&mut self) {
        if self.room.nudge.take().is_some() {
            let base = self.store.settings.speed as f64;
            if let Some(p) = &self.player {
                p.send(Cmd::Speed(base));
            }
        }
    }

    /// Раз в полсекунды: публикуем свои действия, следим за расхождением, «я тут» раз в 10 с
    fn room_tick(&mut self) {
        let Some(r) = rooms::room() else { return };
        let me = rooms::uid();
        if self.room.nudge.is_some_and(|t| Instant::now() >= t) {
            self.room_unnudge();
        }
        if self.room_pause_after_load && !self.ui.get_loading() && self.ui.get_playing() && self.pos > 0.3 {
            self.room_pause_after_load = false;
            self.toggle();
        }
        let playing = self.ui.get_playing() && !self.ui.get_loading();
        let pos = self.pos + if playing { self.pos_at.elapsed().as_secs_f64() } else { 0.0 };
        let cur_key = self.current.as_ref().map(key).unwrap_or_default();
        if self.room.beat.is_none_or(|b| b.elapsed() > Duration::from_secs(10)) {
            self.room.beat = Some(Instant::now());
            let k = cur_key.clone();
            std::thread::spawn(move || {
                let _ = rooms::beat(pos, playing, &k);
            });
        }
        // чужое состояние: следим за расхождением
        if let Some(st) = r.state.as_ref().filter(|s| s.by != me) {
            if st.playing && st.track.as_ref().is_some_and(|t| key(t) == cur_key) && self.room.quiet.is_some_and(|q| q.elapsed() > Duration::from_secs(3)) {
                self.room_drift(st.pos_now());
            }
        }
        if !r.can_control(&me) || self.mirror.is_some() {
            return;
        }
        if self.room.quiet.is_some_and(|q| q.elapsed() < Duration::from_secs(2)) {
            return;
        }
        let Some(t) = self.current.clone() else { return };
        if self.ui.get_loading() {
            return;
        }
        // самое популярное предложение — следующим, за полминуты до конца трека (или сразу, если очередь пуста)
        if let Some(q) = r.next().filter(|q| !self.room.taken.contains(&q.id)) {
            if self.upnext.is_empty() || (self.dur > 0.0 && self.dur - pos < 30.0) {
                self.room.taken.insert(q.id.clone());
                self.upnext.insert(0, q.track.clone());
                self.queue_changed();
                let id = q.id.clone();
                std::thread::spawn(move || {
                    let _ = rooms::unqueue(&id);
                });
            }
        }
        let sig = format!("{}|{}|{}", key(&t), qsig(&self.upnext), playing);
        let expected = self.room.pos + if self.room.sig.ends_with("|true") { self.room.at.map_or(0.0, |a| a.elapsed().as_secs_f64()) } else { 0.0 };
        let owner_refresh = r.is_owner(&me) && self.room.at.is_some_and(|a| a.elapsed() > Duration::from_secs(20));
        if sig == self.room.sig && (pos - expected).abs() < 2.5 && !owner_refresh {
            return;
        }
        if !rooms::shareable(&t) {
            if self.room.warned.is_none_or(|w| w.elapsed() > Duration::from_secs(60)) {
                self.room.warned = Some(Instant::now());
                self.toast(&room_err(&anyhow::anyhow!("LOCAL")));
            }
            return;
        }
        self.room.sig = sig;
        self.room.pos = pos;
        self.room.at = Some(Instant::now());
        self.room.seq += 1;
        let seq = self.room.seq;
        let up: Vec<Track> = self.upnext.iter().take(30).cloned().collect();
        std::thread::spawn(move || {
            if let Err(e) = rooms::set_state(Some(&t), pos, playing, seq, &up) {
                crate::logln!("комната: {e:#}");
            }
        });
    }

    /// Трек кончился. Управляющий — обычный переход по своей очереди (false). Участник включает следующий
    /// из той же очереди сам, не дожидаясь сети: он уже загружен; состояние комнаты потом лишь подправит позицию
    pub(crate) fn room_ended(&mut self) -> bool {
        if !self.room.live {
            return false;
        }
        let Some(r) = rooms::room() else { return false };
        if r.can_control(&rooms::uid()) {
            return false;
        }
        if !self.upnext.is_empty() {
            let next = self.upnext.remove(0);
            if let Some(c) = self.current.take() {
                self.played.push(c);
            }
            self.play_now(next);
            self.room.quiet = Some(Instant::now());
        }
        true
    }

    /// «Дальше» в комнате: управляющий берёт из общей очереди, остальным нельзя
    pub(crate) fn room_next(&mut self) -> bool {
        if !self.room.live {
            return false;
        }
        let Some(r) = rooms::room() else { return false };
        if !r.can_control(&rooms::uid()) {
            self.toast(&tr!("В этой комнате треки переключает владелец", "Only the owner switches tracks in this room", "У цій кімнаті треки перемикає власник", "V tejto miestnosti prepína skladby vlastník"));
            return true;
        }
        false
    }

    pub(crate) fn room_add(&mut self, t: Track) {
        std::thread::spawn(move || {
            let r = rooms::enqueue(&t);
            post(move |a| match r {
                Ok(()) => a.toast(&tr!("«{}» — в очереди комнаты", "“{}” added to the room queue", "«{}» — у черзі кімнати", "„{}“ je vo fronte miestnosti", t.title)),
                Err(e) => a.toast(&room_err(&e)),
            });
        });
    }

    /// Сделать что-то с комнатой в фоне и показать ошибку, если не вышло
    pub(crate) fn room_bg(&mut self, f: impl FnOnce() -> anyhow::Result<()> + Send + 'static) {
        std::thread::spawn(move || {
            if let Err(e) = f() {
                post(move |a| a.toast(&room_err(&e)));
            }
        });
    }

    pub(crate) fn room_publics(&mut self) {
        std::thread::spawn(|| {
            let r = rooms::public_rooms();
            post(move |a| match r {
                Ok(v) => {
                    let rows: Vec<RoomMember> = v.into_iter().map(|(c, n, o)| RoomMember { uid: c.into(), name: n.into(), status: o.into(), ..Default::default() }).collect();
                    a.room_rooms().set_publics(model(rows));
                }
                Err(e) => a.room_rooms().set_error(room_err(&e).into()),
            });
        });
    }

    /// Обложка загрузилась — подставить в комнату (играющий трек, очередь, предложения)
    pub(crate) fn room_cover(&mut self, url: &str, img: &slint::Image) {
        if !self.room.live {
            return;
        }
        let ui = self.ui.global::<Rooms>();
        if ui.get_now_cover_url() == url {
            ui.set_now_cover(img.clone());
            ui.set_now_has_cover(true);
        }
        for m in [ui.get_upnext(), ui.get_queue()] {
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
