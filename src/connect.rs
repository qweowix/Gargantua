//! Connect: одна сессия на все устройства аккаунта, как в Spotify. Звук идёт на одном устройстве («хозяин»),
//! остальные показывают то же самое и управляют им командами; сессию можно перекинуть на другое устройство.
//! Живёт в Firebase Realtime Database, узел connect/<uid>:
//!   devices/<id> = {name, kind, seen}    seen — серверное время последнего «я тут» (раз в 25 с)
//!   state = {host, track, played, upnext, source, pos, at, playing, shuffle, repeat, vol, xfer}
//!       host — где идёт звук; at — серверное время, когда pos был верен (остальные досчитывают сами);
//!       xfer меняется, когда сессию перекидывают: новый host подхватывает её и играет
//!   cmd/<id> = {to, op, a, t}            команды хозяину от остальных; хозяин выполняет и удаляет
//! Читаем потоком событий (SSE) и держим весь узел в памяти.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

use crate::model::Track;

/// Устройство считается живым, если отметилось за это время
pub const ONLINE_MS: i64 = 75_000;
const BEAT: Duration = Duration::from_secs(25);

/// Состояние сессии. Пустые списки база не хранит — приходят как null
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub host: String,
    #[serde(deserialize_with = "nullable")]
    pub track: Option<Track>,
    #[serde(deserialize_with = "nullable")]
    pub played: Vec<Track>,
    #[serde(deserialize_with = "nullable")]
    pub upnext: Vec<Track>,
    /// исходный список (из него перестраивается очередь при перемешке и повторе)
    #[serde(deserialize_with = "nullable")]
    pub source: Vec<Track>,
    pub pos: f64,
    pub at: i64,
    pub playing: bool,
    pub shuffle: bool,
    /// умное перемешивание у хозяина (значок на остальных устройствах)
    pub smart: bool,
    /// ключи треков очереди, подмешанных умным перемешиванием (не из плейлиста) — у всех помечены одинаково
    #[serde(deserialize_with = "nullable")]
    pub marks: Vec<String>,
    /// 0 выкл, 1 вся очередь, 2 один трек
    pub repeat: u8,
    /// громкость там, где идёт звук (0..1). Нет — хозяин старой версии
    pub vol: Option<f64>,
    /// метка передачи: новое значение у host — «подхвати сессию и играй»
    pub xfer: String,
}

fn nullable<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> std::result::Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

impl State {
    /// Где сейчас трек с учётом прошедшего времени
    pub fn pos_now(&self) -> f64 {
        let p = if self.playing && self.at > 0 { self.pos + (now() - self.at).max(0) as f64 / 1000.0 } else { self.pos };
        match &self.track {
            Some(t) if t.dur > 0 => p.min(t.dur as f64),
            _ => p,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String, // "pc" | "phone"
    pub online: bool,
}

pub fn db() -> Option<String> {
    std::env::var("BLACKHOLE_FIREBASE_DB")
        .ok()
        .or(option_env!("BLACKHOLE_FIREBASE_DB").map(String::from))
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
}

struct Shared {
    tree: Value,
    ver: u64,
}

fn shared() -> &'static (Mutex<Shared>, Condvar) {
    static S: OnceLock<(Mutex<Shared>, Condvar)> = OnceLock::new();
    S.get_or_init(|| (Mutex::new(Shared { tree: json!({}), ver: 0 }), Condvar::new()))
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static OFFSET: AtomicI64 = AtomicI64::new(0); // серверное время − наше, мс

#[derive(Clone, Default)]
struct Me {
    id: String,
    name: String,
    kind: String,
}

fn me_lock() -> &'static Mutex<Me> {
    static M: OnceLock<Mutex<Me>> = OnceLock::new();
    M.get_or_init(Default::default)
}

type Notify = Box<dyn Fn() + Send>;

fn notify_lock() -> &'static Mutex<Option<Notify>> {
    static N: OnceLock<Mutex<Option<Notify>>> = OnceLock::new();
    N.get_or_init(Default::default)
}

pub fn me() -> String {
    me_lock().lock().unwrap().id.clone()
}

fn local_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Серверное время (по нему пишут at и seen)
pub fn now() -> i64 {
    local_ms() + OFFSET.load(Ordering::Relaxed)
}

fn base() -> Result<(String, String)> {
    let db = db().context("нет базы Connect")?;
    let c = crate::cloud::cfg().context("облако не настроено")?;
    let a = crate::cloud::account();
    if !a.verified || a.uid.is_empty() {
        bail!("NOT_VERIFIED");
    }
    Ok((format!("{db}/connect/{}", a.uid), crate::cloud::auth_token(&c)?))
}

fn req(method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    let (base, tok) = base()?;
    let url = if path.is_empty() { format!("{base}.json") } else { format!("{base}/{path}.json") };
    let r = crate::util::agent().request(method, &url).query("auth", &tok);
    let r = match body {
        Some(b) => r.send_json(b),
        None => r.call(),
    };
    match r {
        Ok(r) => Ok(r.into_json().unwrap_or(Value::Null)),
        Err(ureq::Error::Status(code, r)) => bail!("Connect {code}: {}", r.into_string().unwrap_or_default()),
        Err(e) => bail!("NETWORK {e}"),
    }
}

pub fn patch(path: &str, v: Value) -> Result<()> {
    req("PATCH", path, Some(&v)).map(drop)
}

pub fn delete(path: &str) -> Result<()> {
    req("DELETE", path, None).map(drop)
}

/// «Я тут» + поправка часов: сервер подставляет своё время, сравниваем с нашим
fn heartbeat() -> Result<()> {
    let m = me_lock().lock().unwrap().clone();
    let t0 = local_ms();
    let v = req("PUT", &format!("devices/{}", m.id), Some(&json!({"name": m.name, "kind": m.kind, "seen": {".sv": "timestamp"}})))?;
    let t1 = local_ms();
    if let Some(seen) = v["seen"].as_i64() {
        OFFSET.store(seen - (t0 + t1) / 2, Ordering::Relaxed);
    }
    Ok(())
}

/// Подключиться (один раз за запуск): отметки «я тут» и поток изменений. on_change зовётся из фонового
/// потока после каждого изменения
pub fn start(id: &str, name: &str, kind: &str, on_change: impl Fn() + Send + 'static) {
    *me_lock().lock().unwrap() = Me { id: id.into(), name: name.into(), kind: kind.into() };
    *notify_lock().lock().unwrap() = Some(Box::new(on_change));
    if db().is_none() || RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| loop {
        if let Err(e) = heartbeat() {
            crate::logln!("connect beat: {e:#}");
        }
        std::thread::sleep(BEAT);
    });
    std::thread::spawn(|| loop {
        if let Err(e) = listen() {
            crate::logln!("connect: {e:#}");
        }
        std::thread::sleep(Duration::from_secs(3));
    });
}

fn listen() -> Result<()> {
    let (base, tok) = base()?;
    // отдельный клиент: база шлёт keep-alive раз в ~30 с, общий таймаут в 20 с оборвал бы поток
    static AG: OnceLock<ureq::Agent> = OnceLock::new();
    let ag = AG.get_or_init(|| ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(70)).build());
    let r = ag.get(&format!("{base}.json")).query("auth", &tok).set("Accept", "text/event-stream").call().map_err(|e| anyhow!("{e}"))?;
    let mut event = String::new();
    for line in BufReader::new(r.into_reader()).lines() {
        let line = line?;
        if let Some(e) = line.strip_prefix("event:") {
            event = e.trim().to_string();
        } else if let Some(d) = line.strip_prefix("data:") {
            match event.as_str() {
                "put" | "patch" => {
                    let v: Value = serde_json::from_str(d.trim())?;
                    apply(&event, v["path"].as_str().unwrap_or("/"), v["data"].clone());
                }
                // токен истёк или доступ отозван — переподключаемся со свежим
                "cancel" | "auth_revoked" => bail!("поток закрыт: {event}"),
                _ => {}
            }
        }
    }
    Ok(())
}

fn apply(kind: &str, path: &str, data: Value) {
    let (lock, cv) = shared();
    {
        let mut s = lock.lock().unwrap();
        let mut node = &mut s.tree;
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        for p in &parts {
            if !node.is_object() {
                *node = json!({});
            }
            node = node.as_object_mut().unwrap().entry(p.to_string()).or_insert(Value::Null);
        }
        match (kind, data) {
            ("patch", Value::Object(m)) => {
                if !node.is_object() {
                    *node = json!({});
                }
                let o = node.as_object_mut().unwrap();
                for (k, v) in m {
                    if v.is_null() {
                        o.remove(&k);
                    } else {
                        o.insert(k, v);
                    }
                }
            }
            (_, d) => *node = d,
        }
        if s.tree.is_null() {
            s.tree = json!({});
        }
        s.ver += 1;
    }
    cv.notify_all();
    if let Some(f) = notify_lock().lock().unwrap().as_ref() {
        f();
    }
}

/// Версия и весь узел
pub fn snapshot() -> (u64, Value) {
    let s = shared().0.lock().unwrap();
    (s.ver, s.tree.clone())
}

/// Ждать изменений после версии since (для телефона: длинный опрос из Kotlin)
pub fn wait(since: u64, timeout: Duration) -> (u64, Value) {
    let (lock, cv) = shared();
    let s = lock.lock().unwrap();
    let (s, _) = cv.wait_timeout_while(s, timeout, |s| s.ver == since).unwrap();
    (s.ver, s.tree.clone())
}

pub fn state(tree: &Value) -> Option<State> {
    serde_json::from_value(tree.get("state")?.clone()).ok()
}

pub fn devices(tree: &Value) -> Vec<Device> {
    let now = now();
    let mut v: Vec<Device> = tree["devices"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(_, d)| d.is_object())
                .map(|(id, d)| Device {
                    id: id.clone(),
                    name: d["name"].as_str().unwrap_or("?").to_string(),
                    kind: d["kind"].as_str().unwrap_or("pc").to_string(),
                    online: now - d["seen"].as_i64().unwrap_or(0) < ONLINE_MS,
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// Где сейчас идёт сессия: хозяин из state, если он живой
pub fn active_host(tree: &Value) -> Option<String> {
    let host = tree["state"]["host"].as_str()?;
    devices(tree).into_iter().find(|d| d.id == host && d.online).map(|d| d.id)
}

/// Записать состояние сессии. queue: false — без очереди (она большая, а меняется редко)
fn publish(s: &State, queue: bool) -> Result<()> {
    let mut v = json!({
        "host": s.host, "track": s.track, "pos": s.pos, "at": {".sv": "timestamp"},
        "playing": s.playing, "shuffle": s.shuffle, "smart": s.smart, "repeat": s.repeat,
    });
    if queue {
        v["marks"] = json!(s.marks);
        v["played"] = json!(s.played);
        v["upnext"] = json!(s.upnext);
        v["source"] = json!(s.source);
    }
    if let Some(vol) = s.vol {
        v["vol"] = json!(vol);
    }
    if !s.xfer.is_empty() {
        v["xfer"] = json!(s.xfer);
    }
    patch("state", v)
}

/// Команда хозяину сессии
fn send(to: &str, op: &str, a: &Value) -> Result<()> {
    req("POST", "cmd", Some(&json!({"to": to, "op": op, "a": a, "from": me(), "t": {".sv": "timestamp"}}))).map(drop)
}

enum Job {
    Publish(State, bool),
    Send(String, String, Value),
}

/// Запись в базу — в фоне и строго по порядку (иначе старое состояние могло бы лечь поверх нового)
fn queue_job(j: Job) {
    static TX: OnceLock<Mutex<std::sync::mpsc::Sender<Job>>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        std::thread::spawn(move || {
            for j in rx {
                let r = match &j {
                    Job::Publish(s, q) => publish(s, *q),
                    Job::Send(to, op, a) => send(to, op, a),
                };
                if let Err(e) = r {
                    crate::logln!("connect: {e:#}");
                }
            }
        });
        Mutex::new(tx)
    });
    let _ = tx.lock().unwrap().send(j);
}

pub fn publish_bg(s: State, queue: bool) {
    queue_job(Job::Publish(s, queue));
}

pub fn send_bg(to: &str, op: &str, a: Value) {
    queue_job(Job::Send(to.into(), op.into(), a));
}

/// Устройство уходит: остальные сразу видят, что его нет. Если звук шёл тут — сессия встаёт на паузу
pub fn bye(paused: Option<State>) {
    if !RUNNING.load(Ordering::SeqCst) {
        return;
    }
    if let Some(s) = paused {
        let _ = publish(&s, false);
    }
    let _ = patch(&format!("devices/{}", me()), json!({"seen": 0}));
}

/// Свежие команды этому устройству (каждая отдаётся один раз и удаляется из базы)
pub fn take_cmds(tree: &Value) -> Vec<(String, Value)> {
    static DONE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let mut done = DONE.get_or_init(Default::default).lock().unwrap();
    let me = me();
    let now = now();
    let mut out = vec![];
    let mut gone = vec![];
    if let Some(m) = tree["cmd"].as_object() {
        let mut list: Vec<(&String, &Value)> = m.iter().collect();
        list.sort_by_key(|(id, _)| id.to_string()); // id из POST идут по времени
        for (id, c) in list {
            if c["to"].as_str() != Some(me.as_str()) || !done.insert(id.clone()) {
                continue;
            }
            gone.push(id.clone());
            // старьё (устройство было офлайн) не выполняем
            if now - c["t"].as_i64().unwrap_or(0) < 30_000 {
                out.push((c["op"].as_str().unwrap_or("").to_string(), c["a"].clone()));
            }
        }
    }
    if !gone.is_empty() {
        std::thread::spawn(move || {
            for id in gone {
                let _ = delete(&format!("cmd/{id}"));
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_with_nulls() {
        let s: State = serde_json::from_value(json!({"host": "a", "track": null, "upnext": null, "pos": 3.5, "playing": true})).unwrap();
        assert_eq!(s.host, "a");
        assert!(s.track.is_none() && s.upnext.is_empty());
        assert_eq!(s.pos, 3.5);
    }

    #[test]
    fn tree_events() {
        apply("put", "/", json!({"state": {"host": "a", "pos": 1.0}}));
        apply("patch", "/state", json!({"pos": 2.0, "host": null}));
        apply("put", "/cmd/x1", json!({"to": "b", "op": "toggle"}));
        let (_, t) = snapshot();
        assert_eq!(t["state"]["pos"], 2.0);
        assert!(t["state"].get("host").is_none());
        assert_eq!(t["cmd"]["x1"]["op"], "toggle");
        apply("put", "/cmd/x1", Value::Null);
        assert!(snapshot().1["cmd"]["x1"].is_null());
    }
}
