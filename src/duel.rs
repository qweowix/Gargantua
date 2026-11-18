//! «Угадай песню» 1 на 1: два игрока, одни и те же треки, кто первый угадал — тот взял раунд.
//! Realtime Database, узел duels/<код>:
//!   meta  {host, host_name, guest, guest_name, source, created}
//!   pool  [треки по порядку] — порядок перемешал хозяин, у обоих он один
//!   round {n, start, at}      — какая песня (номер в pool) и с какой секунды резать фрагмент (одинаково у обоих)
//!   want/<uid>  = сколько раз игрок нажал «дальше» в этом раунде; уровень фрагмента — меньшее из двух,
//!                 то есть длиннее становится, только когда нажали оба
//!   claim/<uid> = {at, level} — угадал (at — серверное время): раунд берёт тот, у кого at раньше
//!   score/<uid> = очки; пишет только сам игрок, когда взял раунд
//! Новый раунд начинает хозяин: перезаписывает round и чистит want и claim.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::Track;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub host: String,
    pub host_name: String,
    pub guest: String,
    pub guest_name: String,
    pub source: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Round {
    pub n: usize,
    pub start: f64,
    pub at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Claim {
    pub at: i64,
    pub level: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Duel {
    pub code: String,
    pub meta: Meta,
    pub pool: Vec<Track>,
    pub round: Option<Round>,
    pub want: HashMap<String, usize>,
    pub claim: HashMap<String, Claim>,
    pub score: HashMap<String, u32>,
    /// закрыт хозяином
    pub gone: bool,
}

impl Duel {
    pub fn opponent(&self, me: &str) -> String {
        if self.meta.host == me { self.meta.guest.clone() } else { self.meta.host.clone() }
    }

    pub fn name_of(&self, uid: &str) -> String {
        if uid == self.meta.host { self.meta.host_name.clone() } else { self.meta.guest_name.clone() }
    }

    pub fn ready(&self) -> bool {
        !self.meta.guest.is_empty()
    }

    /// Уровень фрагмента: длиннее, только когда «дальше» нажали оба
    pub fn level(&self) -> usize {
        let w = |u: &str| self.want.get(u).copied().unwrap_or(0);
        w(&self.meta.host).min(w(&self.meta.guest))
    }

    /// Кто взял раунд: самый ранний верный ответ по серверному времени
    pub fn winner(&self) -> Option<(String, Claim)> {
        self.claim.iter().min_by_key(|(u, c)| (c.at, (*u).clone())).map(|(u, c)| (u.clone(), c.clone()))
    }

    /// Оба сдались: дальше 5 с уже некуда
    pub fn given_up(&self) -> bool {
        self.level() >= crate::guess::LEVELS.len()
    }
}

fn me() -> Result<(String, String)> {
    let a = crate::cloud::account();
    if !a.verified || a.uid.is_empty() {
        bail!("NOT_VERIFIED");
    }
    Ok((a.uid.clone(), a.email.split('@').next().unwrap_or("?").to_string()))
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
        Err(ureq::Error::Status(code, r)) => bail!("Duel {code}: {}", r.into_string().unwrap_or_default()),
        Err(e) => bail!("NETWORK {e}"),
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

static GEN: AtomicU64 = AtomicU64::new(0);

fn code() -> Result<String> {
    joined().lock().unwrap().as_ref().map(|j| j.code.clone()).context("не в дуэли")
}

/// Хозяин: дуэль с перемешанным списком песен. Треки — без обложек в больших размерах, только нужное
pub fn create(source: &str, pool: &[Track], on_change: impl Fn() + Send + 'static) -> Result<String> {
    let (uid, name) = me()?;
    let code = crate::rooms::new_code();
    let pool: Vec<&Track> = pool.iter().take(300).collect();
    req("PUT", &format!("duels/{code}"), Some(&json!({
        "meta": {"host": uid, "host_name": name, "guest": "", "guest_name": "", "source": source, "created": {".sv": "timestamp"}},
        "pool": pool,
    })))?;
    listen_start(&code, on_change);
    Ok(code)
}

/// Соперник: войти по коду (место одно)
pub fn join(code: &str, on_change: impl Fn() + Send + 'static) -> Result<()> {
    let code = code.trim().to_uppercase();
    let (uid, name) = me()?;
    let meta: Meta = serde_json::from_value(req("GET", &format!("duels/{code}/meta"), None)?).map_err(|_| anyhow!("NO_DUEL"))?;
    if meta.host.is_empty() {
        bail!("NO_DUEL");
    }
    if !meta.guest.is_empty() && meta.guest != uid {
        bail!("DUEL_FULL");
    }
    if meta.host != uid {
        req("PATCH", &format!("duels/{code}/meta"), Some(&json!({"guest": uid, "guest_name": name})))?;
    }
    listen_start(&code, on_change);
    Ok(())
}

fn listen_start(code: &str, on_change: impl Fn() + Send + 'static) {
    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    *joined().lock().unwrap() = Some(Joined { code: code.to_string(), tree: json!({}), gen });
    *notify().lock().unwrap() = Some(Box::new(on_change));
    let code = code.to_string();
    std::thread::spawn(move || {
        while GEN.load(Ordering::SeqCst) == gen {
            if let Err(e) = listen(&code, gen) {
                crate::logln!("дуэль {code}: {e:#}");
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}

/// Выйти. Хозяин закрывает дуэль совсем
pub fn leave() {
    let Some(code) = joined().lock().unwrap().take().map(|j| j.code) else { return };
    GEN.fetch_add(1, Ordering::SeqCst);
    let host = state().map(|d| d.meta.host).unwrap_or_default();
    let me = uid();
    std::thread::spawn(move || {
        let r = if host == me {
            req("DELETE", &format!("duels/{code}"), None)
        } else {
            req("PATCH", &format!("duels/{code}/meta"), Some(&json!({"guest": "", "guest_name": ""})))
        };
        if let Err(e) = r {
            crate::logln!("выход из дуэли: {e:#}");
        }
    });
}

fn listen(code: &str, gen: u64) -> Result<()> {
    let db = crate::connect::db().context("нет базы")?;
    let c = crate::cloud::cfg().context("облако не настроено")?;
    let tok = crate::cloud::auth_token(&c)?;
    static AG: OnceLock<ureq::Agent> = OnceLock::new();
    let ag = AG.get_or_init(|| ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(70)).build());
    let r = ag.get(&format!("{db}/duels/{code}.json")).query("auth", &tok).set("Accept", "text/event-stream").call().map_err(|e| anyhow!("{e}"))?;
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

pub fn parse(code: &str, t: &Value) -> Duel {
    let map = |v: &Value| v.as_object().cloned().unwrap_or_default();
    Duel {
        code: code.into(),
        meta: serde_json::from_value(t["meta"].clone()).unwrap_or_default(),
        pool: serde_json::from_value(t["pool"].clone()).unwrap_or_default(),
        round: t.get("round").filter(|r| r.is_object()).and_then(|r| serde_json::from_value(r.clone()).ok()),
        want: map(&t["want"]).into_iter().map(|(k, v)| (k, v.as_u64().unwrap_or(0) as usize)).collect(),
        claim: map(&t["claim"]).into_iter().filter_map(|(k, v)| Some((k, serde_json::from_value(v).ok()?))).collect(),
        score: map(&t["score"]).into_iter().map(|(k, v)| (k, v.as_u64().unwrap_or(0) as u32)).collect(),
        gone: false,
    }
}

/// Дуэль как её видит сервер; None — не в дуэли
pub fn state() -> Option<Duel> {
    let j = joined().lock().unwrap();
    let j = j.as_ref()?;
    let mut d = parse(&j.code, &j.tree);
    // пока поток не прислал первое состояние — meta пустая, но это не «закрыли»
    d.gone = j.tree.as_object().is_some_and(|o| !o.is_empty()) && j.tree["meta"].is_null();
    Some(d)
}

/// Хозяин: следующий раунд — песня n из общего списка, фрагмент с секунды start
pub fn start_round(n: usize, start: f64) -> Result<()> {
    let c = code()?;
    req("PATCH", &format!("duels/{c}"), Some(&json!({"round": {"n": n, "start": start, "at": {".sv": "timestamp"}}, "want": null, "claim": null})))?;
    Ok(())
}

/// «Дальше»: сколько раз я нажал в этом раунде
pub fn want(times: usize) -> Result<()> {
    let (u, _) = me()?;
    req("PUT", &format!("duels/{}/want/{u}", code()?), Some(&json!(times)))?;
    Ok(())
}

/// Угадал на уровне level — время ставит сервер
pub fn claim(level: usize) -> Result<()> {
    let (u, _) = me()?;
    req("PUT", &format!("duels/{}/claim/{u}", code()?), Some(&json!({"at": {".sv": "timestamp"}, "level": level})))?;
    Ok(())
}

pub fn set_score(total: u32) -> Result<()> {
    let (u, _) = me()?;
    req("PUT", &format!("duels/{}/score/{u}", code()?), Some(&json!(total)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duel_rules() {
        let t = json!({
            "meta": {"host": "h", "host_name": "ann", "guest": "g", "guest_name": "bob"},
            "pool": [{"src": "yt", "id": "a", "title": "A", "artist": "x"}],
            "round": {"n": 0, "start": 31.5, "at": 1},
            "want": {"h": 2, "g": 1},
            "claim": {"g": {"at": 500, "level": 1}, "h": {"at": 700, "level": 1}}
        });
        let d = parse("ABC", &t);
        assert_eq!(d.level(), 1, "длиннее — только когда нажали оба");
        assert_eq!(d.winner().unwrap().0, "g", "раньше по серверному времени");
        assert_eq!(d.opponent("h"), "g");
        assert_eq!(d.name_of("g"), "bob");
        assert!(d.ready() && !d.given_up());
        let mut d2 = d.clone();
        d2.want = [("h".to_string(), 5), ("g".to_string(), 5)].into();
        assert!(d2.given_up());
    }
}
