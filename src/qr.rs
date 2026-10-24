//! Обмен плейлистами через QR. Формат рассчитан и на будущую мобильную версию:
//! текст плейлиста → deflate → base45 (это ровно алфавит буквенно-цифрового режима QR — самый плотный
//! для такого текста) → куски по ~1100 символов: QR средней плотности, камера телефона читает уверенно.
//! Каждый кусок: "BH1:<набор>:<часть>/<всего>:<данные>". Большой плейлист = несколько QR.
//!
//! Внутри (до сжатия), построчно, поля через табуляцию. Два вида:
//!   BHPL1 — весь список:              BHPL2 — плейлист, импортированный по ссылке:
//!   <название>                         <название>
//!   <трек>…                            @<ссылка-источник>
//!                                      -<сервис:id> (удалённые после импорта)…
//!                                      <трек>… (добавленные после импорта)
//!   <трек> = <yt|sc|sp|ya> id название артист длительность [url] [обложка] [snip]
//! url и обложку пишем только когда их нельзя восстановить по id (SoundCloud, обложки Яндекса).
//! BHPL2 — 1 QR вместо десятков: получатель сам подтягивает плейлист по ссылке.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use crate::model::{Playlist, Src, Track};

/// Что лежит в QR
#[derive(Debug)]
pub enum Shared {
    Full { name: String, tracks: Vec<Track> },
    Linked { name: String, url: String, removed: Vec<String>, added: Vec<Track> },
}
use crate::spotify;

const B45: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
const CHUNK: usize = 1600; // ~QR v27 с коррекцией M: телефон уверенно читает и с экрана

fn b45_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 3 / 2 + 2);
    for c in data.chunks(2) {
        let mut n = if c.len() == 2 { c[0] as u32 * 256 + c[1] as u32 } else { c[0] as u32 };
        let digits = if c.len() == 2 { 3 } else { 2 };
        for _ in 0..digits {
            s.push(B45[(n % 45) as usize] as char);
            n /= 45;
        }
    }
    s
}

fn b45_decode(s: &str) -> Option<Vec<u8>> {
    let v: Option<Vec<u32>> = s.bytes().map(|b| B45.iter().position(|&x| x == b).map(|p| p as u32)).collect();
    let v = v?;
    let mut out = Vec::with_capacity(v.len() * 2 / 3 + 1);
    for c in v.chunks(3) {
        match c {
            [a, b, c] => {
                let n = a + b * 45 + c * 2025;
                if n > 0xffff {
                    return None;
                }
                out.extend([(n >> 8) as u8, (n & 0xff) as u8]);
            }
            [a, b] => {
                let n = a + b * 45;
                if n > 0xff {
                    return None;
                }
                out.push(n as u8);
            }
            _ => return None,
        }
    }
    Some(out)
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn track_line(t: &Track) -> String {
    // локальный файл другу не передать: едет имя файла, у него трек найдётся на YT Music
    let file;
    let t = if t.src == Src::Lc {
        file = Track { id: std::path::Path::new(&t.id).file_name().map_or(String::new(), |n| n.to_string_lossy().into()), ..t.clone() };
        &file
    } else {
        t
    };
    let (url, cover) = match t.src {
        Src::Sc => (t.url.as_str(), t.cover.as_str()),
        Src::Ya => ("", t.cover.as_str()),
        _ => ("", ""), // восстановятся по id
    };
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        t.src.tag(),
        t.id,
        clean(&t.title),
        clean(&t.artist),
        t.dur,
        url,
        cover,
        if t.snip { "1" } else { "" }
    )
}

fn serialize(pl: &Playlist) -> String {
    match &pl.source {
        Some(src) => {
            let have: std::collections::HashSet<String> = pl.tracks.iter().map(Track::key).collect();
            let from_src: std::collections::HashSet<&String> = src.keys.iter().collect();
            let mut s = format!("BHPL2\n{}\n@{}\n", clean(&pl.name), clean(&src.url));
            for k in src.keys.iter().filter(|k| !have.contains(*k)) {
                s += &format!("-{k}\n");
            }
            for t in pl.tracks.iter().filter(|t| !from_src.contains(&t.key())) {
                s += &track_line(t);
            }
            s
        }
        None => {
            let mut s = format!("BHPL1\n{}\n", clean(&pl.name));
            for t in &pl.tracks {
                s += &track_line(t);
            }
            s
        }
    }
}

fn parse_track(l: &str) -> Option<Track> {
    let f: Vec<&str> = l.split('\t').collect();
    let (tag, id) = (*f.first()?, f.get(1)?.to_string());
    let get = |i: usize| f.get(i).copied().unwrap_or("").to_string();
    let src = match tag {
        "yt" => Src::Yt,
        "sc" => Src::Sc,
        "sp" => Src::Sp,
        "ya" => Src::Ya,
        "lc" => Src::Lc,
        _ => return None, // трек из сервиса, которого эта версия не знает
    };
    let (url, cover) = match src {
        Src::Yt => (format!("https://music.youtube.com/watch?v={id}"), format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")),
        Src::Sp => (format!("https://open.spotify.com/track/{id}"), format!("{}{id}", spotify::OEMBED)),
        Src::Ya => (format!("https://music.yandex.ru/track/{id}"), get(6)),
        Src::Sc => (get(5), get(6)),
        Src::Lc => (String::new(), String::new()),
    };
    Some(Track { artist_id: String::new(), src, title: get(2), artist: get(3), dur: get(4).parse().unwrap_or(0), url, cover, snip: get(7) == "1", id })
}

fn deserialize(text: &str) -> Result<Shared> {
    let mut lines = text.lines();
    let kind = lines.next();
    let name = lines.next().unwrap_or("Плейлист").to_string();
    match kind {
        Some("BHPL1") => Ok(Shared::Full { name, tracks: lines.filter_map(parse_track).collect() }),
        Some("BHPL2") => {
            let url = lines.next().and_then(|l| l.strip_prefix('@')).context("QR повреждён")?.to_string();
            let (mut removed, mut added) = (vec![], vec![]);
            for l in lines {
                match l.strip_prefix('-') {
                    Some(k) => removed.push(k.to_string()),
                    None => added.extend(parse_track(l)),
                }
            }
            Ok(Shared::Linked { name, url, removed, added })
        }
        _ => bail!("это не плейлист BlackHole"),
    }
}

/// Плейлист → тексты QR-кодов (один или несколько)
pub fn encode(pl: &Playlist) -> Vec<String> {
    let packed = miniz_oxide::deflate::compress_to_vec(serialize(pl).as_bytes(), 9);
    let data = b45_encode(&packed);
    // id набора — чтобы не склеить куски от разных плейлистов
    let set = b45_encode(&(packed.len() as u32 ^ packed.iter().fold(0u32, |h, &b| h.rotate_left(5) ^ b as u32)).to_le_bytes())
        .replace([' ', ':'], "0")
        .chars()
        .take(4)
        .collect::<String>();
    let chunks: Vec<&str> = data.as_bytes().chunks(CHUNK).map(|c| std::str::from_utf8(c).unwrap()).collect();
    let n = chunks.len();
    chunks.iter().enumerate().map(|(i, c)| format!("BH1:{set}:{}/{n}:{c}", i + 1)).collect()
}

/// QR-картинка: чёрные модули на белом, 4 модуля поля по краям
pub fn render(text: &str, module: u32) -> Result<image::RgbaImage> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)?;
    let w = code.width() as u32;
    let colors = code.to_colors();
    let size = (w + 8) * module;
    let mut img = image::RgbaImage::from_pixel(size, size, image::Rgba([255, 255, 255, 255]));
    for (i, c) in colors.iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let (x, y) = ((i as u32 % w + 4) * module, (i as u32 / w + 4) * module);
            for dy in 0..module {
                for dx in 0..module {
                    img.put_pixel(x + dx, y + dy, image::Rgba([0, 0, 0, 255]));
                }
            }
        }
    }
    Ok(img)
}

/// Картинки с QR → плейлист. Можно выбрать сразу все части; лишние QR (не наши) игнорируются.
pub fn decode_files(paths: &[PathBuf]) -> Result<Shared> {
    let mut found: Vec<String> = Vec::new();
    for p in paths {
        let img = image::open(p).with_context(|| format!("не открыл {}", p.display()))?;
        // фото с телефона бывают по 12 Мп — детектору хватит и меньше, а так в разы быстрее
        let img = if img.width().max(img.height()) > 1800 { img.resize(1800, 1800, image::imageops::FilterType::Triangle) } else { img };
        let mut prep = rqrr::PreparedImage::prepare(img.to_luma8());
        for g in prep.detect_grids() {
            if let Ok((_, content)) = g.decode() {
                found.push(content);
            }
        }
    }
    if found.is_empty() {
        bail!("на картинке нет QR-кода");
    }
    decode_texts(&found)
}

fn decode_texts(found: &[String]) -> Result<Shared> {
    // набор → (всего частей, номер → данные)
    let mut sets: BTreeMap<String, (usize, BTreeMap<usize, String>)> = BTreeMap::new();
    for t in found {
        let mut it = t.splitn(4, ':');
        let (Some("BH1"), Some(set), Some(part), Some(data)) = (it.next(), it.next(), it.next(), it.next()) else { continue };
        let Some((i, n)) = part.split_once('/').and_then(|(i, n)| Some((i.parse::<usize>().ok()?, n.parse::<usize>().ok()?))) else { continue };
        let e = sets.entry(set.to_string()).or_insert((n, BTreeMap::new()));
        e.1.insert(i, data.to_string());
    }
    let Some((n, parts)) = sets.into_values().max_by_key(|(_, p)| p.len()) else {
        bail!("это не QR-код плейлиста BlackHole");
    };
    if parts.len() < n {
        let have: Vec<String> = parts.keys().map(|k| k.to_string()).collect();
        bail!("не хватает частей QR: есть {} из {n}. Выбери все картинки сразу", have.join(", "));
    }
    let data: String = parts.into_values().collect();
    let packed = b45_decode(&data).context("QR повреждён")?;
    let text = miniz_oxide::inflate::decompress_to_vec(&packed).map_err(|_| anyhow::anyhow!("QR повреждён"))?;
    deserialize(&String::from_utf8_lossy(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(src: Src, id: &str) -> Track {
        Track { artist_id: String::new(), src, id: id.into(), title: format!("Title {id}"), artist: "Artist\tX".into(), dur: 200, cover: "https://c/1.jpg".into(), url: "https://soundcloud.com/a/b".into(), snip: src == Src::Sc }
    }

    #[test]
    fn base45_roundtrip() {
        for data in [&b""[..], b"a", b"ab", b"abc", &[0, 255, 254, 1, 128]] {
            assert_eq!(b45_decode(&b45_encode(data)).unwrap(), data);
        }
        assert_eq!(b45_encode(b"AB"), "BB8"); // пример из RFC 9285
    }

    #[test]
    fn playlist_roundtrip_multi_part() {
        // 300 треков — заведомо больше одного QR
        let tracks: Vec<Track> = (0..300)
            .map(|i| match i % 4 {
                0 => track(Src::Yt, &format!("vid{i:08}")),
                1 => track(Src::Sc, &format!("{}", 100000 + i)),
                2 => track(Src::Sp, &format!("sp{i:020}")),
                _ => track(Src::Ya, &format!("{}", 5000 + i)),
            })
            .collect();
        let pl = Playlist { name: "Мой плейлист".into(), tracks, source: None, ..Default::default() };
        let codes = encode(&pl);
        assert!(codes.len() > 1, "ожидал несколько частей, вышло {}", codes.len());
        // части в любом порядке
        let mut shuffled = codes.clone();
        shuffled.reverse();
        let Shared::Full { name, tracks: t } = decode_texts(&shuffled).unwrap() else { panic!() };
        assert_eq!(name, "Мой плейлист");
        assert_eq!(t.len(), 300);
        assert_eq!(t[1].url, "https://soundcloud.com/a/b");
        assert!(t[1].snip && !t[0].snip);
        assert_eq!(t[0].url, "https://music.youtube.com/watch?v=vid00000000");
        assert_eq!(t[0].artist, "Artist X");
        // без одной части — понятная ошибка
        let err = decode_texts(&codes[1..]).unwrap_err().to_string();
        assert!(err.contains("не хватает"), "{err}");
    }

    #[test]
    fn linked_is_one_qr() {
        // импортированный плейлист на 400 треков, потом 2 удалили и 1 добавили
        let tracks: Vec<Track> = (0..400).map(|i| track(Src::Sp, &format!("sp{i:020}"))).collect();
        let keys: Vec<String> = tracks.iter().map(Track::key).collect();
        let mut now = tracks[2..].to_vec();
        now.push(track(Src::Yt, "newvideo001"));
        let pl = Playlist { name: "GOATED".into(), tracks: now, source: Some(crate::model::Source { url: "https://open.spotify.com/playlist/abc".into(), keys }), ..Default::default() };
        let codes = encode(&pl);
        assert_eq!(codes.len(), 1);
        let Shared::Linked { name, url, removed, added } = decode_texts(&codes).unwrap() else { panic!() };
        assert_eq!((name.as_str(), url.as_str()), ("GOATED", "https://open.spotify.com/playlist/abc"));
        assert_eq!(removed, vec!["sp:sp00000000000000000000", "sp:sp00000000000000000001"]);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].id, "newvideo001");
    }

    #[test]
    fn qr_image_roundtrip() {
        let pl = Playlist { name: "QR".into(), tracks: (0..20).map(|i| track(Src::Yt, &format!("id{i:09}"))).collect(), source: None, ..Default::default() };
        let codes = encode(&pl);
        let img = render(&codes[0], 6).unwrap();
        let dir = std::env::temp_dir().join(format!("bhqr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("qr.png");
        img.save(&p).unwrap();
        let Shared::Full { name, tracks: t } = decode_files(&[p]).unwrap() else { panic!() };
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!((name.as_str(), t.len()), ("QR", 20));
    }
}

