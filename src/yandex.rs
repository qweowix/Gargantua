//! Яндекс Музыка: поиск и импорт без аккаунта через api.music.yandex.net.
//! API отвечает только там, где сервис доступен (РФ/СНГ); из ЕС приходит 451 — об этом и сообщаем.
//! Аудио Яндекс без авторизации не отдаёт, поэтому играем тот же трек с YT Music (как Spotify).

use std::collections::HashMap;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::model::{Src, Track};
use crate::util::{agent, call};

const API: &str = "https://api.music.yandex.net";

fn get(path: &str, params: &[(&str, &str)]) -> Result<Value> {
    let mut req = agent().get(&format!("{API}{path}"));
    for (k, v) in params {
        req = req.query(k, v);
    }
    match call(req) {
        Ok(r) => {
            let v: Value = r.into_json()?;
            Ok(v["result"].clone())
        }
        Err(ureq::Error::Status(451, _)) => bail!("Яндекс Музыка недоступна в вашем регионе"),
        Err(e) => Err(e.into()),
    }
}

fn id_str(v: &Value) -> Option<String> {
    v.as_str().map(String::from).or_else(|| v.as_u64().map(|n| n.to_string()))
}

fn parse(t: &Value) -> Option<Track> {
    let id = id_str(&t["id"])?;
    let mut title = t["title"].as_str()?.to_string();
    if let Some(v) = t["version"].as_str().filter(|v| !v.is_empty()) {
        title = format!("{title} ({v})");
    }
    let artists: Vec<&str> = t["artists"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()).collect();
    let cover = t["coverUri"]
        .as_str()
        .or(t["albums"][0]["coverUri"].as_str())
        .map(|u| format!("https://{}", u.replace("%%", "200x200")))
        .unwrap_or_default();
    Some(Track { artist_id: String::new(),
        src: Src::Ya,
        url: format!("https://music.yandex.ru/track/{id}"),
        id,
        title,
        artist: artists.join(", "),
        dur: t["durationMs"].as_u64().unwrap_or(0) / 1000,
        cover,
        snip: false,
    })
}

pub fn search(q: &str, limit: usize) -> Result<Vec<Track>> {
    let r = get("/search", &[("text", q), ("type", "track"), ("page", "0"), ("nocorrect", "false")])?;
    Ok(r["tracks"]["results"].as_array().into_iter().flatten().filter_map(parse).take(limit).collect())
}

/// Треки плейлиста: у длинных плейлистов часть приходит голыми id — дотягиваем пачками
fn playlist_tracks(r: &Value) -> Result<Vec<Track>> {
    let items = r["tracks"].as_array().cloned().unwrap_or_default();
    let missing: Vec<String> = items.iter().filter(|it| it["track"].is_null()).filter_map(|it| id_str(&it["id"])).collect();
    let mut full: HashMap<String, Track> = HashMap::new();
    for chunk in missing.chunks(100) {
        let r = get("/tracks", &[("track-ids", &chunk.join(","))])?;
        for t in r.as_array().into_iter().flatten().filter_map(parse) {
            full.insert(t.id.clone(), t);
        }
    }
    Ok(items.iter().filter_map(|it| parse(&it["track"]).or_else(|| full.remove(&id_str(&it["id"])?))).collect())
}

/// Сегмент пути после `key/`: music.yandex.ru/users/<owner>/playlists/<kind>
fn after<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let i = url.find(key)? + key.len();
    let rest = &url[i..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&rest[..end]).filter(|s| !s.is_empty())
}

pub fn import(url: &str) -> Result<(String, Vec<Track>)> {
    if let (Some(owner), Some(kind)) = (after(url, "/users/"), after(url, "/playlists/")) {
        let r = get(&format!("/users/{owner}/playlists/{kind}"), &[])?;
        return Ok((r["title"].as_str().unwrap_or("Яндекс Музыка").into(), playlist_tracks(&r)?));
    }
    // новые ссылки: music.yandex.ru/playlists/<uuid>
    if let Some(uuid) = after(url, "/playlists/") {
        let r = get(&format!("/playlist/{uuid}"), &[("rich-tracks", "true")])?;
        return Ok((r["title"].as_str().unwrap_or("Яндекс Музыка").into(), playlist_tracks(&r)?));
    }
    if let Some(track) = after(url, "/track/") {
        let r = get(&format!("/tracks/{track}"), &[])?;
        let t: Vec<Track> = r.as_array().into_iter().flatten().filter_map(parse).collect();
        return Ok((t.first().map(|t| t.title.clone()).unwrap_or_default(), t));
    }
    if let Some(album) = after(url, "/album/") {
        let r = get(&format!("/albums/{album}/with-tracks"), &[])?;
        let tracks = r["volumes"].as_array().into_iter().flatten().flat_map(|v| v.as_array().into_iter().flatten()).filter_map(parse).collect();
        return Ok((r["title"].as_str().unwrap_or("Яндекс Музыка").into(), tracks));
    }
    bail!("не понял ссылку Яндекс Музыки")
}

#[cfg(test)]
mod tests {
    #[test]
    fn url_parts() {
        let u = "https://music.yandex.ru/users/yamusic-top/playlists/1076?utm=x";
        assert_eq!(super::after(u, "/users/"), Some("yamusic-top"));
        assert_eq!(super::after(u, "/playlists/"), Some("1076"));
        assert_eq!(super::after("https://music.yandex.ru/album/123/track/456", "/track/"), Some("456"));
    }

    #[test]
    fn parse_track() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"id":"123","title":"Song","version":"Remix","artists":[{"name":"A"},{"name":"B"}],"durationMs":201000,"coverUri":"avatars.yandex.net/get-yandex-music/1/%%"}"#,
        )
        .unwrap();
        let t = super::parse(&v).unwrap();
        assert_eq!((t.title.as_str(), t.artist.as_str(), t.dur), ("Song (Remix)", "A, B", 201));
        assert_eq!(t.cover, "https://avatars.yandex.net/get-yandex-music/1/200x200");
    }
}
