//! Connect на ПК: где идёт звук, зеркало чужой сессии, команды хозяину, передача сессии.
//! Звук на другом устройстве — тут «зеркало»: плеер, текст и очередь показывают ту сессию, а кнопки
//! уходят командами туда. Пока играет этот ПК — он хозяин и пишет своё состояние в базу

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use slint::{TimerMode, VecModel};

use crate::connect::{self, State};
use crate::model::Track;
use crate::{cloud, key, post, with, App, ConnDev, LANG};

/// Последнее, что записали в базу как хозяин: по нему решаем, пора ли писать снова
#[derive(Default)]
pub struct Published {
    sig: String,
    qsig: u64,
    pos: f64,
    playing: bool,
    at: Option<Instant>,
    /// только что отдали сессию: пока база не ответила, не пишем (иначе «хозяин я» перебьёт передачу)
    quiet: Option<Instant>,
}

/// Нажали паузу/перемотку на зеркале: пока хозяин не записал новое состояние, события с прежним
/// (эхо нашей же команды) не откатывают кнопку и ползунок назад
pub struct Held {
    base: i64,
    pos: f64,
    at: i64,
    playing: bool,
    until: Instant,
}

/// Повтор из базы: число 0/1/2 (телефон и новый ПК) или bool (старый ПК)
fn repeat_of(v: &Value) -> Option<u8> {
    v.as_u64().map(|n| n.min(2) as u8).or(v.as_bool().map(u8::from))
}

fn qsig(played: &[Track], upnext: &[Track], source: &[Track]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for l in [played, upnext, source] {
        l.len().hash(&mut h);
        for t in l {
            t.key().hash(&mut h);
        }
    }
    h.finish()
}

fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "PC".into())
}

impl App {
    /// Подключиться к Connect (после входа в аккаунт; повторные вызовы ничего не делают)
    pub(crate) fn conn_start(&mut self) {
        if connect::db().is_none() || !cloud::account().verified || self.conn_live {
            return;
        }
        self.conn_live = true;
        self.ui.set_conn_on(true);
        connect::start(&self.stats.device, &host_name(), "pc", || post(|a| a.conn_changed()));
        self.conn_timer.start(TimerMode::Repeated, Duration::from_millis(500), || {
            with(|a| a.conn_tick());
        });
    }

    /// Пришли изменения из базы
    pub(crate) fn conn_changed(&mut self) {
        let (_, tree) = connect::snapshot();
        let me = connect::me();
        for (op, a) in connect::take_cmds(&tree) {
            self.conn_exec(&op, a);
        }
        let host = connect::active_host(&tree);
        let devs = connect::devices(&tree);
        let st = connect::state(&tree);
        // сессию перекинули сюда — подхватываем и играем
        if let Some(st) = st.as_ref().filter(|s| s.host == me && !s.xfer.is_empty() && s.xfer != self.conn_adopted && connect::now() - s.at < 60_000) {
            self.conn_adopted = st.xfer.clone();
            self.conn_adopt(st.clone());
        } else {
            match (&host, st) {
                (Some(h), Some(st)) if *h != me => {
                    let name = devs.iter().find(|d| d.id == *h).map_or(String::new(), |d| d.name.clone());
                    self.mirror_show(st, name);
                }
                _ if self.mirror.is_some() => self.mirror_end(),
                _ => {}
            }
        }
        let active = host.clone().or_else(|| (self.mirror.is_none() && self.ui.get_playing()).then(|| me.clone()));
        let list: Vec<ConnDev> = devs
            .iter()
            .filter(|d| d.online || d.id == me)
            .map(|d| ConnDev { id: d.id.as_str().into(), name: d.name.as_str().into(), phone: d.kind == "phone", active: active.as_deref() == Some(d.id.as_str()), me: d.id == me })
            .collect();
        self.ui.set_conn_devs(std::rc::Rc::new(VecModel::from(list)).into());
    }

    /// Команда от другого устройства (мы — хозяин)
    fn conn_exec(&mut self, op: &str, a: Value) {
        let tracks = |k: &str| serde_json::from_value::<Vec<Track>>(a[k].clone()).unwrap_or_default();
        match op {
            "toggle" => self.toggle(),
            "next" => self.advance(),
            "prev" => self.prev(),
            "seek" => self.seek_abs(a.as_f64().unwrap_or(0.0)),
            "repeat" => self.set_repeat(repeat_of(&a).unwrap_or(0)),
            "volume" => self.set_volume(a.as_f64().unwrap_or(0.0) as f32),
            "queue" | "load" => {
                (self.played, self.upnext, self.source) = (tracks("played"), tracks("upnext"), tracks("source"));
                self.set_shuffle(a["shuffle"].as_bool().unwrap_or(self.shuffle));
                self.set_repeat(repeat_of(&a["repeat"]).unwrap_or(self.repeat));
                match serde_json::from_value::<Track>(a["track"].clone()) {
                    Ok(t) if op == "load" => self.play_now(t),
                    _ => self.queue_changed(),
                }
            }
            // кнопку перемешки нажали на другом устройстве: круг выкл → перемешка → умная проходит хозяин
            "shuffle" => self.toggle_shuffle(),
            "transfer" => self.conn_pick(a.as_str().unwrap_or_default().to_string()),
            _ => {}
        }
    }

    /// Наша сессия как она есть
    fn conn_state(&self) -> State {
        State {
            host: connect::me(),
            track: self.current.clone(),
            played: self.played.iter().rev().take(50).rev().cloned().collect(),
            upnext: self.upnext.iter().take(500).cloned().collect(),
            source: self.source.iter().take(1000).cloned().collect(),
            pos: if self.pos.is_finite() { self.pos } else { 0.0 },
            at: 0,
            playing: self.ui.get_playing() && !self.restored,
            shuffle: self.shuffle,
            smart: self.shuffle && self.store.settings.smart_shuffle,
            repeat: self.repeat,
            vol: Some(self.store.settings.volume as f64),
            xfer: String::new(),
        }
    }

    /// Раз в полсекунды: зеркало двигает прогресс, хозяин пишет состояние, если что-то поменялось
    fn conn_tick(&mut self) {
        if let Some(st) = &self.mirror {
            let pos = st.pos_now();
            self.pos = pos;
            self.pos_at = Instant::now();
            if self.dur > 0.0 {
                self.ui.set_position((pos / self.dur) as f32);
            }
            self.ui.set_pos_text(crate::model::fmt_dur(pos as u64).into());
            return;
        }
        if self.conn_pub.quiet.is_some_and(|q| q.elapsed() < Duration::from_secs(5)) {
            return;
        }
        let Some(t) = &self.current else { return };
        let me = connect::me();
        let (_, tree) = connect::snapshot();
        let st = self.conn_state();
        // пишем, только если звук тут или сессия уже числится за нами (пауза)
        if !st.playing && tree["state"]["host"].as_str() != Some(me.as_str()) {
            return;
        }
        let playing = st.playing && !self.ui.get_loading();
        let sig = format!("{}|{playing}|{}{}|{}|{:.0}", key(t), st.shuffle, st.smart, st.repeat, st.vol.unwrap_or(0.0) * 100.0);
        let q = qsig(&self.played, &self.upnext, &self.source);
        let p = &self.conn_pub;
        let expected = p.pos + if p.playing { p.at.map_or(0.0, |a| a.elapsed().as_secs_f64()) } else { 0.0 };
        let stale = p.at.map_or(true, |a| a.elapsed() > Duration::from_secs(30));
        if sig == p.sig && q == p.qsig && (st.pos - expected).abs() < 2.0 && !stale {
            return;
        }
        let queue = q != p.qsig || stale;
        self.conn_pub = Published { sig, qsig: q, pos: st.pos, playing, at: Some(Instant::now()), quiet: None };
        connect::publish_bg(State { playing, ..st }, queue);
    }

    /// Звук на другом устройстве: показываем его сессию
    fn mirror_show(&mut self, mut st: State, name: String) {
        match &self.conn_held {
            Some(h) if st.at == h.base && h.until > Instant::now() => (st.pos, st.at, st.playing) = (h.pos, h.at, h.playing),
            _ => self.conn_held = None,
        }
        let first = self.mirror.is_none();
        if first && !self.restored && self.current.is_some() {
            self.stop(); // играли сами — замолкаем, звук теперь там
        }
        (self.played, self.upnext, self.source) = (st.played.clone(), st.upnext.clone(), st.source.clone());
        let pos = st.pos_now();
        if let Some(t) = st.track.clone() {
            if first || !self.current.as_ref().is_some_and(|c| c.same(&t)) {
                self.show_paused(t.clone(), pos);
                if self.lyr_for != key(&t) {
                    self.load_lyrics(&t);
                }
            }
        }
        self.shuffle = st.shuffle;
        self.ui.set_shuffle(st.shuffle);
        self.ui.set_smart_shuffle(st.smart); // значок — как у устройства со звуком
        self.repeat = st.repeat.min(2);
        self.ui.set_repeat(self.repeat as i32);
        // ползунок показывает громкость там, где звук; пока его тянут, эхо из базы не дёргает его назад
        if let Some(v) = st.vol.filter(|_| self.conn_vol_hold.is_none_or(|h| h < Instant::now())) {
            self.ui.set_volume(v as f32);
        }
        self.ui.set_playing(st.playing);
        self.ui.set_loading(false);
        self.pos = pos;
        self.pos_at = Instant::now();
        self.lyr_shown = pos;
        self.ui.set_conn_remote(name.into());
        self.mirror = Some(st);
        self.refresh_queue();
        self.mark_playing();
        self.update_lyr_timer();
        self.conn_tick();
    }

    /// Устройство со звуком пропало: остаёмся с его треком на паузе, play продолжит уже тут
    fn mirror_end(&mut self) {
        self.mirror = None;
        self.ui.set_smart_shuffle(self.store.settings.smart_shuffle);
        self.ui.set_volume(self.store.settings.volume);
        self.ui.set_conn_remote("".into());
        self.ui.set_playing(false);
        self.update_lyr_timer();
    }

    /// Сессию перекинули сюда
    fn conn_adopt(&mut self, st: State) {
        self.mirror = None;
        self.ui.set_smart_shuffle(self.store.settings.smart_shuffle);
        self.ui.set_volume(self.store.settings.volume);
        self.ui.set_conn_remote("".into());
        (self.played, self.upnext, self.source) = (st.played.clone(), st.upnext.clone(), st.source.clone());
        self.shuffle = st.shuffle;
        self.ui.set_shuffle(st.shuffle);
        self.repeat = st.repeat.min(2);
        self.ui.set_repeat(self.repeat as i32);
        if let Some(t) = st.track.clone() {
            let pos = st.pos_now();
            self.show_paused(t, pos);
            if st.playing {
                self.resume_at = Some(pos).filter(|p| *p > 1.0);
                self.start();
            }
        }
        self.refresh_queue();
        self.conn_pub = Published::default();
    }

    /// Команда хозяину (мы — зеркало). true — ушла туда, локально ничего делать не надо
    pub(crate) fn conn_cmd(&mut self, op: &str, a: Value) -> bool {
        let Some(st) = &mut self.mirror else { return false };
        let base = match &self.conn_held {
            Some(h) if h.until > Instant::now() => h.base,
            _ => st.at,
        };
        // перемотка и пауза видны сразу, не дожидаясь ответа
        if op == "seek" {
            st.pos = a.as_f64().unwrap_or(0.0);
            st.at = connect::now();
        }
        if op == "toggle" {
            st.pos = st.pos_now();
            st.at = connect::now();
            st.playing = !st.playing;
        }
        let (host, playing) = (st.host.clone(), st.playing);
        if op == "seek" || op == "toggle" {
            self.conn_held = Some(Held { base, pos: st.pos, at: st.at, playing, until: Instant::now() + Duration::from_secs(4) });
        }
        connect::send_bg(&host, op, a);
        if op == "toggle" {
            self.ui.set_playing(playing);
            self.update_lyr_timer();
        }
        true
    }

    /// Громкость на зеркале уходит хозяину, своя (settings.volume) не трогается. true — ушла туда.
    /// Пока тянут ползунок (end = false), шлём не чаще 5 раз в секунду: каждая команда — запись в базу
    pub(crate) fn conn_volume(&mut self, v: f32, end: bool) -> bool {
        if self.mirror.is_none() {
            return false;
        }
        self.conn_vol_hold = Some(Instant::now() + Duration::from_secs(3));
        if end {
            self.ui.set_volume(v);
        } else if self.conn_vol_sent.is_some_and(|t| t.elapsed() < Duration::from_millis(200)) {
            return true;
        }
        self.conn_vol_sent = Some(Instant::now());
        self.conn_cmd("volume", json!(v))
    }

    /// Вся очередь — хозяину (play трека, правка очереди, перемешка). load — ещё и сменить трек
    pub(crate) fn conn_queue(&mut self, load: bool) -> bool {
        if self.mirror.is_none() {
            return false;
        }
        let a = json!({
            "played": self.played, "upnext": self.upnext, "source": self.source,
            "track": if load { json!(self.current) } else { Value::Null },
            "shuffle": self.shuffle, "repeat": self.repeat,
        });
        self.conn_cmd(if load { "load" } else { "queue" }, a)
    }

    /// Выбрали устройство в списке: сессия переезжает туда
    pub(crate) fn conn_pick(&mut self, id: String) {
        let me = connect::me();
        if let Some(st) = &self.mirror {
            if st.host != id {
                connect::send_bg(&st.host.clone(), "transfer", json!(id));
            }
            return;
        }
        if id == me {
            return;
        }
        if self.current.is_none() {
            return self.toast(&tr!("Сначала включи что-нибудь", "Play something first", "Спершу увімкни щось", "Najprv niečo pusti"));
        }
        let mut st = self.conn_state();
        st.host = id;
        st.playing = true;
        st.xfer = format!("{me}-{}", connect::now());
        connect::publish_bg(st, true);
        self.conn_pub.quiet = Some(Instant::now());
        if !self.restored {
            self.stop();
        }
    }

    /// Закрываемся: если звук шёл тут — сессия встаёт на паузу, чтобы у остальных не бежал прогресс
    pub(crate) fn conn_bye(&self) {
        if !self.conn_live {
            return;
        }
        let (_, tree) = connect::snapshot();
        let ours = self.mirror.is_none() && tree["state"]["host"].as_str() == Some(connect::me().as_str());
        connect::bye(ours.then(|| State { playing: false, ..self.conn_state() }));
    }
}
