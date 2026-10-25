//! Аккаунт: вход по почте и паролю (Firebase Auth) и плейлисты в облаке (Firestore). Всё через REST,
//! без SDK. Письмо подтверждения шлёт сам Firebase. Проект задаётся при сборке или в окружении:
//! BLACKHOLE_FIREBASE_KEY (Web API key) и BLACKHOLE_FIREBASE_PROJECT (project id); нет их — аккаунтов нет.
//!
//! Синхронизация — трёхстороннее слияние по имени плейлиста: base — хэши того, что было в облаке после
//! прошлой синхронизации. Плейлист не менялся тут — берём облачный (в том числе удаление), не менялся
//! там — берём свой, поменялся в обоих местах — свой.

use crate::model::{Playlist, Store};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Cfg {
    key: String,
    project: String,
}

pub fn cfg() -> Option<Cfg> {
    let get = |name: &str, built: Option<&str>| std::env::var(name).ok().or(built.map(String::from)).filter(|s| !s.trim().is_empty());
    Some(Cfg {
        key: get("BLACKHOLE_FIREBASE_KEY", option_env!("BLACKHOLE_FIREBASE_KEY"))?,
        project: get("BLACKHOLE_FIREBASE_PROJECT", option_env!("BLACKHOLE_FIREBASE_PROJECT"))?,
    })
}

/// Что лежит в account.json: кто вошёл и что было в облаке после прошлой синхронизации
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Account {
    pub email: String,
    pub uid: String,
    pub refresh: String,
    pub verified: bool,
    /// имя плейлиста → хэш его облачной версии
    pub base: HashMap<String, u64>,
}

impl Account {
    pub fn signed_in(&self) -> bool {
        !self.refresh.is_empty()
    }
}

fn acc_path() -> std::path::PathBuf {
    Store::dir().join("account.json")
}

fn acc() -> &'static Mutex<Account> {
    static A: OnceLock<Mutex<Account>> = OnceLock::new();
    A.get_or_init(|| Mutex::new(std::fs::read(acc_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()))
}

pub fn account() -> Account {
    acc().lock().unwrap().clone()
}

pub fn update(f: impl FnOnce(&mut Account)) {
    let mut a = acc().lock().unwrap();
    f(&mut a);
    let p = acc_path();
    let _ = std::fs::create_dir_all(Store::dir());
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(&*a).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(tmp, p);
    }
}

pub fn sign_out() {
    *token() = None;
    update(|a| *a = Account::default());
}

// ── Auth ──

/// id-токен живёт час, держим его только в памяти
fn token() -> std::sync::MutexGuard<'static, Option<(String, Instant)>> {
    static T: Mutex<Option<(String, Instant)>> = Mutex::new(None);
    T.lock().unwrap()
}

/// Код ошибки Firebase ("EMAIL_EXISTS", "WEAK_PASSWORD : ...") — его переводит вызывающий
fn post(url: &str, lang: &str, body: Value) -> Result<Value> {
    match crate::util::agent().post(url).set("X-Firebase-Locale", lang).send_json(body) {
        Ok(r) => Ok(r.into_json()?),
        Err(ureq::Error::Status(code, r)) => {
            let v: Value = r.into_json().unwrap_or_default();
            bail!("{}", v["error"]["message"].as_str().map_or_else(|| format!("HTTP {code}"), String::from))
        }
        Err(e) => Err(anyhow!("NETWORK {e}")),
    }
}

fn auth(c: &Cfg, method: &str, lang: &str, body: Value) -> Result<Value> {
    post(&format!("https://identitytoolkit.googleapis.com/v1/accounts:{method}?key={}", c.key), lang, body)
}

fn keep_session(email: &str, v: &Value) -> Result<()> {
    let (Some(id), Some(refresh), Some(uid)) = (v["idToken"].as_str(), v["refreshToken"].as_str(), v["localId"].as_str()) else {
        bail!("bad answer")
    };
    let secs = v["expiresIn"].as_str().and_then(|s| s.parse().ok()).unwrap_or(3600u64);
    *token() = Some((id.to_string(), Instant::now() + Duration::from_secs(secs.saturating_sub(60))));
    let same = account().uid == uid;
    update(|a| {
        if !same {
            *a = Account::default(); // другой аккаунт — прошлая база слияния не про него
        }
        a.email = email.to_string();
        a.uid = uid.to_string();
        a.refresh = refresh.to_string();
    });
    Ok(())
}

/// Регистрация и сразу письмо подтверждения
pub fn sign_up(c: &Cfg, lang: &str, email: &str, pw: &str) -> Result<()> {
    let v = auth(c, "signUp", lang, json!({"email": email, "password": pw, "returnSecureToken": true}))?;
    keep_session(email, &v)?;
    send_verify(c, lang)
}

/// Вход. Ok(true) — почта подтверждена
pub fn sign_in(c: &Cfg, lang: &str, email: &str, pw: &str) -> Result<bool> {
    let v = auth(c, "signInWithPassword", lang, json!({"email": email, "password": pw, "returnSecureToken": true}))?;
    keep_session(email, &v)?;
    check_verified(c, lang)
}

pub fn send_verify(c: &Cfg, lang: &str) -> Result<()> {
    let id = id_token(c, false)?;
    auth(c, "sendOobCode", lang, json!({"requestType": "VERIFY_EMAIL", "idToken": id}))?;
    Ok(())
}

pub fn send_reset(c: &Cfg, lang: &str, email: &str) -> Result<()> {
    auth(c, "sendOobCode", lang, json!({"requestType": "PASSWORD_RESET", "email": email}))?;
    Ok(())
}

/// Подтвердил ли почту. Если да — токен обновляется: в старом email_verified ещё false, и Firestore его не пустит
pub fn check_verified(c: &Cfg, lang: &str) -> Result<bool> {
    let id = id_token(c, false)?;
    let v = auth(c, "lookup", lang, json!({"idToken": id}))?;
    let ok = v["users"][0]["emailVerified"].as_bool().unwrap_or(false);
    if ok && !account().verified {
        id_token(c, true)?;
    }
    update(|a| a.verified = ok);
    Ok(ok)
}

fn id_token(c: &Cfg, fresh: bool) -> Result<String> {
    if !fresh {
        if let Some((t, _)) = token().as_ref().filter(|(_, exp)| Instant::now() < *exp) {
            return Ok(t.clone());
        }
    }
    let refresh = account().refresh;
    if refresh.is_empty() {
        bail!("NOT_SIGNED_IN");
    }
    let url = format!("https://securetoken.googleapis.com/v1/token?key={}", c.key);
    let v: Value = match crate::util::agent().post(&url).send_form(&[("grant_type", "refresh_token"), ("refresh_token", &refresh)]) {
        Ok(r) => r.into_json()?,
        Err(ureq::Error::Status(code, r)) => {
            let v: Value = r.into_json().unwrap_or_default();
            bail!("{}", v["error"]["message"].as_str().map_or_else(|| format!("HTTP {code}"), String::from))
        }
        Err(e) => bail!("NETWORK {e}"),
    };
    let id = v["id_token"].as_str().ok_or_else(|| anyhow!("bad answer"))?.to_string();
    let secs = v["expires_in"].as_str().and_then(|s| s.parse().ok()).unwrap_or(3600u64);
    *token() = Some((id.clone(), Instant::now() + Duration::from_secs(secs.saturating_sub(60))));
    if let Some(r) = v["refresh_token"].as_str().filter(|r| *r != refresh) {
        update(|a| a.refresh = r.to_string());
    }
    Ok(id)
}

// ── Firestore ──

fn doc_url(c: &Cfg, uid: &str) -> String {
    format!("https://firestore.googleapis.com/v1/projects/{}/databases/(default)/documents/users/{uid}", c.project)
}

/// Плейлисты из облака и время версии документа. None — документа ещё нет
fn pull(c: &Cfg, uid: &str, id: &str) -> Result<Option<(Vec<Playlist>, String)>> {
    let v: Value = match crate::util::agent().get(&doc_url(c, uid)).set("Authorization", &format!("Bearer {id}")).call() {
        Ok(r) => r.into_json()?,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(code, r)) => bail!("Firestore {code}: {}", r.into_string().unwrap_or_default()),
        Err(e) => bail!("NETWORK {e}"),
    };
    let pls = match v["fields"]["pl"]["bytesValue"].as_str() {
        Some(b) => {
            let z = b64_decode(b).ok_or_else(|| anyhow!("bad base64"))?;
            let raw = miniz_oxide::inflate::decompress_to_vec(&z).map_err(|e| anyhow!("inflate: {e:?}"))?;
            serde_json::from_slice(&raw)?
        }
        None => vec![],
    };
    Ok(Some((pls, v["updateTime"].as_str().unwrap_or_default().to_string())))
}

/// Записать, только если в облаке всё ещё версия prev (иначе кто-то успел раньше — Ok(None), сливаем заново)
fn push(c: &Cfg, uid: &str, id: &str, pls: &[Playlist], prev: Option<&str>) -> Result<Option<()>> {
    let pls: Vec<Playlist> = pls.iter().map(strip).collect();
    let z = miniz_oxide::deflate::compress_to_vec(&serde_json::to_vec(&pls)?, 8);
    let body = json!({"fields": {"pl": {"bytesValue": b64_encode(&z)}, "v": {"integerValue": "1"}}});
    let req = crate::util::agent().request("PATCH", &doc_url(c, uid)).set("Authorization", &format!("Bearer {id}"));
    let req = match prev {
        Some(t) => req.query("currentDocument.updateTime", t),
        None => req.query("currentDocument.exists", "false"),
    };
    match req.send_json(body) {
        Ok(_) => Ok(Some(())),
        Err(ureq::Error::Status(400 | 404 | 409, r)) => {
            let s = r.into_string().unwrap_or_default();
            if s.contains("FAILED_PRECONDITION") || s.contains("ALREADY_EXISTS") || s.contains("NOT_FOUND") {
                Ok(None)
            } else {
                bail!("Firestore: {s}")
            }
        }
        Err(ureq::Error::Status(code, r)) => bail!("Firestore {code}: {}", r.into_string().unwrap_or_default()),
        Err(e) => bail!("NETWORK {e}"),
    }
}

pub struct Synced {
    /// что теперь должно быть у нас (уже совпадает с облаком)
    pub playlists: Vec<Playlist>,
    pub base: HashMap<String, u64>,
}

/// Слить свои плейлисты с облачными и записать результат в облако. Базу не трогает: её коммитит
/// вызывающий, когда применит результат (если за время запроса плейлисты поменялись — не применит)
pub fn sync(c: &Cfg, local: &[Playlist]) -> Result<Synced> {
    let a = account();
    if !a.verified {
        bail!("NOT_VERIFIED");
    }
    let id = id_token(c, false)?;
    for _ in 0..3 {
        let (remote, ut) = match pull(c, &a.uid, &id)? {
            Some((p, t)) => (p, Some(t)),
            None => (vec![], None),
        };
        let merged = merge(local, &remote, &a.base);
        if hashes(&merged) != hashes(&remote) && push(c, &a.uid, &id, &merged, ut.as_deref())?.is_none() {
            continue;
        }
        let base = hashes(&merged).into_iter().collect();
        return Ok(Synced { playlists: merged, base });
    }
    bail!("облако всё время меняется, попробую позже")
}

/// Своя обложка — файл на этом компьютере, в облако не едет
fn strip(p: &Playlist) -> Playlist {
    Playlist { cover: String::new(), ..p.clone() }
}

/// FNV-1a: в отличие от DefaultHasher не меняется между версиями Rust, база в account.json не протухнет
fn hash(p: &Playlist) -> u64 {
    serde_json::to_vec(&strip(p)).unwrap_or_default().iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
}

pub fn hashes(pls: &[Playlist]) -> Vec<(String, u64)> {
    pls.iter().map(|p| (p.name.clone(), hash(p))).collect()
}

pub fn merge(local: &[Playlist], remote: &[Playlist], base: &HashMap<String, u64>) -> Vec<Playlist> {
    let find = |v: &[Playlist], n: &str| v.iter().find(|p| p.name == n).cloned();
    let pick = |name: &str| -> Option<Playlist> {
        let (l, r) = (find(local, name), find(remote, name));
        let b = base.get(name).copied();
        let (lh, rh) = (l.as_ref().map(hash), r.as_ref().map(hash));
        if lh == b {
            // тут не трогали — как в облаке; своя обложка остаётся
            r.map(|r| Playlist { cover: l.map(|l| l.cover).unwrap_or_default(), ..r })
        } else if rh == b {
            l
        } else {
            l.or(r)
        }
    };
    let mut out: Vec<Playlist> = local.iter().filter_map(|p| pick(&p.name)).collect();
    out.extend(remote.iter().filter(|p| !local.iter().any(|l| l.name == p.name)).filter_map(|p| pick(&p.name)));
    out
}

// ── base64 ──

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { B64[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut n, mut bits) = (0u32, 0);
    for ch in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = match ch {
            b'-' => 62, // Firestore может отдать и url-safe
            b'_' => 63,
            _ => B64.iter().position(|&x| x == ch)? as u32,
        };
        n = n << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Src, Track};

    fn pl(name: &str, ids: &[&str]) -> Playlist {
        Playlist { name: name.into(), tracks: ids.iter().map(|i| Track { src: Src::Yt, id: i.to_string(), ..Default::default() }).collect(), ..Default::default() }
    }

    fn names(v: &[Playlist]) -> Vec<String> {
        v.iter().map(|p| format!("{}:{}", p.name, p.tracks.len())).collect()
    }

    #[test]
    fn base64_roundtrip() {
        for n in 0..20 {
            let d: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&d)).unwrap(), d);
        }
        assert_eq!(b64_encode(b"hi!?"), "aGkhPw==");
    }

    #[test]
    fn merge_three_way() {
        let base_list = [pl("a", &["1"]), pl("b", &["1"]), pl("c", &["1"])];
        let base: HashMap<_, _> = hashes(&base_list).into_iter().collect();
        // тут: в a добавили трек, c удалили, завели d. Там: в b добавили трек, завели e
        let local = [pl("a", &["1", "2"]), pl("b", &["1"]), pl("d", &["1"])];
        let remote = [pl("a", &["1"]), pl("b", &["1", "2", "3"]), pl("c", &["1"]), pl("e", &["1"])];
        assert_eq!(names(&merge(&local, &remote, &base)), ["a:2", "b:3", "d:1", "e:1"]);
    }

    #[test]
    fn merge_first_login_keeps_both() {
        // база пустая: новый компьютер со своими плейлистами и облако с другими — ничего не пропадает
        let local = [pl("mine", &["1"]), pl("same", &["1"])];
        let remote = [pl("cloud", &["1"]), pl("same", &["1", "2"])];
        assert_eq!(names(&merge(&local, &remote, &HashMap::new())), ["mine:1", "same:1", "cloud:1"]);
    }

    #[test]
    fn merge_remote_delete_and_local_cover_kept() {
        let base_list = [pl("a", &["1"]), pl("b", &["1"])];
        let base: HashMap<_, _> = hashes(&base_list).into_iter().collect();
        let mut a = pl("a", &["1"]);
        a.cover = "/covers/a.png".into();
        let local = [a, pl("b", &["1"])];
        let remote = [pl("a", &["1", "9"])]; // b удалили на другом компьютере
        let m = merge(&local, &remote, &base);
        assert_eq!(names(&m), ["a:2"]);
        assert_eq!(m[0].cover, "/covers/a.png");
    }
}
