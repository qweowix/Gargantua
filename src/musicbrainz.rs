//! MusicBrainz — открытая база музыкальных метаданных (данные CC0). Отсюда подтверждённые связи:
//! участники групп, соавторы, продюсеры и музыканты записи, альбом и лейбл, жанры артиста.
//! Их правило — не больше запроса в секунду и внятный User-Agent. Ответы кэшируются в mb.json
//! на 30 дней (пустые — на 7), повторно ради того же артиста сеть не трогаем.
//! Включается в настройках: имена артистов и треков из библиотеки уходят на musicbrainz.org.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const API: &str = "https://musicbrainz.org/ws/2";
const FRESH: i64 = 30 * 86_400;
const FRESH_EMPTY: i64 = 7 * 86_400;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Person {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MbArtist {
    pub id: String,
    pub name: String,
    /// жанры с числом голосов редакторов
    pub genres: Vec<(String, u32)>,
    /// участники группы (или группы, где артист участник)
    pub members: Vec<Person>,
    /// совместные проекты и связанные артисты
    pub collabs: Vec<Person>,
    pub at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Credit {
    /// producer, mix, recording, instrument, vocal…
    pub role: String,
    /// уточнение: на чём играл («guitar»), какой вокал
    pub detail: String,
    pub who: Person,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MbRec {
    pub id: String,
    pub credits: Vec<Credit>,
    /// альбом (release group) и его тип: Album, Single, EP
    pub album: Option<(String, String)>,
    pub album_kind: String,
    pub label: Option<Person>,
    pub at: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Cache {
    artists: HashMap<String, MbArtist>,
    recs: HashMap<String, MbRec>,
}

fn file() -> std::path::PathBuf {
    crate::model::Store::dir().join("mb.json")
}

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()))
}

fn save() {
    let c = cache().lock().unwrap();
    if let Ok(b) = serde_json::to_vec(&*c) {
        let tmp = file().with_extension("tmp");
        if std::fs::write(&tmp, b).is_ok() {
            let _ = std::fs::rename(tmp, file());
        }
    }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn fresh(at: i64, empty: bool) -> bool {
    now() - at < if empty { FRESH_EMPTY } else { FRESH }
}

/// Не чаще запроса в 1.1 с на весь процесс
fn get(path: &str, query: &[(&str, &str)]) -> Result<Value> {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut l = LAST.lock().unwrap();
        if let Some(t) = *l {
            let wait = Duration::from_millis(1100).saturating_sub(t.elapsed());
            std::thread::sleep(wait);
        }
        *l = Some(Instant::now());
    }
    let mut r = crate::util::agent()
        .get(&format!("{API}/{path}"))
        .set("User-Agent", concat!("Gargantua/", env!("CARGO_PKG_VERSION"), " ( https://github.com/qweowix/Gargantua )"))
        .set("Accept", "application/json")
        .query("fmt", "json");
    for (k, v) in query {
        r = r.query(k, v);
    }
    match r.call() {
        Ok(r) => Ok(r.into_json()?),
        Err(ureq::Error::Status(503, _)) => bail!("MusicBrainz просит помедленнее"),
        Err(ureq::Error::Status(c, _)) => bail!("MusicBrainz {c}"),
        Err(e) => bail!("MusicBrainz: {e}"),
    }
}

/// Кавычки в запросе Lucene
fn q(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn person(v: &Value) -> Person {
    Person { id: v["id"].as_str().unwrap_or_default().into(), name: v["name"].as_str().unwrap_or_default().into() }
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Артист по имени: берём только уверенное совпадение (score ≥ 95 и то же имя), иначе пусто — чужой
/// тёзка дал бы ложные связи
pub fn artist(name: &str) -> Result<MbArtist> {
    let k = norm(name);
    if let Some(a) = cache().lock().unwrap().artists.get(&k).filter(|a| fresh(a.at, a.id.is_empty())) {
        return Ok(a.clone());
    }
    let found = get("artist", &[("query", &format!("artist:\"{}\"", q(name))), ("limit", "3")])?;
    let hit = found["artists"].as_array().into_iter().flatten().find(|a| a["score"].as_i64().unwrap_or(0) >= 95 && norm(a["name"].as_str().unwrap_or_default()) == k);
    let mut out = MbArtist { name: name.into(), at: now(), ..Default::default() };
    if let Some(id) = hit.and_then(|h| h["id"].as_str()) {
        let v = get(&format!("artist/{id}"), &[("inc", "artist-rels+genres")])?;
        out.id = id.into();
        out.name = v["name"].as_str().unwrap_or(name).into();
        out.genres = v["genres"].as_array().into_iter().flatten().filter_map(|g| Some((g["name"].as_str()?.to_string(), g["count"].as_u64().unwrap_or(0) as u32))).collect();
        out.genres.sort_by(|a, b| b.1.cmp(&a.1));
        for r in v["relations"].as_array().into_iter().flatten() {
            let p = person(&r["artist"]);
            if p.id.is_empty() {
                continue;
            }
            match r["type"].as_str().unwrap_or_default() {
                "member of band" => out.members.push(p),
                "collaboration" | "supporting musician" | "vocal supporting musician" | "instrumental supporting musician" => out.collabs.push(p),
                _ => {}
            }
        }
    }
    cache().lock().unwrap().artists.insert(k, out.clone());
    save();
    Ok(out)
}

/// Запись (трек): кто продюсировал, кто играл, в каком она альбоме и на каком лейбле
pub fn recording(key: &str, title: &str, artist: &str, dur_s: u64) -> Result<MbRec> {
    if let Some(r) = cache().lock().unwrap().recs.get(key).filter(|r| fresh(r.at, r.id.is_empty())) {
        return Ok(r.clone());
    }
    let main = crate::stats::artists(artist).first().copied().unwrap_or(artist).to_string();
    let found = get("recording", &[("query", &format!("recording:\"{}\" AND artist:\"{}\"", q(title), q(&main))), ("limit", "5")])?;
    // то же название и длина ±5 с (если MusicBrainz её знает): у песни бывают каверы и ремиксы с тем же именем
    let hit = found["recordings"].as_array().into_iter().flatten().find(|r| {
        r["score"].as_i64().unwrap_or(0) >= 90
            && norm(r["title"].as_str().unwrap_or_default()) == norm(title)
            && r["length"].as_u64().is_none_or(|ms| dur_s == 0 || (ms as i64 / 1000 - dur_s as i64).abs() <= 5)
    });
    let mut out = MbRec { at: now(), ..Default::default() };
    if let Some(h) = hit {
        let id = h["id"].as_str().unwrap_or_default();
        out.id = id.into();
        // альбом: предпочитаем настоящий Album, а не сингл или сборник
        let rels: Vec<&Value> = h["releases"].as_array().into_iter().flatten().collect();
        let pick = rels
            .iter()
            .find(|r| r["release-group"]["primary-type"] == "Album" && r["release-group"]["secondary-types"].as_array().is_none_or(|v| v.is_empty()))
            .or_else(|| rels.first());
        let mut release = None;
        if let Some(r) = pick {
            let g = &r["release-group"];
            if let (Some(gid), Some(t)) = (g["id"].as_str(), g["title"].as_str()) {
                out.album = Some((gid.into(), t.into()));
                out.album_kind = g["primary-type"].as_str().unwrap_or_default().into();
            }
            release = r["id"].as_str().map(String::from);
        }
        let v = get(&format!("recording/{id}"), &[("inc", "artist-rels")])?;
        for r in v["relations"].as_array().into_iter().flatten() {
            let who = person(&r["artist"]);
            if who.id.is_empty() {
                continue;
            }
            let role = r["type"].as_str().unwrap_or_default();
            if !matches!(role, "producer" | "co-producer" | "mix" | "recording" | "engineer" | "instrument" | "vocal" | "arranger" | "programming") {
                continue;
            }
            let detail = r["attributes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ");
            out.credits.push(Credit { role: role.into(), detail, who });
        }
        if let Some(rid) = release {
            let v = get(&format!("release/{rid}"), &[("inc", "labels")])?;
            out.label = v["label-info"].as_array().into_iter().flatten().map(|l| person(&l["label"])).find(|p| !p.id.is_empty() && p.name != "[no label]");
        }
    }
    cache().lock().unwrap().recs.insert(key.into(), out.clone());
    save();
    Ok(out)
}

/// Все артисты и записи из кэша (для графа)
pub fn snapshot() -> (HashMap<String, MbArtist>, HashMap<String, MbRec>) {
    let c = cache().lock().unwrap();
    (c.artists.clone(), c.recs.clone())
}

/// Подтянуть по списку (блокирует — звать из фонового потока): сначала артисты, потом записи.
/// Свежее в кэше пропускается без сети. done — после каждого нового ответа (перестроить граф).
/// Пять ошибок подряд — бросаем: сеть легла или MusicBrainz нас притормозил
pub fn fetch_blocking(artists: &[String], recs: &[crate::model::Track], done: &dyn Fn()) {
    let mut fails = 0;
    for a in artists {
        if fails > 5 {
            return;
        }
        if cache().lock().unwrap().artists.get(&norm(a)).is_some_and(|x| fresh(x.at, x.id.is_empty())) {
            continue;
        }
        match artist(a) {
            Ok(_) => {
                fails = 0;
                done();
            }
            Err(e) => {
                crate::logln!("MusicBrainz {a}: {e:#}");
                fails += 1;
            }
        }
    }
    for t in recs {
        if fails > 5 {
            return;
        }
        let k = t.key();
        if cache().lock().unwrap().recs.get(&k).is_some_and(|x| fresh(x.at, x.id.is_empty())) {
            continue;
        }
        match recording(&k, &t.title, &t.artist, t.dur) {
            Ok(_) => {
                fails = 0;
                done();
            }
            Err(e) => {
                crate::logln!("MusicBrainz {}: {e:#}", t.title);
                fails += 1;
            }
        }
    }
}
