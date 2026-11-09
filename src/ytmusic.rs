use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::model::{Src, Track};
use crate::util::{agent, send_json, ytdlp_run};

// фильтр "Songs" в поиске YT Music
const SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";

pub fn search(q: &str, limit: usize) -> Result<Vec<Track>> {
    let body = json!({
        "context": {"client": {"clientName": "WEB_REMIX", "clientVersion": "1.20251015.03.00", "hl": "en"}},
        "query": q,
        "params": SONGS,
    });
    let req = agent()
        .post("https://music.youtube.com/youtubei/v1/search?prettyPrint=false")
        .set("Origin", "https://music.youtube.com")
        .set("Referer", "https://music.youtube.com/");
    let v: Value = send_json(req, body)?.into_json()?;
    let mut items = Vec::new();
    collect(&v, &mut items);
    Ok(items.into_iter().filter_map(parse).take(limit).collect())
}

/// Лучший трек на YT Music по запросу — так играем Spotify и SC-превью
pub fn best_match(q: &str) -> Result<String> {
    search(q, 1)?.into_iter().next().map(|t| t.url).context("на YT Music ничего не нашлось")
}

/// Этот же трек на YT Music — для импорта из Spotify и Apple Music. Второе значение: уверены ли мы
/// (название и артист сходятся, длительность отличается не больше чем на 10 с). Неуверенные
/// импорт показывает юзеру — оставить или пропустить
pub fn find_same(t: &Track) -> Result<Option<(Track, bool)>> {
    Ok(pick(t, search(&format!("{} {}", t.artist, t.title), 5)?))
}

/// Лучший кандидат и уверены ли в нём. Длительность важнее артиста: артиста сервисы пишут вразнобой
/// (транслит, "feat." в названии), а трек на минуту длиннее — почти всегда другая версия
fn pick(t: &Track, found: Vec<Track>) -> Option<(Track, bool)> {
    let best = found.into_iter().map(|c| (rate(t, &c), c)).min_by_key(|((title_ok, artist_ok, diff), _)| (!title_ok, *diff > 10, !artist_ok, *diff));
    best.map(|((title_ok, artist_ok, diff), c)| (c, title_ok && artist_ok && diff <= 10))
}

/// Насколько найденный трек тот же: (название сходится, артист сходится, разница длительности в секундах)
fn rate(t: &Track, c: &Track) -> (bool, bool, u64) {
    let words = |s: &str| -> Vec<String> { latin(s).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect() };
    // "(feat. X)", " - Remastered 2011" в сравнении названий только мешают
    let core = |s: &str| words(s.split(['(', '[']).next().unwrap_or(s).split(" - ").next().unwrap_or(s)).concat();
    let (title, ct) = (core(&t.title), core(&c.title));
    let title_ok = !title.is_empty() && !ct.is_empty() && (ct.contains(&title) || title.contains(&ct) || close(&title, &ct));
    // YT Music часто прячет второго артиста в название: "Космос (feat. Charusha)"
    let ca: Vec<String> = words(&c.artist).into_iter().chain(words(&c.title)).collect();
    let artist_ok = words(&t.artist).iter().any(|w| ca.iter().any(|x| x == w || close(w, x)));
    let diff = if t.dur == 0 || c.dur == 0 { 0 } else { t.dur.abs_diff(c.dur) };
    (title_ok, artist_ok, diff)
}

/// Почти одинаковые (транслит вразнобой: Skryptonite / Скриптонит): не больше одной правки на 5 букв.
/// Короткие слова сравниваем только точно — иначе "lil" сошёлся бы с чем попало
fn close(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().min(b.len()) < 5 {
        return false;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (prev + (a[i - 1] != b[j - 1]) as usize).min(row[j] + 1).min(row[j - 1] + 1);
            prev = cur;
        }
    }
    row[b.len()] * 5 <= a.len().max(b.len())
}

/// Кириллица и латиница к одному виду: Spotify пишет "Skryptonit — Eto lyubov", YT Music — "Скриптонит —
/// Это любовь". Транслитерируем и сглаживаем то, что транслитерации пишут по-разному (y/i/j, kh/h...)
pub fn latin(s: &str) -> String {
    let mut out = String::new();
    for ch in s.to_lowercase().chars() {
        out += match ch {
            'а' => "a", 'б' => "b", 'в' => "v", 'г' | 'ґ' => "g", 'д' => "d", 'е' | 'ё' | 'є' | 'э' => "e",
            'ж' => "zh", 'з' => "z", 'и' | 'і' | 'ї' | 'й' | 'ы' => "i", 'к' => "k", 'л' => "l", 'м' => "m",
            'н' => "n", 'о' => "o", 'п' => "p", 'р' => "r", 'с' => "s", 'т' => "t", 'у' => "u", 'ф' => "f",
            'х' => "h", 'ц' => "ts", 'ч' => "ch", 'ш' => "sh", 'щ' => "sch", 'ъ' | 'ь' => "", 'ю' => "iu", 'я' => "ia",
            _ => {
                out.push(ch);
                continue;
            }
        };
    }
    out.replace("kh", "h").replace("ck", "k").replace("w", "v").replace("q", "k").replace("x", "ks").replace(['y', 'j'], "i")
}

fn collect<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == "musicResponsiveListItemRenderer" {
                    out.push(x);
                } else {
                    collect(x, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
        _ => {}
    }
}

fn runs(col: &Value) -> Vec<String> {
    col["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"]
        .as_array()
        .map(|a| a.iter().filter_map(|r| r["text"].as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn parse(r: &Value) -> Option<Track> {
    parse_item(r, "", "")
}

/// Строка-трек YT Music (поиск, страница артиста, альбом). Длительность бывает в отдельной колонке,
/// у треков альбома нет своей обложки и иногда артиста — тогда берём альбомные
fn parse_item(r: &Value, fallback_cover: &str, fallback_artist: &str) -> Option<Track> {
    let vid = r["playlistItemData"]["videoId"].as_str()?;
    let cols = r["flexColumns"].as_array()?;
    let title = runs(cols.first()?).concat();
    if title.is_empty() {
        return None;
    }
    // "Daft Punk • Homework • 7:10"
    let meta = cols.get(1).map(runs).unwrap_or_default();
    let groups: Vec<String> = meta.split(|s| s == " • ").map(|g| g.concat()).collect();
    let artist_id = cols
        .get(1)
        .and_then(|c| c["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"].as_array())
        .into_iter()
        .flatten()
        .find(|r| r["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str().is_some_and(|b| b.starts_with("UC")))
        .and_then(|r| r["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str())
        .unwrap_or("")
        .to_string();
    let fixed = r["fixedColumns"][0]["musicResponsiveListItemFixedColumnRenderer"]["text"]["runs"][0]["text"].as_str().map(parse_dur);
    let cover = r["thumbnail"]["musicThumbnailRenderer"]["thumbnail"]["thumbnails"]
        .as_array()
        .and_then(|a| a.last())
        .and_then(|t| t["url"].as_str())
        .map(resize)
        .unwrap_or_else(|| fallback_cover.to_string());
    let artist = groups.first().cloned().filter(|a| !a.is_empty() && parse_dur(a) == 0).unwrap_or_else(|| fallback_artist.to_string());
    Some(Track {
        artist_id,
        src: Src::Yt,
        id: vid.into(),
        title,
        artist,
        dur: fixed.filter(|&d| d > 0).unwrap_or_else(|| groups.last().map(|s| parse_dur(s)).unwrap_or(0)),
        cover,
        url: format!("https://music.youtube.com/watch?v={vid}"),
        snip: false,
    })
}

// ── страницы артистов и альбомов ──

fn post(endpoint: &str, mut body: Value) -> Result<Value> {
    body["context"] = json!({"client": {"clientName": "WEB_REMIX", "clientVersion": "1.20251015.03.00", "hl": "en"}});
    let req = agent()
        .post(&format!("https://music.youtube.com/youtubei/v1/{endpoint}?prettyPrint=false"))
        .set("Origin", "https://music.youtube.com")
        .set("Referer", "https://music.youtube.com/");
    Ok(send_json(req, body)?.into_json()?)
}

fn text(v: &Value) -> String {
    v["runs"].as_array().map(|r| r.iter().filter_map(|x| x["text"].as_str()).collect()).unwrap_or_default()
}

fn last_thumb(v: &Value) -> String {
    let mut th = Vec::new();
    collect_key(v, "thumbnails", &mut th);
    th.first().and_then(|a| a.as_array()).and_then(|a| a.last()).and_then(|t| t["url"].as_str()).unwrap_or("").to_string()
}

/// Карточка альбома / сингла / артиста в карусели
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Card {
    pub id: String, // browseId: MPREb_… альбом, UC… артист
    pub title: String,
    pub sub: String,
    pub cover: String,
}

#[derive(Clone, Debug, Default)]
pub struct Artist {
    pub name: String,
    pub desc: String,
    pub photo: String,
    pub stats: String, // "7.2M subscribers · 23M monthly audience"
    pub top: Vec<Track>,
    pub songs_id: Option<String>, // плейлист со всеми песнями
    pub albums: Vec<Card>,
    pub singles: Vec<Card>,
    pub similar: Vec<Card>,
}

/// Артист по имени — для треков, у которых нет id артиста (SoundCloud, Spotify, Яндекс)
pub fn find_artist(name: &str) -> Result<String> {
    let v = post("search", json!({"query": name, "params": ARTISTS}))?;
    let mut items = Vec::new();
    collect_key(&v, "musicResponsiveListItemRenderer", &mut items);
    items
        .iter()
        .find_map(|r| r["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str().filter(|b| b.starts_with("UC")))
        .map(String::from)
        .context("артист не найден на YouTube Music")
}

/// Фото артиста. У трека SoundCloud — аватар автора (поиск по имени нашёл бы однофамильца); у остальных —
/// с YT Music: по id из трека, если это его первый артист, иначе поиском по имени
pub fn artist_photo(name: &str, t: Option<&Track>) -> Result<String> {
    match t {
        Some(t) if t.src == Src::Sc => return crate::soundcloud::avatar(&t.url),
        Some(t) if t.src != Src::Lc && !t.artist_id.is_empty() && crate::stats::artists(&t.artist).first() == Some(&name) => {
            return Ok(artist(&t.artist_id)?.photo)
        }
        _ => {}
    }
    Ok(artist(&find_artist(name)?)?.photo)
}

pub fn artist(id: &str) -> Result<Artist> {
    let v = post("browse", json!({"browseId": id}))?;
    let h = v["header"].as_object().and_then(|o| o.values().next()).cloned().unwrap_or_default();
    let subs = text(&h["subscriptionButton"]["subscribeButtonRenderer"]["subscriberCountText"]);
    let monthly = text(&h["monthlyListenerCount"]);
    let mut a = Artist {
        name: text(&h["title"]),
        desc: text(&h["description"]),
        photo: last_thumb(&h["thumbnail"]),
        stats: [if subs.is_empty() { String::new() } else { format!("{subs} subscribers") }, monthly].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "),
        ..Default::default()
    };
    let sections = v["contents"]["singleColumnBrowseResultsRenderer"]["tabs"][0]["tabRenderer"]["content"]["sectionListRenderer"]["contents"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for s in &sections {
        if let Some(shelf) = s.get("musicShelfRenderer") {
            a.top = shelf["contents"].as_array().into_iter().flatten().filter_map(|it| parse_item(&it["musicResponsiveListItemRenderer"], "", &a.name)).collect();
            a.songs_id = shelf["bottomEndpoint"]["browseEndpoint"]["browseId"]
                .as_str()
                .or(shelf["title"]["runs"][0]["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str())
                .map(String::from);
        }
        if let Some(car) = s.get("musicCarouselShelfRenderer") {
            let title = text(&car["header"]["musicCarouselShelfBasicHeaderRenderer"]["title"]).to_lowercase();
            let cards: Vec<Card> = car["contents"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| {
                    let it = &c["musicTwoRowItemRenderer"];
                    Some(Card {
                        id: it["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str()?.to_string(),
                        title: text(&it["title"]),
                        sub: text(&it["subtitle"]),
                        cover: resize(&last_thumb(&it["thumbnailRenderer"])),
                    })
                })
                .collect();
            let albums: Vec<Card> = cards.iter().filter(|c| c.id.starts_with("MPREb")).cloned().collect();
            if title.contains("single") {
                a.singles = albums;
            } else if title.contains("album") {
                a.albums = albums;
            } else if title.contains("like") || title.contains("similar") {
                a.similar = cards.into_iter().filter(|c| c.id.starts_with("UC")).collect();
            }
        }
    }
    if a.name.is_empty() {
        bail!("YouTube Music не отдал страницу артиста");
    }
    Ok(a)
}

/// Все песни артиста (плейлист из "Top songs → Show all"), с подгрузкой продолжений
pub fn all_songs(browse_id: &str, artist: &str) -> Result<Vec<Track>> {
    let mut v = post("browse", json!({"browseId": browse_id}))?;
    let mut out = Vec::new();
    for _ in 0..12 {
        let mut items = Vec::new();
        collect_key(&v, "musicResponsiveListItemRenderer", &mut items);
        out.extend(items.into_iter().filter_map(|r| parse_item(r, "", artist)));
        let mut cmds = Vec::new();
        collect_key(&v, "continuationCommand", &mut cmds);
        let Some(tok) = cmds.first().and_then(|c| c["token"].as_str()).map(String::from) else { break };
        v = post("browse", json!({"continuation": tok}))?;
    }
    Ok(out)
}

/// Альбом/сингл: (название, "Album • 2023", обложка, треки)
pub fn album(browse_id: &str) -> Result<(String, String, String, Vec<Track>)> {
    let v = post("browse", json!({"browseId": browse_id}))?;
    let mut hdr = Vec::new();
    collect_key(&v, "musicResponsiveHeaderRenderer", &mut hdr);
    let h = hdr.first().copied().cloned().unwrap_or_default();
    let cover = resize(&last_thumb(&h["thumbnail"]));
    let artist = text(&h["straplineTextOne"]);
    let mut items = Vec::new();
    collect_key(&v, "musicResponsiveListItemRenderer", &mut items);
    let tracks = items.into_iter().filter_map(|r| parse_item(r, &cover, &artist)).collect();
    let sub = [text(&h["subtitle"]), artist].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
    Ok((text(&h["title"]), sub, cover, tracks))
}

/// Фильтры поиска YT Music
pub const ARTISTS: &str = "EgWKAQIgAWoKEAkQBRAKEAMQBA%3D%3D";
pub const ALBUMS: &str = "EgWKAQIYAWoKEAkQBRAKEAMQBA%3D%3D";

/// Артисты или альбомы по запросу — карточки для каруселей в поиске
pub fn search_cards(q: &str, filter: &str, limit: usize) -> Result<Vec<Card>> {
    let v = post("search", json!({"query": q, "params": filter}))?;
    let mut items = Vec::new();
    collect_key(&v, "musicResponsiveListItemRenderer", &mut items);
    Ok(items
        .iter()
        .filter_map(|r| {
            let id = r["navigationEndpoint"]["browseEndpoint"]["browseId"].as_str()?;
            if !(id.starts_with("UC") || id.starts_with("MPREb")) {
                return None;
            }
            let cols = r["flexColumns"].as_array()?;
            let title = runs(cols.first()?).concat();
            // "Artist • 1.2M subscribers" / "Album • Artist • 2019" — тип в начале не нужен
            let sub: Vec<String> = cols.get(1).map(runs).unwrap_or_default().concat().split(" • ").map(String::from).collect();
            let sub = if sub.len() > 1 { sub[1..].join(" • ") } else { sub.concat() };
            Some(Card { id: id.into(), title, sub, cover: resize(&last_thumb(&r["thumbnail"])) })
        })
        .take(limit)
        .collect())
}

fn resize(u: &str) -> String {
    match u.rfind("=w") {
        Some(i) if u.contains("googleusercontent") || u.contains("ggpht") => format!("{}=w226-h226-l90-rj", &u[..i]),
        _ => u.to_string(),
    }
}

fn parse_dur(s: &str) -> u64 {
    if !s.contains(':') {
        return 0;
    }
    s.trim().split(':').try_fold(0u64, |a, p| p.parse::<u64>().ok().map(|x| a * 60 + x)).unwrap_or(0)
}

/// Импорт плейлиста/альбома YouTube / YT Music через yt-dlp
pub fn import(url: &str) -> Result<(String, Vec<Track>)> {
    let out = ytdlp_run(|c| { c.args(["--flat-playlist", "-J", "--no-warnings", url]); })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().last().unwrap_or("yt-dlp упал"));
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    let name = v["title"].as_str().unwrap_or("YouTube").trim_start_matches("Album - ").to_string();
    let tracks = v["entries"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let id = e["id"].as_str()?;
                    let artist = e["channel"].as_str().or(e["uploader"].as_str()).unwrap_or("");
                    Some(Track { artist_id: String::new(),
                        src: Src::Yt,
                        id: id.into(),
                        title: e["title"].as_str()?.to_string(),
                        artist: artist.trim_end_matches(" - Topic").to_string(),
                        dur: e["duration"].as_f64().unwrap_or(0.0) as u64,
                        cover: format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
                        url: format!("https://music.youtube.com/watch?v={id}"),
                        snip: false,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((name, tracks))
}

/// Прямая ссылка на аудиопоток (YouTube/SoundCloud) через yt-dlp — её и отдаём mpv
pub fn stream_url(url: &str) -> Result<String> {
    crate::util::stream_urls(&[url.to_string()], false)?.remove(url).context("yt-dlp не дал ссылку")
}

/// Прямые ссылки для пачки треков (предзагрузка: фоновый живой yt-dlp с nice 19).
/// Возвращает входной url → ссылка; что не отрезолвилось — просто нет в карте
pub fn stream_urls(urls: &[String]) -> Result<std::collections::HashMap<String, String>> {
    crate::util::stream_urls(urls, true)
}

/// Радио по треку — тот же микс, что "Радио" в YouTube Music: ~50 похожих треков, первым идёт сам трек
pub fn radio(video_id: &str) -> Result<Vec<Track>> {
    let body = json!({
        "context": {"client": {"clientName": "WEB_REMIX", "clientVersion": "1.20251015.03.00", "hl": "en"}},
        "videoId": video_id,
        "playlistId": format!("RDAMVM{video_id}"),
        "isAudioOnly": true,
    });
    let req = agent()
        .post("https://music.youtube.com/youtubei/v1/next?prettyPrint=false")
        .set("Origin", "https://music.youtube.com")
        .set("Referer", "https://music.youtube.com/");
    let v: Value = send_json(req, body)?.into_json()?;
    let mut items = Vec::new();
    collect_key(&v, "playlistPanelVideoRenderer", &mut items);
    let text = |v: &Value| -> String { v["runs"].as_array().map(|r| r.iter().filter_map(|x| x["text"].as_str()).collect()).unwrap_or_default() };
    let mut seen = std::collections::HashSet::new();
    let tracks: Vec<Track> = items
        .into_iter()
        .filter_map(|r| {
            let vid = r["videoId"].as_str()?;
            if !seen.insert(vid.to_string()) {
                return None;
            }
            // "Daft Punk • Homework • 1996"
            let byline = text(&r["longBylineText"]);
            let artist = byline.split(" • ").next().unwrap_or("").to_string();
            let cover = r["thumbnail"]["thumbnails"].as_array().and_then(|a| a.last()).and_then(|t| t["url"].as_str()).map(resize).unwrap_or_default();
            Some(Track { artist_id: String::new(),
                src: Src::Yt,
                id: vid.into(),
                title: text(&r["title"]),
                artist,
                dur: parse_dur(&text(&r["lengthText"])),
                cover,
                url: format!("https://music.youtube.com/watch?v={vid}"),
                snip: false,
            })
        })
        .collect();
    if tracks.is_empty() {
        bail!("YouTube Music не дал радио для этого трека");
    }
    Ok(tracks)
}

fn collect_key<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == key {
                    out.push(x);
                } else {
                    collect_key(x, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_key(x, key, out)),
        _ => {}
    }
}

/// videoId из ссылки YouTube / YT Music
pub fn video_id(url: &str) -> Option<String> {
    let i = url.find("v=")? + 2;
    Some(url[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect()).filter(|s: &String| s.len() == 11)
}

#[cfg(test)]
mod tests {
    #[test]
    fn transliterated_is_same() {
        let tr = |artist: &str, title: &str, dur| crate::model::Track { artist: artist.into(), title: title.into(), dur, ..Default::default() };
        assert_eq!(super::rate(&tr("Skryptonit", "Eto lyubov", 200), &tr("Скриптонит", "Это любовь", 201)), (true, true, 1));
        assert_eq!(super::rate(&tr("Скриптонит", "Это любовь", 200), &tr("Skryptonit", "Eto lyubov", 200)), (true, true, 0));
        assert_eq!(super::rate(&tr("Kino", "Gruppa krovi", 200), &tr("КИНО", "Группа крови", 200)), (true, true, 0));
        assert_eq!(super::rate(&tr("Skryptonite", "Это любовь", 279), &tr("Скриптонит", "Это любовь", 280)), (true, true, 1));
        assert_eq!(super::rate(&tr("Skryptonite, Charusha", "Космос", 227), &tr("Скриптонит", "Космос (feat. Charusha)", 228)), (true, true, 1));
        assert!(!super::rate(&tr("Skryptonit", "Eto lyubov", 200), &tr("Скриптонит", "Положение", 200)).0);
        // живая выдача YT Music: версия на минуту длиннее с точным артистом не должна побеждать
        let found = vec![tr("Gruppa Skryptonite", "Это любовь (360)", 334), tr("Скриптонит", "Это любовь", 280)];
        let (got, sure) = super::pick(&tr("Skryptonite", "Это любовь", 279), found).unwrap();
        assert!(got.dur == 280 && sure);
        assert!(!super::rate(&tr("Lil Peep", "X", 200), &tr("Lil Tecca", "Y", 200)).0);
    }

    // сетевой и долгий: cargo test -- --ignored idle
    // NAT университета рвёт соединения после ~2 мин простоя — второй поиск не должен висеть
    #[test]
    #[ignore]
    fn same_track() {
        let want = crate::model::Track { title: "Cigarettes out the Window".into(), artist: "TV Girl".into(), dur: 199, ..crate::model::Track::default() };
        let (t, sure) = super::find_same(&want).unwrap().unwrap();
        println!("{} — {} {}", t.artist, t.title, t.dur);
        assert!(sure);
        let odd = crate::model::Track { dur: 600, ..want };
        assert!(!super::find_same(&odd).unwrap().unwrap().1, "10 минут вместо 4 — надо спросить");
    }

    #[test]
    #[ignore]
    fn cards() {
        for f in [super::ARTISTS, super::ALBUMS] {
            let c = super::search_cards("tv girl", f, 6).unwrap();
            for c in &c {
                println!("{} | {} | {} | {}", c.id, c.title, c.sub, c.cover);
            }
            assert!(!c.is_empty());
        }
    }

    #[test]
    #[ignore]
    fn search_after_idle() {
        assert!(!super::search("daft punk", 3).unwrap().is_empty());
        std::thread::sleep(std::time::Duration::from_secs(130));
        let t = std::time::Instant::now();
        assert!(!super::search("daft punk", 3).unwrap().is_empty());
        println!("после простоя: {:?}", t.elapsed());
        assert!(t.elapsed().as_secs() < 3);
    }

    #[test]
    #[ignore]
    fn radio_net() {
        let r = super::radio("Jb6gcoR266U").unwrap();
        println!("{} треков, второй: {} — {}", r.len(), r[1].artist, r[1].title);
        assert!(r.len() > 20 && r[0].id == "Jb6gcoR266U");
    }

    #[test]
    #[ignore]
    fn artist_page_net() {
        let id = super::find_artist("Daft Punk").unwrap();
        let a = super::artist(&id).unwrap();
        println!("{} | {} | top {} | albums {} | singles {} | similar {} | {}", a.name, a.stats, a.top.len(), a.albums.len(), a.singles.len(), a.similar.len(), &a.desc[..60]);
        assert!(!a.top.is_empty() && !a.albums.is_empty() && !a.similar.is_empty() && !a.photo.is_empty());
        let all = super::all_songs(a.songs_id.as_deref().unwrap(), &a.name).unwrap();
        println!("все песни: {}", all.len());
        assert!(all.len() > 100);
        let (title, sub, cover, tracks) = super::album(&a.albums[0].id).unwrap();
        println!("альбом: {title} | {sub} | {} треков", tracks.len());
        assert!(!tracks.is_empty() && !cover.is_empty() && tracks.iter().all(|t| !t.cover.is_empty()));
    }

    #[test]
    fn video_id() {
        assert_eq!(super::video_id("https://music.youtube.com/watch?v=Jb6gcoR266U&list=x").as_deref(), Some("Jb6gcoR266U"));
    }
}

