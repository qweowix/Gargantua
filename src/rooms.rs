//! Listening Rooms — совместное прослушивание с друзьями из разных аккаунтов и стран.
//! Живут в Firebase Realtime Database (той же, что Connect), узел rooms/<код>:
//!   meta    {owner, owner_name, name, public, queue: all|owner, control: all|owner, created}
//!   members/<uid> {name, role, seen, pos, playing, key}       seen — серверное время, раз в 10 с
//!   state   {track, pos, at, playing, by, seq}                 at — серверное время, когда pos был верен
//!   queue/<push-id> {track, by, by_name, ord, at}
//!   votes/<queue-id>/<uid> = true
//!   chat/<push-id> {uid, name, text, emoji, key, at}           реакции и сообщения, хранятся последние 200
//! public_rooms/<код> {name, owner_name, at} — список открытых комнат.
//! Источник правды — сервер: состояние пишет тот, кому можно управлять, все остальные досчитывают позицию
//! по at и поправке часов (connect::now) и подстраиваются. Правила доступа — packaging/database.rules.json.
//! Звук у каждого свой: трек — это ссылка на YouTube/SoundCloud, её открывает каждый у себя.
//! Локальные файлы в комнату не попадают — у других их нет.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::{Src, Track};

pub const ONLINE_MS: i64 = 40_000;
const CHAT_KEEP: usize = 200;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub owner: String,
    pub owner_name: String,
    pub name: String,
    pub public: bool,
    pub queue: String,
    pub control: String,
    pub created: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Member {
    pub uid: String,
    pub name: String,
    pub role: String,
    pub seen: i64,
    pub pos: f64,
    pub playing: bool,
    pub key: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayState {
    pub track: Option<Track>,
    pub pos: f64,
    pub at: i64,
    pub playing: bool,
    pub by: String,
    pub seq: u64,
}

impl PlayState {
    /// Где трек должен быть сейчас по серверным часам
    pub fn pos_now(&self) -> f64 {
        if self.playing && self.at > 0 {
            self.pos + (crate::connect::now() - self.at).max(0) as f64 / 1000.0
        } else {
            self.pos
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QItem {
    pub id: String,
    pub track: Track,
    pub by: String,
    pub by_name: String,
    pub ord: f64,
    pub at: i64,
    pub votes: u32,
    pub mine: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Msg {
    pub id: String,
    pub uid: String,
    pub name: String,
    pub text: String,
    pub emoji: String,
    pub key: String,
    pub at: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Room {
    pub code: String,
    pub meta: Meta,
    pub members: Vec<Member>,
    pub state: Option<PlayState>,
    pub queue: Vec<QItem>,
    pub chat: Vec<Msg>,
}

impl Room {
    pub fn is_owner(&self, uid: &str) -> bool {
        self.meta.owner == uid
    }
    pub fn can_control(&self, uid: &str) -> bool {
        self.is_owner(uid) || self.meta.control == "all"
    }
    pub fn can_queue(&self, uid: &str) -> bool {
        self.is_owner(uid) || self.meta.queue == "all"
    }
    /// Что играть следующим: больше голосов, потом раньше в очереди
    pub fn next(&self) -> Option<&QItem> {
        self.queue.iter().max_by(|a, b| a.votes.cmp(&b.votes).then(b.ord.total_cmp(&a.ord)))
    }
}

struct Joined {
    code: String,
    tree: Value,
    gen: u64,
}

fn joined() -> &'static Mutex<Option<Joined>> {
    static J: OnceLock<Mutex<Option<Joined>>> = OnceLock::new();
    J.get_or_init(|| Mutex::new(None))
}

type Notify = Box<dyn Fn() + Send>;
fn notify() -> &'static Mutex<Option<Notify>> {
    static N: OnceLock<Mutex<Option<Notify>>> = OnceLock::new();
    N.get_or_init(|| Mutex::new(None))
}

/// Каждый вход в комнату — новое поколение: старый поток событий видит, что он лишний, и выходит
static GEN: AtomicU64 = AtomicU64::new(0);

fn me() -> Result<(String, String)> {
    let a = crate::cloud::account();
    if !a.verified || a.uid.is_empty() {
        bail!("NOT_VERIFIED");
    }
    let name = a.email.split('@').next().unwrap_or("?").to_string();
    Ok((a.uid, name))
}

pub fn uid() -> String {
    crate::cloud::account().uid
}

fn req(method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    let db = crate::connect::db().context("нет базы")?;
    let c = crate::cloud::cfg().context("облако не настроено")?;
    me()?;
    let tok = crate::cloud::auth_token(&c)?;
    let r = crate::util::agent().request(method, &format!("{db}/{path}.json")).query("auth", &tok);
    let r = match body {
        Some(b) => r.send_json(b),
        None => r.call(),
    };
    match r {
        Ok(r) => Ok(r.into_json().unwrap_or(Value::Null)),
        Err(ureq::Error::Status(401 | 403, _)) => bail!("PERMISSION"),
        Err(ureq::Error::Status(code, r)) => bail!("Rooms {code}: {}", r.into_string().unwrap_or_default()),
        Err(e) => bail!("NETWORK {e}"),
    }
}

/// Код комнаты: 6 знаков без похожих (0/O, 1/I/L), из системного источника случайности
fn new_code() -> String {
    const AB: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut buf = [0u8; 6];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut buf)).is_err() {
        // Windows: время в наносекундах и адрес на стеке — для кода приглашения этого хватает
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()) as u64;
        let x = t ^ (&buf as *const _ as u64).rotate_left(17);
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (x >> (i * 8)) as u8 ^ (x >> 40) as u8;
        }
    }
    buf.iter().map(|b| AB[*b as usize % AB.len()] as char).collect()
}

pub fn create(name: &str, public: bool, queue_all: bool, control_all: bool) -> Result<String> {
    let (uid, my_name) = me()?;
    let code = new_code();
    let meta = json!({
        "owner": uid, "owner_name": my_name, "name": name.chars().take(60).collect::<String>(), "public": public,
        "queue": if queue_all { "all" } else { "owner" }, "control": if control_all { "all" } else { "owner" },
        "created": {".sv": "timestamp"},
    });
    req("PUT", &format!("rooms/{code}/meta"), Some(&meta))?;
    req("PUT", &format!("rooms/{code}/members/{uid}"), Some(&json!({"name": my_name, "role": "owner", "seen": {".sv": "timestamp"}})))?;
    if public {
        req("PUT", &format!("public_rooms/{code}"), Some(&json!({"name": meta["name"], "owner": uid, "owner_name": my_name, "at": {".sv": "timestamp"}})))?;
    }
    Ok(code)
}

pub fn join(code: &str, on_change: impl Fn() + Send + 'static) -> Result<()> {
    let code = code.trim().to_uppercase();
    let (uid, my_name) = me()?;
    let meta = req("GET", &format!("rooms/{code}/meta"), None)?;
    if meta.is_null() {
        bail!("NO_ROOM");
    }
    let role = if meta["owner"].as_str() == Some(uid.as_str()) { "owner" } else { "member" };
    req("PATCH", &format!("rooms/{code}/members/{uid}"), Some(&json!({"name": my_name, "role": role, "seen": {".sv": "timestamp"}})))?;
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    *joined().lock().unwrap() = Some(Joined { code: code.clone(), tree: json!({}), gen });
    *notify().lock().unwrap() = Some(Box::new(on_change));
    std::thread::spawn(move || {
        while GEN.load(Ordering::SeqCst) == gen {
            if let Err(e) = listen(&code, gen) {
                crate::logln!("комната {code}: {e:#}");
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
    Ok(())
}

/// Выйти. Владелец закрывает комнату совсем
pub fn leave() {
    let Some(code) = joined().lock().unwrap().take().map(|j| j.code) else { return };
    GEN.fetch_add(1, Ordering::SeqCst);
    let (owner, uid) = {
        let r = room();
        (r.as_ref().map(|r| r.meta.owner.clone()).unwrap_or_default(), uid())
    };
    std::thread::spawn(move || {
        let r = if owner == uid {
            req("DELETE", &format!("public_rooms/{code}"), None).and_then(|_| req("DELETE", &format!("rooms/{code}"), None))
        } else {
            req("DELETE", &format!("rooms/{code}/members/{uid}"), None)
        };
        if let Err(e) = r {
            crate::logln!("выход из комнаты: {e:#}");
        }
    });
}

pub fn code() -> Option<String> {
    joined().lock().unwrap().as_ref().map(|j| j.code.clone())
}

fn listen(code: &str, gen: u64) -> Result<()> {
    let db = crate::connect::db().context("нет базы")?;
    let c = crate::cloud::cfg().context("облако не настроено")?;
    let tok = crate::cloud::auth_token(&c)?;
    static AG: OnceLock<ureq::Agent> = OnceLock::new();
    let ag = AG.get_or_init(|| ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(70)).build());
    let r = ag.get(&format!("{db}/rooms/{code}.json")).query("auth", &tok).set("Accept", "text/event-stream").call().map_err(|e| anyhow!("{e}"))?;
    let mut event = String::new();
    for line in BufReader::new(r.into_reader()).lines() {
        if GEN.load(Ordering::SeqCst) != gen {
            return Ok(());
        }
        let line = line?;
        if let Some(e) = line.strip_prefix("event:") {
            event = e.trim().to_string();
        } else if let Some(d) = line.strip_prefix("data:") {
            match event.as_str() {
                "put" | "patch" => {
                    let v: Value = serde_json::from_str(d.trim())?;
                    apply(gen, &event, v["path"].as_str().unwrap_or("/"), v["data"].clone());
                }
                "cancel" | "auth_revoked" => bail!("поток закрыт: {event}"),
                _ => {}
            }
        }
    }
    Ok(())
}

fn apply(gen: u64, kind: &str, path: &str, data: Value) {
    {
        let mut j = joined().lock().unwrap();
        let Some(j) = j.as_mut().filter(|j| j.gen == gen) else { return };
        let mut node = &mut j.tree;
        for p in path.split('/').filter(|p| !p.is_empty()) {
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
        if j.tree.is_null() {
            j.tree = json!({});
        }
    }
    if let Some(f) = notify().lock().unwrap().as_ref() {
        f();
    }
}

/// Комната целиком, как её видит сервер. None — не в комнате; meta.owner пустой — комнату закрыли
pub fn room() -> Option<Room> {
    let j = joined().lock().unwrap();
    let j = j.as_ref()?;
    Some(parse(&j.code, &j.tree, &uid()))
}

pub fn parse(code: &str, t: &Value, me: &str) -> Room {
    let meta: Meta = serde_json::from_value(t["meta"].clone()).unwrap_or_default();
    let mut members: Vec<Member> = t["members"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(uid, v)| Some(Member { uid: uid.clone(), ..serde_json::from_value(v.clone()).ok()? }))
        .collect();
    members.sort_by(|a, b| (a.role != "owner").cmp(&(b.role != "owner")).then(a.name.cmp(&b.name)));
    let votes: HashMap<String, (u32, bool)> = t["votes"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(q, v)| {
            let m = v.as_object();
            (q.clone(), (m.map_or(0, |m| m.len()) as u32, m.is_some_and(|m| m.contains_key(me))))
        })
        .collect();
    let mut queue: Vec<QItem> = t["queue"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, v)| {
            let mut q: QItem = serde_json::from_value(v.clone()).ok()?;
            q.id = id.clone();
            (q.votes, q.mine) = votes.get(id).copied().unwrap_or((0, false));
            Some(q)
        })
        .collect();
    queue.sort_by(|a, b| a.ord.total_cmp(&b.ord).then(a.id.cmp(&b.id)));
    let mut chat: Vec<Msg> = t["chat"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, v)| Some(Msg { id: id.clone(), ..serde_json::from_value(v.clone()).ok()? }))
        .collect();
    chat.sort_by(|a, b| a.id.cmp(&b.id));
    let state = t.get("state").filter(|s| s.is_object()).and_then(|s| serde_json::from_value(s.clone()).ok());
    Room { code: code.into(), meta, members, state, queue, chat }
}

fn path(sub: &str) -> Result<String> {
    Ok(format!("rooms/{}/{sub}", code().context("не в комнате")?))
}

/// Записать состояние плеера комнаты (только тем, кому можно управлять)
pub fn set_state(track: Option<&Track>, pos: f64, playing: bool, seq: u64) -> Result<()> {
    let (uid, _) = me()?;
    req("PUT", &path("state")?, Some(&json!({"track": track, "pos": pos, "at": {".sv": "timestamp"}, "playing": playing, "by": uid, "seq": seq})))?;
    Ok(())
}

/// «Я тут» и что у меня играет — остальные видят статус каждого
pub fn beat(pos: f64, playing: bool, key: &str) -> Result<()> {
    let (uid, _) = me()?;
    req("PATCH", &path(&format!("members/{uid}"))?, Some(&json!({"seen": {".sv": "timestamp"}, "pos": pos, "playing": playing, "key": key})))?;
    Ok(())
}

pub fn shareable(t: &Track) -> bool {
    t.src != Src::Lc && !t.id.is_empty()
}

pub fn enqueue(t: &Track) -> Result<()> {
    if !shareable(t) {
        bail!("LOCAL");
    }
    let (uid, name) = me()?;
    let ord = room().and_then(|r| r.queue.last().map(|q| q.ord + 1.0)).unwrap_or(1.0);
    req("POST", &path("queue")?, Some(&json!({"track": t, "by": uid, "by_name": name, "ord": ord, "at": {".sv": "timestamp"}})))?;
    Ok(())
}

pub fn unqueue(id: &str) -> Result<()> {
    req("DELETE", &path(&format!("queue/{id}"))?, None)?;
    let _ = req("DELETE", &path(&format!("votes/{id}"))?, None);
    Ok(())
}

/// Сдвинуть в очереди: встаёт между соседями (порядок — дробное ord, остальных не трогаем)
pub fn requeue(id: &str, to: usize) -> Result<()> {
    let r = room().context("не в комнате")?;
    let rest: Vec<&QItem> = r.queue.iter().filter(|q| q.id != id).collect();
    let before = to.checked_sub(1).and_then(|i| rest.get(i)).map(|q| q.ord);
    let after = rest.get(to).map(|q| q.ord);
    let ord = match (before, after) {
        (Some(a), Some(b)) => (a + b) / 2.0,
        (Some(a), None) => a + 1.0,
        (None, Some(b)) => b - 1.0,
        (None, None) => 1.0,
    };
    req("PATCH", &path(&format!("queue/{id}"))?, Some(&json!({"ord": ord})))?;
    Ok(())
}

pub fn vote(id: &str, on: bool) -> Result<()> {
    let (uid, _) = me()?;
    let p = path(&format!("votes/{id}/{uid}"))?;
    if on {
        req("PUT", &p, Some(&json!(true)))?;
    } else {
        req("DELETE", &p, None)?;
    }
    Ok(())
}

/// Сообщение или реакция (emoji без текста). key — к какому треку относится
pub fn say(text: &str, emoji: &str, key: &str) -> Result<()> {
    let (uid, name) = me()?;
    let text: String = text.trim().chars().take(300).collect();
    if text.is_empty() && emoji.is_empty() {
        return Ok(());
    }
    req("POST", &path("chat")?, Some(&json!({"uid": uid, "name": name, "text": text, "emoji": emoji, "key": key, "at": {".sv": "timestamp"}})))?;
    // старые сообщения чистит владелец — чат не растёт бесконечно
    if let Some(r) = room().filter(|r| r.is_owner(&uid) && r.chat.len() > CHAT_KEEP) {
        for m in &r.chat[..r.chat.len() - CHAT_KEEP] {
            let _ = req("DELETE", &path(&format!("chat/{}", m.id))?, None);
        }
    }
    Ok(())
}

pub fn set_meta(public: bool, queue_all: bool, control_all: bool) -> Result<()> {
    let r = room().context("не в комнате")?;
    req("PATCH", &path("meta")?, Some(&json!({"public": public, "queue": if queue_all { "all" } else { "owner" }, "control": if control_all { "all" } else { "owner" }})))?;
    if public {
        req("PUT", &format!("public_rooms/{}", r.code), Some(&json!({"name": r.meta.name, "owner": r.meta.owner, "owner_name": r.meta.owner_name, "at": {".sv": "timestamp"}})))?;
    } else {
        req("DELETE", &format!("public_rooms/{}", r.code), None)?;
    }
    Ok(())
}

pub fn kick(uid: &str) -> Result<()> {
    req("DELETE", &path(&format!("members/{uid}"))?, None)?;
    Ok(())
}

/// Открытые комнаты: (код, название, владелец)
pub fn public_rooms() -> Result<Vec<(String, String, String)>> {
    let v = req("GET", "public_rooms", None)?;
    let mut out: Vec<(i64, String, String, String)> = v
        .as_object()
        .into_iter()
        .flatten()
        .map(|(c, r)| (r["at"].as_i64().unwrap_or(0), c.clone(), r["name"].as_str().unwrap_or("?").to_string(), r["owner_name"].as_str().unwrap_or("?").to_string()))
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(out.into_iter().take(30).map(|x| (x.1, x.2, x.3)).collect())
}

/// Насколько подстроить скорость, чтобы догнать комнату: расхождение (мы − комната, с) →
/// None — расхождение мелкое или такое большое, что проще перемотать; иначе скорость на ближайшие секунды
pub fn nudge(drift: f64) -> Option<f64> {
    let a = drift.abs();
    if !(0.15..=2.0).contains(&a) {
        return None;
    }
    Some(if drift > 0.0 { 0.97 } else { 1.03 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_from_tree() {
        let t = json!({
            "meta": {"owner": "u1", "name": "вечер", "public": true, "queue": "all", "control": "owner"},
            "members": {"u2": {"name": "bob", "role": "member", "seen": 1}, "u1": {"name": "ann", "role": "owner", "seen": 1}},
            "queue": {"-b": {"track": {"src": "yt", "id": "b", "title": "B", "artist": "x"}, "ord": 2.0}, "-a": {"track": {"src": "yt", "id": "a", "title": "A", "artist": "x"}, "ord": 1.0}},
            "votes": {"-b": {"u1": true, "u2": true}},
            "chat": {"-2": {"uid": "u2", "text": "огонь"}, "-1": {"uid": "u1", "emoji": "🔥"}},
            "state": {"track": {"src": "yt", "id": "z", "title": "Z", "artist": "x"}, "pos": 10.0, "at": 0, "playing": false}
        });
        let r = parse("ABC123", &t, "u2");
        assert_eq!(r.members[0].name, "ann", "владелец первым");
        assert_eq!(r.queue.iter().map(|q| q.track.id.as_str()).collect::<Vec<_>>(), ["a", "b"], "порядок по ord");
        assert_eq!(r.next().unwrap().track.id, "b", "больше голосов — играет раньше");
        assert!(r.queue[1].mine && r.queue[1].votes == 2);
        assert_eq!(r.chat[0].emoji, "🔥");
        assert!(r.can_queue("u2") && !r.can_control("u2") && r.can_control("u1"));
        assert_eq!(r.state.unwrap().pos_now(), 10.0, "на паузе позиция стоит");
    }

    #[test]
    fn drift_correction() {
        assert_eq!(nudge(0.05), None, "мелочь не трогаем");
        assert_eq!(nudge(0.5), Some(0.97), "убежали вперёд — чуть медленнее");
        assert_eq!(nudge(-0.5), Some(1.03));
        assert_eq!(nudge(5.0), None, "сильно разошлись — перемотка, а не скорость");
        assert_eq!(new_code().len(), 6);
        assert_ne!(new_code(), new_code());
    }
}
