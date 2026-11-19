//! «Угадай песню»: из плейлиста или артиста играет случайный трек — сначала 0.1 с, потом 0.5, 1, 2 и 5 с.
//! Угадал — вводишь название (поиск как обычный). Ответ сверяется не по id: трек мог прийти с SoundCloud,
//! а выбран в поиске YouTube Music — сравниваем очищенные названия и исполнителей.
//! Общее для ПК и телефона; нарезка фрагментов (ffmpeg) — только ПК, телефон режет плеером.

use std::collections::HashSet;

use crate::model::Track;

/// Длительность фрагмента на каждом уровне, секунды
pub const LEVELS: [f64; 5] = [0.1, 0.5, 1.0, 2.0, 5.0];

/// Очки за угаданное на уровне
pub fn points(level: usize) -> u32 {
    [100, 60, 40, 25, 10].get(level).copied().unwrap_or(0)
}

/// Слова, по которым хвост после « - » — это не часть названия, а пометка версии
const NOISE: [&str; 16] = ["remaster", "live", "version", "edit", "mix", "radio", "official", "video", "audio", "lyric", "prod", "visualizer", "mono", "stereo", "explicit", "clip"];

fn letters(s: &str) -> String {
    let mut out = String::new();
    let mut space = true;
    for c in s.to_lowercase().replace('ё', "е").chars() {
        if c.is_alphanumeric() {
            out.push(c);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim().to_string()
}

/// Без скобок любого вида
fn unbracket(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Исполнители трека, нормализованные: из поля артиста и из «Артист - Название» в названии
fn artists(t: &Track) -> HashSet<String> {
    let mut v: HashSet<String> = crate::graph::track_artists(t).iter().map(|a| letters(a)).filter(|a| !a.is_empty()).collect();
    if let Some((head, _)) = split_dash(&t.title) {
        for a in crate::stats::artists(head) {
            let a = letters(a);
            if !a.is_empty() {
                v.insert(a);
            }
        }
    }
    v
}

fn split_dash(s: &str) -> Option<(&str, &str)> {
    for sep in [" - ", " – ", " — "] {
        if let Some(i) = s.find(sep) {
            return Some((&s[..i], &s[i + sep.len()..]));
        }
    }
    None
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn drop_feat(mut s: String) -> String {
    for tag in ["feat ", "ft ", "featuring ", "prod "] {
        if let Some(i) = s.find(&format!(" {tag}")) {
            s.truncate(i);
        } else if s.starts_with(tag) {
            s.clear();
        }
    }
    s.trim().to_string()
}

/// Варианты названия для сравнения: главный (с срезанным «Артист - » или хвостом версии) и запасные —
/// целиком и половинки вокруг тире. Так «kanyewestofficial: Kanye West - Stronger» совпадёт со «Stronger»
fn variants(t: &Track) -> Vec<String> {
    let arts: Vec<String> = crate::graph::track_artists(t).iter().map(|a| compact(&letters(a))).filter(|a| !a.is_empty()).collect();
    let title = unbracket(&t.title);
    let mut out = Vec::new();
    match split_dash(&title) {
        Some((head, tail)) => {
            let (h, tl) = (letters(head), letters(tail));
            let hc = compact(&h);
            let artist_head = arts.iter().any(|a| *a == hc || (hc.len() >= 4 && a.contains(hc.as_str())) || (a.len() >= 4 && hc.contains(a.as_str())));
            let noise = NOISE.iter().any(|n| tl.contains(n));
            if (artist_head || t.src == crate::model::Src::Sc) && !noise && !tl.is_empty() {
                out.push(drop_feat(tl.clone()));
            } else if noise {
                out.push(drop_feat(h.clone()));
            }
            out.push(drop_feat(letters(&title)));
            for v in [h, tl] {
                if v.chars().count() >= 3 {
                    out.push(drop_feat(v));
                }
            }
        }
        None => out.push(drop_feat(letters(&title))),
    }
    out.retain(|v| !v.is_empty());
    out.dedup();
    out
}

/// Чистое название: без «Артист - », скобок, «feat.» и пометок версии
pub fn clean_title(t: &Track) -> String {
    variants(t).into_iter().next().unwrap_or_default()
}

fn lev(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = cur;
        }
    }
    row[b.len()]
}

/// Почти одинаковые: опечатка-другая на длинном названии прощается
fn close(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let n = a.chars().count().max(b.chars().count());
    a == b || (n >= 6 && lev(a, b) <= n / 8)
}

/// Выбранный в поиске трек — это загаданная песня? Название (очищенное) совпадает и есть общий исполнитель.
/// Исполнитель ищется и в поле артиста, и в названии: у SoundCloud в артисте часто канал, а артист — в «Артист - Песня»
pub fn same(answer: &Track, guess: &Track) -> bool {
    if answer.src == guess.src && answer.id == guess.id && !answer.id.is_empty() {
        return true;
    }
    let (va, vg) = (variants(answer), variants(guess));
    if !va.iter().any(|a| vg.iter().any(|g| close(a, g))) {
        return false;
    }
    let (a, g) = (artists(answer), artists(guess));
    if a.is_empty() || g.is_empty() {
        return true;
    }
    let raw = letters(&format!("{} {}", answer.artist, answer.title));
    let (a, g): (Vec<String>, Vec<String>) = (a.iter().map(|x| compact(x)).collect(), g.iter().map(|x| compact(x)).collect());
    let raw = compact(&raw);
    a.iter().any(|x| g.iter().any(|y| x == y || (x.len() >= 4 && y.contains(x.as_str())) || (y.len() >= 4 && x.contains(y.as_str())))) || g.iter().any(|y| y.len() >= 3 && raw.contains(y.as_str()))
}

/// Ввели название руками и нажали Enter
pub fn typed(answer: &Track, text: &str) -> bool {
    let t = drop_feat(letters(&unbracket(text)));
    variants(answer).iter().any(|a| close(a, &t))
}

/// С какого места резать фрагмент: с начала песни (так её и узнают). Тишину в самом начале ПК пропускает
/// сам при нарезке (make_clips с trim); в дуэли — ровно с нуля у обоих, чтобы было честно
pub fn clip_start(_t: &Track, _dur: f64) -> f64 {
    0.0
}

/// Простой поиск по названию и исполнителю среди песен игры: начинающиеся с запроса — первыми
pub fn search_pool(pool: &[Track], q: &str, limit: usize) -> Vec<usize> {
    let q = letters(q);
    if q.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u8, usize)> = pool
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let title = letters(&t.title);
            let all = format!("{title} {}", letters(&t.artist));
            if title.starts_with(&q) {
                Some((0, i))
            } else if title.contains(&q) {
                Some((1, i))
            } else if all.contains(&q) {
                Some((2, i))
            } else {
                None
            }
        })
        .collect();
    hits.sort();
    hits.into_iter().take(limit).map(|h| h.1).collect()
}

/// Перемешать (Фишер — Йейтс на xorshift, засеянном временем — для игры этого хватает)
pub fn shuffle(v: &mut [Track]) {
    let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) | 1;
    for i in (1..v.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

/// Без повторов одной песни (одна и та же в нескольких версиях — один раз)
pub fn dedupe(v: Vec<Track>) -> Vec<Track> {
    let mut seen = HashSet::new();
    v.into_iter().filter(|t| !t.title.is_empty() && seen.insert(clean_title(t))).collect()
}

/// Песни артиста для игры: популярные и, если есть, все песни из YouTube Music
pub fn artist_pool(browse_id: &str, name: &str) -> anyhow::Result<Vec<Track>> {
    let a = crate::ytmusic::artist(browse_id)?;
    let mut v = a.top.clone();
    if let Some(id) = &a.songs_id {
        if let Ok(all) = crate::ytmusic::all_songs(id, if a.name.is_empty() { name } else { &a.name }) {
            v.extend(all);
        }
    }
    let mut v = dedupe(v);
    if v.len() < 3 {
        anyhow::bail!("у артиста слишком мало песен");
    }
    shuffle(&mut v);
    Ok(v)
}

/// ПК: нарезать фрагменты всех уровней одним заходом. src — файл или прямая ссылка на поток.
/// ffmpeg отдаёт 5 с звука с start, дальше — WAV на каждый уровень с мягкими краями (без щелчков)
#[cfg(not(target_os = "android"))]
pub fn make_clips(src: &str, start: f64, trim: bool, dir: &std::path::Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    use anyhow::Context;
    use std::io::{Read, Write};
    const SR: u32 = 44_100;
    let ffmpeg = crate::util::find_bin("ffmpeg").context("для игры нужен ffmpeg")?;
    let mut child = crate::util::command(&ffmpeg)
        .args(["-v", "error", "-nostdin", "-ss", &format!("{start:.2}"), "-i", src, "-t", if trim { "9" } else { "5.2" }, "-vn", "-ac", "2", "-ar", &SR.to_string(), "-f", "s16le", "-"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("ffmpeg не запустился")?;
    let mut raw = Vec::new();
    child.stdout.take().context("нет вывода ffmpeg")?.read_to_end(&mut raw)?;
    child.wait()?;
    let mut pcm: Vec<i16> = raw.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    if trim {
        // тишина в начале трека (бывает и пара секунд) — мимо: иначе 0.1 с беззвучные
        let quiet = pcm.chunks(2).take(SR as usize * 4).position(|f| f.iter().any(|x| x.unsigned_abs() > 330)).unwrap_or(0);
        pcm.drain(..quiet.saturating_sub(SR as usize / 100) * 2);
    }
    if pcm.len() < (SR as usize) * 2 {
        anyhow::bail!("не вышло достать звук");
    }
    std::fs::create_dir_all(dir)?;
    let mut out = Vec::new();
    for (i, len) in LEVELS.iter().enumerate() {
        let frames = ((len * SR as f64) as usize).min(pcm.len() / 2);
        let fade_in = (SR as usize / 200).min(frames / 4); // 5 мс
        let fade_out = ((SR as f64 * 0.025) as usize).min(frames / 3); // 25 мс
        let mut data = Vec::with_capacity(frames * 4);
        for f in 0..frames {
            let k = if f < fade_in {
                f as f32 / fade_in as f32
            } else if f + fade_out > frames {
                (frames - f) as f32 / fade_out as f32
            } else {
                1.0
            };
            for c in 0..2 {
                let s = (pcm[f * 2 + c] as f32 * k) as i16;
                data.extend_from_slice(&s.to_le_bytes());
            }
        }
        let p = dir.join(format!("clip{i}.wav"));
        let mut w = std::fs::File::create(&p)?;
        let n = data.len() as u32;
        w.write_all(b"RIFF")?;
        w.write_all(&(36 + n).to_le_bytes())?;
        w.write_all(b"WAVEfmt ")?;
        w.write_all(&16u32.to_le_bytes())?;
        w.write_all(&1u16.to_le_bytes())?; // PCM
        w.write_all(&2u16.to_le_bytes())?;
        w.write_all(&SR.to_le_bytes())?;
        w.write_all(&(SR * 4).to_le_bytes())?;
        w.write_all(&4u16.to_le_bytes())?;
        w.write_all(&16u16.to_le_bytes())?;
        w.write_all(b"data")?;
        w.write_all(&n.to_le_bytes())?;
        w.write_all(&data)?;
        out.push(p);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Src;

    fn t(src: Src, title: &str, artist: &str) -> Track {
        Track { src, id: format!("{title}|{artist}"), title: title.into(), artist: artist.into(), ..Default::default() }
    }

    #[test]
    fn cross_source_answers() {
        let sc = t(Src::Sc, "Kanye West - Stronger (Official Video)", "kanyewestofficial");
        assert!(same(&sc, &t(Src::Yt, "Stronger", "Kanye West")), "SoundCloud «Артист - Песня» против YouTube");
        let a = t(Src::Yt, "Blinding Lights", "The Weeknd");
        assert!(same(&a, &t(Src::Sc, "Blinding Lights - Remastered", "The Weeknd")));
        assert!(same(&a, &t(Src::Yt, "Blinding Lights (feat. Nobody)", "The Weeknd, Nobody")));
        assert!(!same(&a, &t(Src::Yt, "Save Your Tears", "The Weeknd")), "другая песня того же артиста");
        assert!(!same(&a, &t(Src::Yt, "Blinding Lights", "Some Cover Band")), "одноимённая песня другого артиста");
        let ru = t(Src::Ya, "Ёлка", "Скриптонит");
        assert!(same(&ru, &t(Src::Yt, "Елка", "Скриптонит")));
    }

    #[test]
    fn typed_answers() {
        let a = t(Src::Yt, "Blinding Lights", "The Weeknd");
        assert!(typed(&a, "blinding lights"));
        assert!(typed(&a, "Blinding Light"), "опечатка на длинном названии");
        assert!(!typed(&a, "lights"));
        assert!(!typed(&a, ""));
    }

    #[test]
    fn clean_titles() {
        assert_eq!(clean_title(&t(Src::Sc, "Mac DeMarco - Chamber of Reflection [HD]", "macdemarco")), "chamber of reflection");
        assert_eq!(clean_title(&t(Src::Yt, "Song ft. X", "A")), "song");
        assert_eq!(clean_title(&t(Src::Yt, "Hey - Jude", "The Beatles")), "hey jude", "тире внутри названия — не пометка");
        assert!(same(&t(Src::Yt, "Hey - Jude", "The Beatles"), &t(Src::Sc, "The Beatles - Hey Jude", "beatlesofficial")));
        assert_eq!(points(0), 100);
        assert_eq!(points(5), 0);
    }
}
