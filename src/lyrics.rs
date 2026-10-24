//! Тексты песен: быстро с lrclib.net, а точные тайминги (по словам, из Apple Music/Musixmatch) — через Liriqo

use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

use crate::util::{agent, call};

#[derive(Clone)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Clone)]
pub struct Line {
    pub start: f64, // сек
    pub end: Option<f64>,
    pub text: String,
    pub words: Vec<Word>, // пусто — тайминги только по строке
    pub agent: Option<String>, // исполнитель (Apple: v1, v2, v2000…) — для дуэтов
    pub bg: bool,              // бэк-вокал: мелкой строкой под основной, как в Apple Music
}

#[derive(Clone)]
pub enum Lyrics {
    Synced(Vec<Line>),
    Plain(Vec<String>),
}

/// Кого показать под текстом: провайдер всегда, а для community-синков Spicy Lyrics — ещё и людей
#[derive(Clone, Default)]
pub struct Credit {
    pub provider: String,          // "Apple Music", "LRCLIB"...
    pub via: &'static str,         // агрегатор, через который пришло ("" — напрямую)
    pub people: Vec<(&'static str, String, String)>, // роль ("uploader"/"maker"), имя, ссылка
    pub community: bool,           // синк сделан людьми вручную — обычно самый точный
}

#[derive(Clone)]
pub struct Found {
    pub lyr: Lyrics,
    pub credit: Credit,
}

impl Found {
    /// Чем больше — тем лучше: сначала точность таймингов, при равной — community > Apple > остальные
    pub fn rank(&self) -> (u8, u8) {
        let src = if self.credit.community { 3 } else if self.credit.provider == "Apple Music" { 2 } else if self.credit.provider == "LRCLIB" { 0 } else { 1 };
        (self.lyr.quality(), src)
    }
}

/// Выбирает лучший текст из всех ответивших источников. lrclib — якорь: он подогнан под длительность
/// нашего трека, остальные выравниваются по нему, а другая версия трека (не сошлись строки) отсеивается
pub fn choose(all: &[Found]) -> Option<Found> {
    let anchor = all.iter().find(|f| f.credit.provider == "LRCLIB").and_then(|f| match &f.lyr {
        Lyrics::Synced(l) => Some(l),
        _ => None,
    });
    all.iter()
        .filter_map(|f| {
            let mut f = f.clone();
            if let (Some(anchor), Lyrics::Synced(lines)) = (anchor, &mut f.lyr) {
                if f.credit.provider != "LRCLIB" {
                    let Some(off) = offset(lines, anchor) else {
                        eprintln!("lyrics: у {} другая версия трека, пропускаю", f.credit.provider);
                        return None;
                    };
                    if off.abs() > 0.1 {
                        eprintln!("lyrics: сдвигаю {} на {off:.2} с", f.credit.provider);
                        shift(lines, off);
                    }
                }
            }
            Some(f)
        })
        .max_by_key(Found::rank)
}

/// Дисковый кеш ответов (~/.cache/BlackHole/lyrics): сеть дёргаем один раз на трек
pub fn cache_path(name: &str) -> Option<std::path::PathBuf> {
    Some(dirs::cache_dir()?.join("BlackHole").join("lyrics").join(format!("{:016x}.json", fnv(name))))
}

pub fn cache_get(name: &str, max_age: Duration) -> Option<Value> {
    let p = cache_path(name)?;
    let age = std::fs::metadata(&p).ok()?.modified().ok()?.elapsed().unwrap_or_default();
    if age > max_age {
        return None;
    }
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

pub fn cache_put(name: &str, v: &Value) {
    if let Some(p) = cache_path(name) {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, v.to_string());
    }
}

pub const FOREVER: Duration = Duration::from_secs(u64::MAX / 4);

impl Lyrics {
    /// Насколько точны тайминги: по словам > по строкам с концом строки > LRC > просто текст
    pub fn quality(&self) -> u8 {
        match self {
            Lyrics::Plain(_) => 1,
            Lyrics::Synced(l) if l.iter().filter(|x| !x.words.is_empty()).count() * 2 > l.len() => 4,
            Lyrics::Synced(l) if l.iter().all(|x| x.end.is_some()) => 3,
            Lyrics::Synced(_) => 2,
        }
    }
}

/// Убирает мусор из названий с YouTube: (Official Video), [HD], feat. ...
pub fn clean(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let lower = out.to_lowercase();
    let cut = [" feat.", " ft.", " feat ", " ft ", " | "].iter().filter_map(|p| lower.find(p)).min();
    if let Some(i) = cut {
        out.truncate(i);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ").trim_end_matches(" - Topic").to_string()
}

fn get(path: &str, params: &[(&str, &str)]) -> Result<Value> {
    for attempt in 0..3 {
        let mut req = agent().get(&format!("https://lrclib.net/api{path}")).set("Lrclib-Client", "BlackHole 0.1");
        for (k, v) in params {
            req = req.query(k, v);
        }
        match call(req) {
            // lrclib иногда отвечает 5xx — пробуем ещё пару раз
            Err(ureq::Error::Status(500..=599, _)) if attempt < 2 => std::thread::sleep(std::time::Duration::from_millis(600)),
            r => return Ok(r?.into_json()?),
        }
    }
    unreachable!()
}

fn pick(v: &Value) -> Option<Lyrics> {
    if let Some(s) = v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return Some(Lyrics::Synced(lines));
        }
    }
    let plain = v["plainLyrics"].as_str().filter(|s| !s.trim().is_empty())?;
    Some(Lyrics::Plain(plain.lines().map(|l| l.trim().to_string()).collect()))
}

pub fn fetch(artist: &str, title: &str, dur: u64) -> Result<Option<Lyrics>> {
    let (artist, title) = (clean(artist), clean(title));
    // YouTube-видосы часто "Артист - Трек" или "Трек - Артист": пробуем все варианты
    let mut pairs = vec![(artist.clone(), title.clone())];
    if let Some((x, y)) = title.split_once(" - ") {
        let (x, y) = (x.trim().to_string(), y.trim().to_string());
        pairs.push((x.clone(), y.clone()));
        pairs.push((y, x));
    }
    let d = dur.to_string();

    // 1. точное совпадение
    for (a, t) in &pairs {
        let mut params = vec![("artist_name", a.as_str()), ("track_name", t.as_str())];
        if dur > 0 {
            params.push(("duration", &d));
        }
        if let Some(l) = get("/get", &params).ok().as_ref().and_then(pick) {
            return Ok(Some(l));
        }
    }

    // 2. поиск: синхронный текст с близкой длительностью в приоритете
    let close = |x: &Value| dur == 0 || x["duration"].as_f64().is_some_and(|xd| (xd - dur as f64).abs() < 8.0);
    let mut queries: Vec<String> = pairs.iter().map(|(a, t)| format!("{a} {t}")).collect();
    queries.extend(pairs.iter().skip(1).map(|(_, t)| t.clone()));
    for q in queries {
        let items = get("/search", &[("q", &q)])?.as_array().cloned().unwrap_or_default();
        let best = items
            .iter()
            .find(|x| close(x) && x["syncedLyrics"].is_string())
            .or_else(|| items.iter().find(|x| close(x)));
        if let Some(l) = best.and_then(pick) {
            return Ok(Some(l));
        }
    }
    Ok(None)
}

fn parse_lrc(s: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in s.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            if let Some(t) = parse_time(&stripped[..end]) {
                times.push(t);
            }
            rest = stripped[end + 1..].trim_start();
        }
        for t in times {
            out.push(Line { start: t, end: None, text: rest.trim().to_string(), words: vec![], agent: None, bg: false });
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

fn parse_time(s: &str) -> Option<f64> {
    let (m, sec) = s.split_once(':')?;
    Some(m.trim().parse::<f64>().ok()? * 60.0 + sec.trim().parse::<f64>().ok()?)
}

/// Чем искать в Liriqo: точный videoId надёжнее всего
pub enum Query {
    Video(String),
    Url(String),
    Meta { artist: String, title: String, dur: u64 },
}

/// Liriqo (api.liriqo-alfarrizi.my.id) сам опрашивает Apple Music, Musixmatch, QQ, KuGou, lrclib...
/// и отдаёт тайминги вплоть до слогов. Берём трек с самыми точными таймингами.
pub fn fetch_liriqo(q: &Query) -> Result<Option<Found>> {
    let (k, v) = match q {
        Query::Video(id) => ("v", id.clone()),
        Query::Url(u) => ("url", u.clone()),
        Query::Meta { artist, title, dur } => ("meta", format!("{}\n{}\n{dur}", clean(artist), clean(title))),
    };
    // удачный ответ кешируем на диск: сервер Liriqo бывает отвечает по 15–20 с или 503,
    // и тогда показывался текст похуже. Со второго раза — сразу хороший
    let name = format!("{k}\n{v}");
    if let Some(t) = cache_get(&name, FOREVER) {
        return Ok(parse_liriqo(&t));
    }
    let mut req = agent().get("https://api.liriqo-alfarrizi.my.id/v1/lyrics").set("User-Agent", "BlackHole/0.1").timeout(Duration::from_secs(40));
    req = match q {
        Query::Video(id) => req.query("v", id),
        Query::Url(u) => req.query("url", u),
        Query::Meta { artist, title, dur } => {
            let req = req.query("artist", &clean(artist)).query("title", &clean(title));
            if *dur > 0 { req.query("duration", &dur.to_string()) } else { req }
        }
    };
    // повторяем и 5xx, и оборванный посередине ответ (у них такое бывает); 4xx — нет смысла
    let mut tries = 0;
    let v: Value = loop {
        let r = call(req.clone()).map_err(anyhow::Error::from).and_then(|r| Ok(r.into_json::<Value>()?));
        match r {
            Err(e) if tries < 4 && !matches!(e.downcast_ref(), Some(ureq::Error::Status(400..=499, _))) => {
                eprintln!("liriqo: {e:#}, повторяю");
                tries += 1;
                std::thread::sleep(Duration::from_secs(2));
            }
            r => break r?,
        }
    };
    let d = if v["data"].is_object() { &v["data"] } else { &v };
    let best = d["tracks"]
        .as_array()
        .into_iter()
        .flatten()
        // lrclib_wordsync — слова там просто размазаны по строке пропорционально, это не тайминги
        .filter(|t| t["provider"].as_str() != Some("lrclib_wordsync"))
        .filter_map(|t| Some((t, parse_liriqo(t)?)))
        .max_by_key(|(_, f)| f.rank());
    if let Some((t, f)) = &best {
        if f.lyr.quality() >= 3 {
            cache_put(&name, t);
        }
    }
    Ok(best.map(|(_, f)| f))
}

/// Человеческое имя провайдера Liriqo (binimum_apple_music → Apple Music)
fn provider_name(p: &str) -> String {
    let p = p.to_lowercase();
    let known = [("apple", "Apple Music"), ("musixmatch", "Musixmatch"), ("netease", "NetEase"), ("qq", "QQ Music"), ("kugou", "KuGou"),
        ("ytm", "YouTube Music"), ("youtube", "YouTube Music"), ("spotify", "Spotify"), ("lrclib", "LRCLIB"), ("unison", "Unison")];
    known.iter().find(|(k, _)| p.contains(k)).map_or(p.clone(), |(_, n)| n.to_string())
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn parse_liriqo(t: &Value) -> Option<Found> {
    let credit = Credit { provider: provider_name(t["provider"].as_str().unwrap_or("?")), via: "Liriqo", ..Default::default() };
    Some(Found { lyr: parse_liriqo_lyrics(t)?, credit })
}

fn parse_liriqo_lyrics(t: &Value) -> Option<Lyrics> {
    let ms = |v: &Value| v.as_f64().map(|x| x / 1000.0);
    let mut lines: Vec<Line> = t["timed"]
        .as_array()?
        .iter()
        .filter(|l| !l["isCue"].as_bool().unwrap_or(false))
        .filter_map(|l| {
            let words = l["words"]
                .as_array()
                .map(|ws| {
                    ws.iter()
                        .filter_map(|w| Some(Word { start: ms(&w["start"])?, end: ms(&w["end"])?, text: w["text"].as_str()?.to_string() }))
                        .collect()
                })
                .unwrap_or_default();
            let text = if l["isInstrumental"].as_bool().unwrap_or(false) { String::new() } else { l["text"].as_str()?.trim().to_string() };
            let agent = l["agent"].as_str().map(String::from);
            Some(Line { start: ms(&l["start"])?, end: ms(&l["end"]), text, words, agent, bg: false })
        })
        .collect();
    if lines.is_empty() {
        let plain = t["plain"].as_str().filter(|s| !s.trim().is_empty())?;
        return Some(Lyrics::Plain(plain.lines().map(|l| l.trim().to_string()).collect()));
    }
    lines.sort_by(|a, b| a.start.total_cmp(&b.start));
    mark_bg(&mut lines);
    sanitize_ends(&mut lines);
    Some(Lyrics::Synced(lines))
}

/// Liriqo теряет пометку бэк-вокала из TTML Apple, остаётся только текст в скобках. Строка целиком
/// в скобках, которая звучит поверх предыдущей, — это он и есть
fn mark_bg(lines: &mut [Line]) {
    for i in 1..lines.len() {
        let t = lines[i].text.trim();
        let over = lines[i - 1].end.is_some_and(|e| e > lines[i].start) && !lines[i - 1].bg;
        if over && t.len() > 2 && t.starts_with('(') && t.ends_with(')') && !t[1..].contains('(') {
            lines[i].text = t[1..t.len() - 1].trim().to_string();
            lines[i].bg = true;
        }
    }
}

/// NetEase и некоторые другие ставят фейковый конец строки (просто start + 5 с), и заливка растягивалась
/// на 5 секунд при строке в 2. Если концы массово залезают на следующую строку — им нельзя верить.
/// Единичные нахлёсты — это нормально: у Apple так звучат бэк-вокал и дуэты, их не трогаем.
fn sanitize_ends(lines: &mut [Line]) {
    let n = lines.len();
    let bad = lines.windows(2).filter(|w| w[0].end.is_some_and(|e| e > w[1].start + 0.3)).count();
    if bad * 10 > n * 3 {
        lines.iter_mut().for_each(|l| l.end = None);
    }
}

/// Строка в том виде, как её показывает экран текста
pub struct Disp {
    pub start: f64,
    pub end: f64,      // когда строка перестаёт быть активной (у Apple строки могут идти внахлёст)
    pub fill_end: f64, // докуда заливается строка без таймингов слов
    pub text: String,
    pub words: Vec<Word>,
    pub right: bool, // второй исполнитель — по правому краю, как в Apple Music
    pub dots: bool,  // "•••" в проигрыше
    pub bg: bool,
}

const DOTS_GAP: f64 = 4.5;

fn dots(start: f64, end: f64) -> Disp {
    Disp { start, end, fill_end: end, text: String::new(), words: vec![], right: false, dots: true, bg: false }
}

/// Готовит строки к показу: концы, дуэты (кто не основной исполнитель — справа), точки в интро
/// и в длинных проигрышах. Пустые строки LRC (метки проигрыша) тоже становятся точками.
pub fn display_lines(raw: &[Line]) -> Vec<Disp> {
    let mut count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for a in raw.iter().filter_map(|l| l.agent.as_deref()) {
        *count.entry(a).or_default() += 1;
    }
    let main = count.into_iter().max_by_key(|(_, n)| *n).map(|(a, _)| a.to_string());
    let mut out = Vec::new();
    if let Some(first) = raw.first().filter(|l| l.start > DOTS_GAP + 0.5) {
        out.push(dots(0.5, first.start - 0.2));
    }
    for (i, l) in raw.iter().enumerate() {
        let next = raw.get(i + 1).map(|n| n.start);
        if l.text.trim().is_empty() {
            let end = next.unwrap_or(l.start + 5.0);
            if end - l.start >= 2.0 {
                out.push(dots(l.start, end - 0.2));
            }
            continue;
        }
        let gap = next.unwrap_or(l.start + 6.0) - l.start;
        let (end, fill_end) = match l.end {
            Some(e) if e > l.start => (e, e),
            // без конца строки: активна до следующей, заливку угадываем — поют обычно не до самой следующей
            _ => (l.start + gap, l.start + (gap * 0.92).min(0.6 + l.text.chars().count() as f64 * 0.11).max(0.2)),
        };
        out.push(Disp {
            start: l.start,
            end,
            fill_end,
            text: l.text.clone(),
            words: l.words.iter().map(|w| Word { start: w.start, end: w.end, text: w.text.clone() }).collect(),
            right: l.agent.is_some() && l.agent != main,
            dots: false,
            bg: l.bg,
        });
        if let (Some(ns), Some(_)) = (next, l.end) {
            if ns - end > DOTS_GAP {
                out.push(dots(end + 0.2, ns - 0.2));
            }
        }
    }
    out
}

/// Какие строки активны в момент pos (их может быть несколько — нахлёст у Apple) и по какой
/// строке держать прокрутку. Если ни одна не активна (короткая пауза) — подсвечена последняя начатая.
pub fn active(starts: &[f64], ends: &[f64], pos: f64) -> (i32, Vec<usize>) {
    let act: Vec<usize> = (0..starts.len()).filter(|&i| starts[i] <= pos && pos < ends[i]).collect();
    let cur = act.first().map(|&i| i as i32).unwrap_or(starts.partition_point(|&s| s <= pos) as i32 - 1);
    (cur, act)
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Liriqo часто отдаёт тайминги альбомной версии из Apple Music, а играет YouTube-видео с интро или
/// другим мастером — всё съезжает на секунды. lrclib ищется по длительности именно нашего трека, поэтому
/// служит опорой: находим одинаковые строки и берём медианный сдвиг.
/// None — разброс большой, значит версии трека разные и точным таймингам верить нельзя.
pub fn offset(precise: &[Line], anchor: &[Line]) -> Option<f64> {
    let mut diffs: Vec<f64> = precise
        .iter()
        .filter_map(|l| {
            let n = norm(&l.text);
            if n.len() < 4 {
                return None;
            }
            // одинаковые строки (припев) — берём ближайшую по времени
            let a = anchor.iter().filter(|a| norm(&a.text) == n).min_by(|x, y| (x.start - l.start).abs().total_cmp(&(y.start - l.start).abs()))?;
            Some(a.start - l.start).filter(|d| d.abs() < 30.0)
        })
        .collect();
    if diffs.len() < 3 {
        return Some(0.0); // сравнить не с чем (другой язык/транскрипция) — оставляем как есть
    }
    diffs.sort_by(f64::total_cmp);
    let m = diffs[diffs.len() / 2];
    let mut dev: Vec<f64> = diffs.iter().map(|d| (d - m).abs()).collect();
    dev.sort_by(f64::total_cmp);
    (dev[dev.len() / 2] <= 1.0).then_some(m)
}

pub fn shift(lines: &mut [Line], off: f64) {
    for l in lines {
        l.start += off;
        if let Some(e) = &mut l.end {
            *e += off;
        }
        for w in &mut l.words {
            w.start += off;
            w.end += off;
        }
    }
}

/// Режет строку на визуальные ряды по словам — чтобы караоке-заливка шла ряд за рядом
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for w in text.split_whitespace() {
        match rows.last_mut() {
            Some(r) if r.chars().count() + 1 + w.chars().count() <= max => {
                r.push(' ');
                r.push_str(w);
            }
            _ => rows.push(w.to_string()),
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    #[test]
    fn lrc() {
        let l = super::parse_lrc("[ar:x]\n[00:01.50] a\n[00:03.00][01:00.00] b\n[00:02.00]");
        let v: Vec<_> = l.iter().map(|l| (l.start, l.text.as_str())).collect();
        assert_eq!(v, vec![(1.5, "a"), (2.0, ""), (3.0, "b"), (60.0, "b")]);
    }

    #[test]
    fn clean() {
        assert_eq!(super::clean("Instant Crush (feat. Julian Casablancas)"), "Instant Crush");
        assert_eq!(super::clean("Get Lucky [Official Video] ft. Pharrell"), "Get Lucky");
    }

    #[test]
    fn wrap() {
        assert_eq!(super::wrap("I didn't want to be the one to forget", 16), vec!["I didn't want to", "be the one to", "forget"]);
    }

    fn line(start: f64, text: &str) -> super::Line {
        super::Line { start, end: None, text: text.into(), words: vec![], agent: None, bg: false }
    }

    #[test]
    fn overlap_like_apple() {
        // Cigarettes Out the Window, 3:01: основная строка и поверх неё бэк-вокал другого исполнителя
        let ag = |a: &str| Some(a.to_string());
        let raw = vec![
            super::Line { end: Some(176.93), agent: ag("v2000"), ..line(176.11, "You mind if I smoke?") },
            super::Line { end: Some(181.10), agent: ag("v2000"), ..line(177.90, "You'll find moonlit nights strangely empty") },
            super::Line { end: Some(184.52), agent: ag("v2000"), ..line(181.10, "Because when you call my name through them") },
            super::Line { end: Some(184.31), agent: ag("v1"), ..line(182.10, "Ooh- ooh") },
            super::Line { end: Some(185.58), agent: ag("v2000"), ..line(184.31, "I always smoke with the theatre") },
            super::Line { end: Some(200.0), agent: ag("v2000"), ..line(195.0, "after a long break") },
        ];
        let d = super::display_lines(&raw);
        let texts: Vec<&str> = d.iter().map(|x| if x.dots { "•••" } else { x.text.as_str() }).collect();
        // интро-точки (первая строка на 176 с) и точки в паузе 185.6 → 195
        assert_eq!(texts.first(), Some(&"•••"));
        assert!(texts.contains(&"•••") && texts[texts.len() - 2] == "•••", "{texts:?}");
        let ooh = d.iter().position(|x| x.text == "Ooh- ooh").unwrap();
        assert!(d[ooh].right && !d[ooh - 1].right, "бэк-вокал другого исполнителя — справа");
        let (starts, ends): (Vec<f64>, Vec<f64>) = d.iter().map(|x| (x.start, x.end)).unzip();
        // 3:02.5 — играют обе: основная строка не пропадает
        let (cur, act) = super::active(&starts, &ends, 182.5);
        assert_eq!(act, vec![ooh - 1, ooh]);
        assert_eq!(cur as usize, ooh - 1);
        // 184.4 — "Ooh" кончилась, основная ещё звучит, следующая уже началась
        let (_, act) = super::active(&starts, &ends, 184.4);
        assert_eq!(act, vec![ooh - 1, ooh + 1]);
    }

    #[test]
    fn fake_ends_dropped() {
        // как у NetEase: каждая строка "длится" 5 с, хотя следующая начинается через 2
        let mut l: Vec<super::Line> = (0..10).map(|i| super::Line { end: Some(i as f64 * 2.0 + 5.0), ..line(i as f64 * 2.0, "x") }).collect();
        super::sanitize_ends(&mut l);
        assert!(l.iter().all(|x| x.end.is_none()));
        assert_eq!(super::Lyrics::Synced(l).quality(), 2);
        // нормальные концы и единичный нахлёст (бэк-вокал) остаются как есть
        let mut l = vec![super::Line { end: Some(1.9), ..line(0.0, "a") }, super::Line { end: Some(4.2), ..line(2.0, "b") }, super::Line { end: Some(5.0), ..line(4.0, "c") }];
        super::sanitize_ends(&mut l);
        assert_eq!(l.iter().map(|x| x.end.unwrap()).collect::<Vec<_>>(), vec![1.9, 4.2, 5.0]);
    }

    #[test]
    fn offset() {
        let anchor = vec![line(10.0, "Hello there"), line(14.0, "Chorus line!"), line(20.0, "Another one"), line(30.0, "Chorus line")];
        // точные тайминги отстают на 2.5 с
        let precise = vec![line(7.5, "hello there"), line(11.5, "Chorus line"), line(17.5, "Another one"), line(27.5, "chorus line")];
        assert_eq!(super::offset(&precise, &anchor), Some(2.5));
        // разные версии: сдвиги скачут
        let other = vec![line(1.0, "hello there"), line(11.5, "Chorus line"), line(40.0, "Another one"), line(50.0, "chorus line")];
        assert_eq!(super::offset(&other, &anchor), None);
    }

    /// Дуэт из Apple Music (слева и справа) должен приходить и со второго раза — из кеша
    #[test]
    #[ignore]
    fn liriqo_duet_cached() {
        let q = super::Query::Meta { artist: "TV Girl".into(), title: "Cigarettes out the Window".into(), dur: 184 };
        for _ in 0..2 {
            let t = std::time::Instant::now();
            let Some(super::Found { lyr: super::Lyrics::Synced(l), .. }) = super::fetch_liriqo(&q).unwrap() else { panic!("нет текста") };
            let agents: std::collections::HashSet<_> = l.iter().filter_map(|l| l.agent.clone()).collect();
            println!("{} строк, агенты {agents:?}, {:?}", l.len(), t.elapsed());
            assert!(agents.len() >= 2);
        }
    }

    #[test]
    #[ignore]
    fn liriqo_words() {
        let l = super::fetch_liriqo(&super::Query::Meta { artist: "Eminem".into(), title: "Rap God".into(), dur: 364 }).unwrap().unwrap().lyr;
        assert_eq!(l.quality(), 4);
        let l = super::fetch_liriqo(&super::Query::Video("HaEYUJ2aRHs".into())).unwrap().unwrap().lyr;
        println!("dynamite quality {}", l.quality());
        assert!(l.quality() >= 3);
    }

    #[test]
    #[ignore]
    fn fetch_net() {
        assert!(matches!(super::fetch("Daft Punk", "Instant Crush (feat. Julian Casablancas)", 338).unwrap(), Some(super::Lyrics::Synced(_))));
        assert!(super::fetch("Peso Pluma", "BELLAKEO (Video Oficial) - Peso Pluma, Anitta", 235).unwrap().is_some());
    }
}
