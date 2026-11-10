use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::model::{Src, Track};
use crate::util::{agent, call};

pub const OEMBED: &str = "spotify-oembed:";

fn parse_url(url: &str) -> Option<(&'static str, String)> {
    for kind in ["playlist", "album", "track"] {
        for pat in [format!("{kind}/"), format!("spotify:{kind}:")] {
            if let Some(i) = url.find(&pat) {
                let id: String = url[i + pat.len()..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                if !id.is_empty() {
                    return Some((kind, id));
                }
            }
        }
    }
    None
}

/// Без ключей: embed-страница + GraphQL веб-плеера для длинных плейлистов
pub fn import(url: &str) -> Result<(String, Vec<Track>)> {
    let (kind, sid) = parse_url(url).context("не понял ссылку Spotify")?;
    import_embed(kind, &sid)
}

fn import_embed(kind: &str, sid: &str) -> Result<(String, Vec<Track>)> {
    const TAG: &str = r#"<script id="__NEXT_DATA__" type="application/json">"#;
    let html = call(agent().get(&format!("https://open.spotify.com/embed/{kind}/{sid}")))?.into_string()?;
    let start = html.find(TAG).context("Spotify поменял embed")? + TAG.len();
    let end = start + html[start..].find("</script>").context("Spotify поменял embed")?;
    let v: Value = serde_json::from_str(&html[start..end])?;
    let e = &v["props"]["pageProps"]["state"]["data"]["entity"];
    let name = e["name"].as_str().or(e["title"].as_str()).unwrap_or("Spotify").to_string();
    // обложка: coverArt.sources (плейлисты) или visualIdentity.image (альбомы)
    let cover = [&e["coverArt"]["sources"], &e["visualIdentity"]["image"]]
        .into_iter()
        .filter_map(Value::as_array)
        .flat_map(|a| a.iter().find(|s| s["width"].as_u64().or(s["maxWidth"].as_u64()).is_some_and(|w| (200..=320).contains(&w))).or(a.first()))
        .find_map(|s| s["url"].as_str())
        .unwrap_or("")
        .to_string();
    // embed отдаёт максимум 100 треков — остальное тянем через GraphQL веб-плеера
    // с анонимным токеном, который лежит прямо в этой же странице
    if kind == "playlist" {
        if let Some(tok) = v["props"]["pageProps"]["state"]["settings"]["session"]["accessToken"].as_str() {
            match pathfinder_playlist(sid, tok) {
                Ok(t) if !t.is_empty() => return Ok((name, t)),
                Ok(_) => crate::logln!("pathfinder: пусто, беру embed"),
                Err(e) => crate::logln!("pathfinder: {e:#}, беру embed"),
            }
        }
    }
    let own_cover = kind != "playlist" && !cover.is_empty();
    let tracks = e["trackList"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    let id = t["uri"].as_str()?.rsplit(':').next()?.to_string();
                    Some(Track { artist_id: String::new(),
                        src: Src::Sp,
                        url: format!("https://open.spotify.com/track/{id}"),
                        // у плейлиста обложки треков подтянутся лениво через oEmbed (см. covers.rs)
                        cover: if own_cover { cover.clone() } else { format!("{OEMBED}{id}") },
                        id,
                        title: t["title"].as_str()?.to_string(),
                        artist: t["subtitle"].as_str().unwrap_or("").replace('\u{a0}', " "),
                        dur: t["duration"].as_u64().unwrap_or(0) / 1000,
                        snip: false,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((name, tracks))
}

// хеш persisted-запроса fetchPlaylist из JS веб-плеера; если Spotify его сменит — перечитаем
const PLAYLIST_HASH: &str = "21ea11a6ac75a5f3e01fd365d29c18e0a4a8b7dc09f89bbb5d73910190c6acaf";
static HASH: Mutex<Option<String>> = Mutex::new(None);

fn playlist_hash(fresh: bool) -> Result<String> {
    let mut g = HASH.lock().unwrap();
    if !fresh {
        return Ok(g.clone().unwrap_or_else(|| PLAYLIST_HASH.into()));
    }
    const PAT: &str = r#""fetchPlaylist","query",""#;
    let html = agent().get("https://open.spotify.com/").call()?.into_string()?;
    for (i, _) in html.match_indices("https://open.spotifycdn.com/cdn/build/web-player/") {
        let Some(e) = html[i..].find(".js") else { continue };
        let Ok(js) = agent().get(&html[i..i + e + 3]).call().map_err(anyhow::Error::from).and_then(|r| Ok(r.into_string()?)) else {
            continue;
        };
        if let Some(h) = js.find(PAT).and_then(|j| js.get(j + PAT.len()..j + PAT.len() + 64)) {
            *g = Some(h.to_string());
            return Ok(h.to_string());
        }
    }
    bail!("не нашёл хеш fetchPlaylist")
}

fn pathfinder_playlist(sid: &str, tok: &str) -> Result<Vec<Track>> {
    let mut hash = playlist_hash(false)?;
    let mut refreshed = false;
    let mut out = Vec::new();
    let mut offset = 0;
    loop {
        let vars = json!({"uri": format!("spotify:playlist:{sid}"), "offset": offset, "limit": 100, "enableWatchFeedEntrypoint": false});
        let ext = json!({"persistedQuery": {"version": 1, "sha256Hash": hash}});
        let r = agent()
            .get("https://api-partner.spotify.com/pathfinder/v1/query")
            .query("operationName", "fetchPlaylist")
            .query("variables", &vars.to_string())
            .query("extensions", &ext.to_string())
            .set("Authorization", &format!("Bearer {tok}"))
            .set("App-Platform", "WebPlayer")
            .call();
        let v: Option<Value> = r.ok().and_then(|r| r.into_json().ok());
        let content = v.as_ref().map(|v| &v["data"]["playlistV2"]["content"]);
        let Some(items) = content.and_then(|c| c["items"].as_array()) else {
            if refreshed {
                bail!("pathfinder не отдал плейлист");
            }
            hash = playlist_hash(true)?;
            refreshed = true;
            continue;
        };
        out.extend(items.iter().filter_map(|it| parse_gql(&it["itemV2"]["data"])));
        offset += 100;
        let total = content.and_then(|c| c["totalCount"].as_u64()).unwrap_or(0) as usize;
        if items.is_empty() || offset >= total {
            break;
        }
    }
    Ok(out)
}

fn parse_gql(d: &Value) -> Option<Track> {
    let id = d["uri"].as_str()?.strip_prefix("spotify:track:")?; // локальные файлы и подкасты мимо
    let artists: Vec<&str> = d["artists"]["items"].as_array()?.iter().filter_map(|a| a["profile"]["name"].as_str()).collect();
    let cover = d["albumOfTrack"]["coverArt"]["sources"]
        .as_array()
        .and_then(|a| a.iter().find(|s| s["width"].as_u64().is_some_and(|w| (200..=320).contains(&w))).or(a.first()))
        .and_then(|s| s["url"].as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("{OEMBED}{id}"));
    Some(Track { artist_id: String::new(),
        src: Src::Sp,
        id: id.into(),
        title: d["name"].as_str()?.to_string(),
        artist: artists.join(", "),
        dur: d["trackDuration"]["totalMilliseconds"].as_u64().unwrap_or(0) / 1000,
        cover,
        url: format!("https://open.spotify.com/track/{id}"),
        snip: false,
    })
}

// ── поиск Spotify ID для трека с другого сервиса (нужен Spicy Lyrics) ──

/// Анонимный токен веб-плеера из embed-страницы. Живёт ~час, держим 30 минут
fn token() -> Result<String> {
    static TOK: Mutex<Option<(std::time::Instant, String)>> = Mutex::new(None);
    let mut g = TOK.lock().unwrap();
    if let Some((_, tok)) = g.as_ref().filter(|(t, _)| t.elapsed().as_secs() < 1800) {
        return Ok(tok.clone());
    }
    let html = call(agent().get("https://open.spotify.com/embed/track/4cOdK2wGLETKBW3PvgPWqT"))?.into_string()?;
    const KEY: &str = r#""accessToken":""#;
    let i = html.find(KEY).context("нет токена в embed")? + KEY.len();
    let tok = html[i..i + html[i..].find('"').context("нет токена в embed")?].to_string();
    *g = Some((std::time::Instant::now(), tok.clone()));
    Ok(tok)
}

// хеш searchTracks лежит в лениво грузящемся чанке страницы поиска
const SEARCH_HASH: &str = "b02683192a98dde7966b5e6655a79eeb62713eab703eda9902c932818dd52751";
static SHASH: Mutex<Option<String>> = Mutex::new(None);

fn search_hash(fresh: bool) -> Result<String> {
    let mut g = SHASH.lock().unwrap();
    if !fresh {
        return Ok(g.clone().unwrap_or_else(|| SEARCH_HASH.into()));
    }
    const PAT: &str = r#""searchTracks","query",""#;
    let html = agent().get("https://open.spotify.com/").call()?.into_string()?;
    let main = html
        .match_indices("https://open.spotifycdn.com/cdn/build/web-player/web-player.")
        .map(|(i, _)| &html[i..i + html[i..].find('"').unwrap_or(0)])
        .find(|u| u.ends_with(".js"))
        .context("нет web-player.js")?;
    let js = agent().get(main).call()?.into_string()?;
    // чанки: {4406:"xpui-routes-search", ...} и где-то рядом {4406:"071e703d", ...}
    let id = js.find(r#":"xpui-routes-search""#).and_then(|i| js[..i].rsplit([',', '{']).next()).context("нет чанка поиска")?;
    for (i, _) in js.match_indices(&format!("{id}:\"")) {
        let h = &js[i + id.len() + 2..];
        let Some(h) = h.get(..8).filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit())) else { continue };
        let url = format!("https://open.spotifycdn.com/cdn/build/web-player/xpui-routes-search.{h}.js");
        let Ok(c) = agent().get(&url).call().map_err(anyhow::Error::from).and_then(|r| Ok(r.into_string()?)) else { continue };
        if let Some(h) = c.find(PAT).and_then(|j| c.get(j + PAT.len()..j + PAT.len() + 64)) {
            *g = Some(h.to_string());
            return Ok(h.to_string());
        }
    }
    bail!("не нашёл хеш searchTracks")
}

fn norm(s: &str) -> String {
    crate::lyrics::clean(s).to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Spotify ID того же трека: название совпадает, исполнитель пересекается, длительность ближе всех
pub fn find_id(artist: &str, title: &str, dur: u64) -> Result<Option<String>> {
    let tok = token()?;
    let (want_t, want_a) = (norm(title), norm(artist));
    let mut hash = search_hash(false)?;
    for attempt in 0..2 {
        let vars = json!({"searchTerm": format!("{} {}", crate::lyrics::clean(artist), crate::lyrics::clean(title)), "offset": 0, "limit": 10,
            "numberOfTopResults": 5, "includeAudiobooks": false, "includePreReleases": false, "includeAuthors": false});
        let ext = json!({"persistedQuery": {"version": 1, "sha256Hash": hash}});
        let r = call(
            agent()
                .get("https://api-partner.spotify.com/pathfinder/v1/query")
                .query("operationName", "searchTracks")
                .query("variables", &vars.to_string())
                .query("extensions", &ext.to_string())
                .set("Authorization", &format!("Bearer {tok}"))
                .set("App-Platform", "WebPlayer"),
        );
        let v: Option<Value> = r.ok().and_then(|r| r.into_json().ok());
        let Some(items) = v.as_ref().and_then(|v| v["data"]["searchV2"]["tracksV2"]["items"].as_array()) else {
            if attempt == 0 {
                hash = search_hash(true)?;
                continue;
            }
            bail!("searchTracks не ответил");
        };
        let best = items
            .iter()
            .filter_map(|it| {
                let d = &it["item"]["data"];
                let name = norm(d["name"].as_str()?);
                let artists: Vec<String> = d["artists"]["items"].as_array()?.iter().filter_map(|a| Some(norm(a["profile"]["name"].as_str()?))).collect();
                let title_ok = !name.is_empty() && (name == want_t || want_t.contains(&name) || name.contains(&want_t));
                // у YouTube исполнитель бывает в названии ("Артист - Трек"), поэтому ищем и там
                let artist_ok = artists.iter().any(|a| !a.is_empty() && (want_a.contains(a.as_str()) || a.contains(&want_a) || want_t.contains(a.as_str())));
                if !(title_ok && artist_ok) {
                    return None;
                }
                let sd = d["duration"]["totalMilliseconds"].as_u64().unwrap_or(0) / 1000;
                let diff = if dur == 0 || sd == 0 { 0 } else { sd.abs_diff(dur) };
                Some((diff, d["id"].as_str()?.to_string()))
            })
            .min_by_key(|(diff, _)| *diff);
        return Ok(best.map(|(_, id)| id));
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    // сетевые: cargo test -- --ignored
    #[test]
    #[ignore]
    fn big_playlist_without_keys() {
        let (name, t) = super::import("https://open.spotify.com/playlist/37i9dQZF1DX5Ejj0EkURtP").unwrap();
        println!("{name}: {} треков, последний: {} — {}", t.len(), t.last().unwrap().artist, t.last().unwrap().title);
        assert!(t.len() > 100);
    }

    #[test]
    #[ignore]
    fn find_ids() {
        assert_eq!(super::find_id("TV Girl", "Cigarettes out the Window", 184).unwrap().as_deref(), Some("6QeYSvYqYUsfBzsApbjDHO"));
        assert!(super::find_id("Eminem", "Rap God (Explicit)", 364).unwrap().is_some());
        assert!(super::find_id("Kino - Topic", "Группа крови", 284).unwrap().is_some());
        assert!(super::search_hash(true).unwrap().len() == 64);
    }

    #[test]
    #[ignore]
    fn hash_refresh() {
        assert_eq!(super::playlist_hash(true).unwrap().len(), 64);
    }
}
