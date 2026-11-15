use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::model::{Src, Track};
use crate::util::{agent, call};

static CID: Mutex<Option<String>> = Mutex::new(None);

/// Публичный client_id выдираем из JS самого soundcloud.com (как yt-dlp)
fn client_id(fresh: bool) -> Result<String> {
    let mut g = CID.lock().unwrap();
    if !fresh {
        if let Some(c) = g.as_ref() {
            return Ok(c.clone());
        }
    }
    let html = call(agent().get("https://soundcloud.com/"))?.into_string()?;
    let scripts: Vec<&str> = html
        .match_indices("https://a-v2.sndcdn.com/assets/")
        .filter_map(|(i, _)| html[i..].find(".js").map(|e| &html[i..i + e + 3]))
        .collect();
    for s in scripts.into_iter().rev() {
        let Ok(js) = agent().get(s).call().map_err(anyhow::Error::from).and_then(|r| Ok(r.into_string()?)) else {
            continue;
        };
        if let Some(id) = find_id(&js) {
            *g = Some(id.clone());
            return Ok(id);
        }
    }
    bail!("не нашёл client_id SoundCloud")
}

fn find_id(js: &str) -> Option<String> {
    for pat in ["client_id:\"", "client_id=\"", "client_id="] {
        for (i, _) in js.match_indices(pat) {
            let id: String = js[i + pat.len()..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if id.len() == 32 {
                return Some(id);
            }
        }
    }
    None
}

fn get(path: &str, params: &[(&str, &str)]) -> Result<Value> {
    for attempt in 0..2 {
        let cid = client_id(attempt == 1)?;
        let mut req = agent().get(&format!("https://api-v2.soundcloud.com{path}")).query("client_id", &cid);
        for (k, v) in params {
            req = req.query(k, v);
        }
        match call(req) {
            Ok(r) => return Ok(r.into_json()?),
            Err(ureq::Error::Status(401 | 403, _)) if attempt == 0 => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("SoundCloud не пускает")
}

fn parse(t: &Value) -> Option<Track> {
    let id = t["id"].as_u64()?.to_string();
    let title = t["title"].as_str()?.to_string();
    let artist = t["publisher_metadata"]["artist"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or(t["user"]["username"].as_str())
        .unwrap_or("")
        .to_string();
    let cover = t["artwork_url"].as_str().or(t["user"]["avatar_url"].as_str()).unwrap_or("").replace("-large.", "-t300x300.");
    let policy = t["policy"].as_str().unwrap_or("");
    Some(Track { artist_id: String::new(),
        src: Src::Sc,
        id,
        title,
        artist,
        dur: t["full_duration"].as_u64().or(t["duration"].as_u64()).unwrap_or(0) / 1000,
        cover,
        url: t["permalink_url"].as_str().unwrap_or("").to_string(),
        snip: policy == "SNIP" || policy == "BLOCK",
    })
}

pub fn search(q: &str, limit: usize) -> Result<Vec<Track>> {
    let v = get("/search/tracks", &[("q", q), ("limit", &limit.to_string())])?;
    Ok(v["collection"].as_array().map(|a| a.iter().filter_map(parse).collect()).unwrap_or_default())
}

fn norm(s: &str) -> String {
    crate::lyrics::clean(s).to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Та же песня на SoundCloud — запасной путь, когда YouTube не даёт звук ни так, ни эдак.
/// Строго: название совпадает, исполнитель в имени автора или в названии, длительность ±3 с.
/// Лучше ничего, чем чужая песня
#[allow(dead_code)] // зовёт только телефон
pub fn find_same(t: &Track) -> Result<Option<Track>> {
    let (want_t, want_a) = (norm(&t.title), norm(t.artist.split([',', '&']).next().unwrap_or(&t.artist)));
    if want_t.is_empty() {
        return Ok(None);
    }
    let hits = search(&format!("{} {}", crate::lyrics::clean(&t.artist), crate::lyrics::clean(&t.title)), 10)?;
    Ok(hits
        .into_iter()
        .filter(|h| !h.snip && !h.url.is_empty())
        .filter(|h| t.dur == 0 || h.dur.abs_diff(t.dur) <= 3)
        .filter(|h| {
            let (ht, ha) = (norm(&h.title), norm(&h.artist));
            // «Артист - Песня» в названии у перезаливов — тоже годится
            let title_ok = ht == want_t || ht == format!("{want_a}{want_t}");
            title_ok && (want_a.is_empty() || ha.contains(&want_a) || ht.starts_with(&want_a))
        })
        .min_by_key(|h| h.dur.abs_diff(t.dur)))
}

/// Аватар автора трека
pub fn avatar(url: &str) -> Result<String> {
    let v = get("/resolve", &[("url", url)])?;
    Ok(v["user"]["avatar_url"].as_str().unwrap_or("").replace("-large.", "-t300x300."))
}

/// Прямая ссылка на звук трека (для телефона: на ПК это делает yt-dlp). Обычный файл, если есть, иначе HLS:
/// многие треки SoundCloud отдаёт только так. Зашифрованные варианты (DRM) не сыграть
#[allow(dead_code)] // зовёт только телефон
pub fn stream(url: &str) -> Result<String> {
    let v = get("/resolve", &[("url", url)])?;
    let auth = v["track_authorization"].as_str().unwrap_or("");
    // обычный файл лучше HLS, дальше — кто с большим битрейтом (из имени: aac_160k, mp3_128)
    let rank = |t: &&Value| {
        let kind = match t["format"]["protocol"].as_str() {
            Some("progressive") => 2,
            Some("hls") => 1,
            _ => 0,
        };
        let rate: String = t["preset"].as_str().unwrap_or("").chars().filter(char::is_ascii_digit).collect();
        (kind, rate.parse::<u32>().unwrap_or(0))
    };
    let Some(best) = v["media"]["transcodings"].as_array().and_then(|a| a.iter().filter(|t| !t["snipped"].as_bool().unwrap_or(false) && rank(t).0 > 0).max_by_key(rank)) else {
        bail!("SoundCloud не отдаёт звук этого трека");
    };
    let link = best["url"].as_str().unwrap_or("");
    for attempt in 0..2 {
        let cid = client_id(attempt == 1)?;
        match call(agent().get(link).query("client_id", &cid).query("track_authorization", auth)) {
            Ok(r) => {
                let j: Value = r.into_json()?;
                return j["url"].as_str().map(String::from).ok_or_else(|| anyhow::anyhow!("SoundCloud не дал ссылку"));
            }
            Err(ureq::Error::Status(401 | 403, _)) if attempt == 0 => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("SoundCloud не пускает")
}

pub fn import(url: &str) -> Result<(String, Vec<Track>)> {
    let v = get("/resolve", &[("url", url)])?;
    match v["kind"].as_str() {
        Some("track") => Ok((v["title"].as_str().unwrap_or("SoundCloud").into(), parse(&v).into_iter().collect())),
        Some("playlist" | "system-playlist") => {
            let raw = v["tracks"].as_array().cloned().unwrap_or_default();
            // в ответе полные данные только у первых треков, остальные — голые id
            let missing: Vec<String> =
                raw.iter().filter(|t| t["title"].is_null()).filter_map(|t| t["id"].as_u64().map(|i| i.to_string())).collect();
            let mut full: HashMap<String, Track> = HashMap::new();
            for chunk in missing.chunks(50) {
                if let Ok(r) = get("/tracks", &[("ids", &chunk.join(","))]) {
                    for t in r.as_array().into_iter().flatten().filter_map(parse) {
                        full.insert(t.id.clone(), t);
                    }
                }
            }
            let tracks = raw
                .iter()
                .filter_map(|t| parse(t).or_else(|| full.remove(&t["id"].as_u64()?.to_string())))
                .collect();
            Ok((v["title"].as_str().unwrap_or("SoundCloud").into(), tracks))
        }
        _ => bail!("это не трек и не плейлист SoundCloud"),
    }
}

#[cfg(test)]
mod live {
    /// Живой SoundCloud: какие варианты звука у треков и что выбрали. Не гонять в цикле
    #[test]
    #[ignore]
    fn stream_picks() {
        for t in super::search("one way ticket slowed", 8).unwrap() {
            let v = super::get("/resolve", &[("url", &t.url)]).unwrap();
            let kinds: Vec<String> = v["media"]["transcodings"].as_array().unwrap().iter().map(|x| format!("{}/{}/{}", x["preset"], x["format"]["protocol"], x["snipped"])).collect();
            println!("{} {}s snip={} {:?}", t.title, t.dur, t.snip, kinds);
            println!("  -> {:?}", super::stream(&t.url).map(|u| u.chars().take(70).collect::<String>()));
        }
    }
}
