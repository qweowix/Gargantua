//! Spicy Lyrics (api.spicylyrics.org): community-синки по слогам, Apple Music и Spotify из одной ручки.
//! Нужен ключ (своё приложение в их дашборде) и Spotify ID трека — для треков с YouTube/SoundCloud
//! находим его поиском Spotify. И ID, и ответы кешируем на диск, чтобы не тратить лимит ключа.

use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::lyrics::{cache_get, cache_put, Credit, Found, Line, Lyrics, Word, FOREVER};
use crate::model::{Src, Track};
use crate::util::{agent, call};

const DAY: Duration = Duration::from_secs(86400);

/// Ключ из настроек, иначе из переменной окружения (как советуют их доки)
pub fn key(from_settings: &str) -> Option<String> {
    let k = from_settings.trim();
    if !k.is_empty() {
        return Some(k.to_string());
    }
    std::env::var("SPICY_LYRICS_SECRET_KEY").ok().filter(|k| !k.trim().is_empty())
}

fn spotify_id(t: &Track) -> Result<Option<String>> {
    if t.src == Src::Sp {
        return Ok(Some(t.id.clone()));
    }
    let name = format!("spotify-id\n{}", t.key());
    if let Some(v) = cache_get(&name, FOREVER) {
        return Ok(v.as_str().map(String::from));
    }
    let id = crate::spotify::find_id(&t.artist, &t.title, t.dur)?;
    // не нашли — запоминаем на неделю, а не навсегда: трек могут добавить
    if id.is_some() {
        cache_put(&name, &Value::from(id.clone()));
    }
    Ok(id)
}

pub fn fetch(t: &Track, key: &str) -> Result<Option<Found>> {
    let Some(id) = spotify_id(t)? else { return Ok(None) };
    let name = format!("spicy\n{id}");
    // текст не меняется часто, но community-синк могут залить позже — держим неделю; "нет текста" — сутки
    if let Some(v) = cache_get(&name, 7 * DAY) {
        if !v.is_null() {
            return Ok(parse(&v));
        }
        if cache_get(&name, DAY).is_some() {
            return Ok(None);
        }
    }
    let r = call(agent().get(&format!("https://api.spicylyrics.org/v1/lyrics/{id}")).set("Authorization", &format!("Bearer {key}")));
    let body: Value = match r {
        Ok(r) => r.into_json::<Value>()?["Body"].take(),
        Err(ureq::Error::Status(404, _)) => {
            cache_put(&name, &Value::Null);
            return Ok(None);
        }
        Err(ureq::Error::Status(code @ (401 | 403), r)) => {
            let msg = r.into_json::<Value>().ok().and_then(|v| v["Body"]["message"].as_str().map(String::from)).unwrap_or_default();
            bail!("ключ не принят ({code}): {msg}")
        }
        Err(ureq::Error::Status(429, _)) => bail!("лимит ключа исчерпан"),
        Err(e) => return Err(e.into()),
    };
    let found = parse(&body);
    if found.is_some() {
        cache_put(&name, &body);
    }
    Ok(found)
}

fn parse(b: &Value) -> Option<Found> {
    let source = b["source"].as_str().unwrap_or("unknown");
    let community = source == "spicy_lyrics";
    let provider = match source {
        "spicy_lyrics" => "Spicy Lyrics",
        "apple_music" => "Apple Music",
        "spotify" => "Spotify",
        _ => "Spicy Lyrics",
    };
    let a = &b["UploadAttribution"];
    let people = [("uploader", &a["Uploader"]), ("maker", &a["Maker"])]
        .into_iter()
        .filter_map(|(role, p)| Some((role, p["username"].as_str()?.to_string(), p["url"].as_str().unwrap_or("").to_string())))
        .collect();
    let credit = Credit { provider: provider.into(), via: if community { "" } else { "Spicy Lyrics" }, people, community };
    let lyr = match b["Type"].as_str()? {
        "Syllable" => Lyrics::Synced(syllables(&b["Content"])?),
        "Line" => Lyrics::Synced(
            b["Content"]
                .as_array()?
                .iter()
                .filter_map(|l| {
                    let text = l["Text"].as_str()?.trim().to_string();
                    Some(Line { start: l["StartTime"].as_f64()?, end: l["EndTime"].as_f64(), text, words: vec![], agent: agent_of(l), bg: false })
                })
                .collect(),
        ),
        "Static" => Lyrics::Plain(b["Lines"].as_array()?.iter().filter_map(|l| Some(l["Text"].as_str()?.trim().to_string())).collect()),
        _ => return None,
    };
    match &lyr {
        Lyrics::Synced(l) if l.is_empty() => None,
        Lyrics::Plain(l) if l.is_empty() => None,
        _ => Some(Found { lyr, credit }),
    }
}

fn agent_of(l: &Value) -> Option<String> {
    Some(if l["OppositeAligned"].as_bool().unwrap_or(false) { "v2" } else { "v1" }.to_string())
}

/// Слоги → строка с таймингами по словам. Бэк-вокал — отдельной мелкой строкой сразу под основной,
/// она идёт одновременно с ней (экран умеет несколько активных строк, как у Apple)
fn syllables(content: &Value) -> Option<Vec<Line>> {
    let mut out: Vec<(f64, Line)> = Vec::new(); // (начало основной строки, строка) — бэк держится за своей основной
    for l in content.as_array()? {
        let lead_start = l["Lead"]["StartTime"].as_f64();
        let agent = agent_of(l);
        let groups = std::iter::once((&l["Lead"], false)).chain(l["Background"].as_array().into_iter().flatten().map(|g| (g, true)));
        for (g, bg) in groups {
            let Some(syl) = g["Syllables"].as_array().filter(|s| !s.is_empty()) else { continue };
            let mut text = String::new();
            let mut words = Vec::new();
            for (i, s) in syl.iter().enumerate() {
                let Some(t) = s["Text"].as_str() else { continue };
                text.push_str(t);
                if let (Some(a), Some(b)) = (s["StartTime"].as_f64(), s["EndTime"].as_f64()) {
                    words.push(Word { start: a, end: b, text: t.to_string() });
                }
                if i + 1 < syl.len() && !s["IsPartOfWord"].as_bool().unwrap_or(false) {
                    text.push(' ');
                }
            }
            // скобки у Spicy бывают прямо в слогах — под основной строкой они лишние
            let text = if bg { text.trim().trim_start_matches('(').trim_end_matches(')').trim().to_string() } else { text.trim().to_string() };
            let start = g["StartTime"].as_f64().or(words.first().map(|w| w.start))?;
            let end = g["EndTime"].as_f64().or(words.last().map(|w| w.end));
            out.push((lead_start.unwrap_or(start), Line { start, end, text, words, agent: agent.clone(), bg }));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(out.into_iter().map(|(_, l)| l).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_syllables() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"Type":"Syllable","source":"spicy_lyrics","UploadAttribution":{"Uploader":{"id":"1","username":"spikerko","url":"https://spicylyrics.org/uid/1"}},
            "Content":[{"Type":"Vocal","OppositeAligned":false,"Lead":{"StartTime":1.0,"EndTime":3.0,"Syllables":[
              {"Text":"Hel","StartTime":1.0,"EndTime":1.4,"IsPartOfWord":true},{"Text":"lo","StartTime":1.4,"EndTime":2.0,"IsPartOfWord":false},
              {"Text":"world","StartTime":2.0,"EndTime":3.0,"IsPartOfWord":false}]},
             "Background":[{"StartTime":2.5,"EndTime":3.2,"Syllables":[{"Text":"ooh","StartTime":2.5,"EndTime":3.2}]}]},
             {"Type":"Vocal","OppositeAligned":true,"Lead":{"StartTime":4.0,"EndTime":5.0,"Syllables":[{"Text":"yo","StartTime":4.0,"EndTime":5.0}]}}]}"#,
        )
        .unwrap();
        let f = super::parse(&v).unwrap();
        let super::Lyrics::Synced(l) = &f.lyr else { panic!() };
        let texts: Vec<_> = l.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["Hello world", "ooh", "yo"]);
        assert!(l[1].bg && !l[0].bg);
        assert_eq!(l[0].words.len(), 3);
        assert_eq!((l[0].agent.as_deref(), l[2].agent.as_deref()), (Some("v1"), Some("v2")));
        assert!(f.credit.community && f.credit.people.len() == 1 && f.credit.via.is_empty());
        assert_eq!(f.lyr.quality(), 4);
    }
}
