use anyhow::{Context, Result};
use serde_json::Value;

use crate::model::{Src, Track};
use crate::util::{agent, call};

pub fn is_apple(url: &str) -> bool {
    url.contains("music.apple.com")
}

/// Плейлист или альбом Apple Music без ключей: страница сама несёт список треков в
/// <script id="serialized-server-data">. Треки потом ищутся на YT Music (см. ytmusic::find_same) —
/// src/id здесь только заглушки, играть их напрямую нельзя
pub fn import(url: &str) -> Result<(String, Vec<Track>)> {
    const TAG: &str = r#"id="serialized-server-data""#;
    let html = call(agent().get(url))?.into_string()?;
    let start = html.find(TAG).context("Apple Music поменял страницу")?;
    let start = start + html[start..].find('>').context("Apple Music поменял страницу")? + 1;
    let end = start + html[start..].find("</script>").context("Apple Music поменял страницу")?;
    let v: Value = serde_json::from_str(&html[start..end])?;
    let sections = v["data"][0]["data"]["sections"].as_array().context("Apple Music поменял страницу")?;
    let name = sections
        .iter()
        .find(|s| s["itemKind"] == "containerDetailHeaderLockup")
        .and_then(|s| s["items"][0]["title"].as_str())
        .unwrap_or("Apple Music")
        .to_string();
    let tracks = sections
        .iter()
        .filter(|s| s["itemKind"] == "trackLockup")
        .flat_map(|s| s["items"].as_array().into_iter().flatten())
        .filter_map(|t| {
            let title = t["title"].as_str()?.to_string();
            let artist = t["artistName"].as_str().map(String::from).unwrap_or_else(|| {
                t["subtitleLinks"].as_array().map(|a| a.iter().filter_map(|l| l["title"].as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default()
            });
            let id = t["contentDescriptor"]["identifiers"]["storeAdamID"].as_str().unwrap_or("").to_string();
            let cover = t["artwork"]["dictionary"]["url"].as_str().map(|u| u.replace("{w}x{h}bb.{f}", "300x300bb.jpg")).unwrap_or_default();
            Some(Track {
                src: Src::Yt,
                url: t["contentDescriptor"]["url"].as_str().unwrap_or("").to_string(),
                id,
                title,
                artist,
                dur: t["duration"].as_u64().unwrap_or(0) / 1000,
                cover,
                snip: false,
                artist_id: String::new(),
            })
        })
        .collect();
    Ok((name, tracks))
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore] // живой запрос
    fn playlist() {
        let (name, t) = super::import("https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb").unwrap();
        assert!(!name.is_empty() && t.len() > 10, "{name} {}", t.len());
        assert!(t.iter().all(|t| !t.title.is_empty() && !t.artist.is_empty() && t.dur > 0));
    }

    #[test]
    #[ignore] // живые запросы: весь плейлист через YT Music
    fn match_stats() {
        let (_, t) = super::import("https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb").unwrap();
        let (mut sure, mut ask, mut none) = (0, 0, 0);
        for w in &t {
            match crate::ytmusic::find_same(w).unwrap() {
                Some((_, true)) => sure += 1,
                Some((g, false)) => {
                    ask += 1;
                    println!("? {} — {} {}  →  {} — {} {}", w.artist, w.title, w.dur, g.artist, g.title, g.dur);
                }
                None => none += 1,
            }
        }
        println!("уверенно {sure}, спросить {ask}, не нашлось {none} из {}", t.len());
    }
}
