use anyhow::Context;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

pub fn agent() -> &'static ureq::Agent {
    static A: OnceLock<ureq::Agent> = OnceLock::new();
    A.get_or_init(|| {
        // без пула: университетский NAT молча рвёт простаивающие соединения, и запрос по такому
        // висел до таймаута. Свежее соединение на каждый запрос стоит ~40 мс на TLS.
        ureq::AgentBuilder::new()
            .max_idle_connections(0)
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
            .build()
    })
}

/// Запрос с одним автоповтором при сетевой ошибке (не при HTTP-статусе)
pub fn call(req: ureq::Request) -> Result<ureq::Response, ureq::Error> {
    match req.clone().call() {
        Err(ureq::Error::Transport(e)) => {
            eprintln!("сеть: {e}, повторяю");
            req.call()
        }
        r => r,
    }
}

pub fn send_json(req: ureq::Request, body: serde_json::Value) -> Result<ureq::Response, ureq::Error> {
    match req.clone().send_json(body.clone()) {
        Err(ureq::Error::Transport(e)) => {
            eprintln!("сеть: {e}, повторяю");
            req.send_json(body)
        }
        r => r,
    }
}

/// Ищет бинарь рядом с exe, в PATH, в ~/.local/bin и (на маке) в Homebrew
pub fn find_bin(name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) {
        dirs.push(d.join("bin"));
        dirs.push(d);
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(h) = dirs::home_dir() {
        dirs.push(h.join(".local/bin"));
    }
    // .app из Finder запускается с урезанным PATH (/usr/bin:/bin) — Homebrew надо искать самим
    if cfg!(target_os = "macos") {
        dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    }
    dirs.into_iter().map(|d| d.join(&file)).find(|p| p.is_file())
}

/// Command без всплывающего консольного окна на винде
pub fn command(p: &std::path::Path) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(p);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Браузер, из которого yt-dlp берёт куки YouTube ("" — без кук). Ставится из настроек
static COOKIES: RwLock<String> = RwLock::new(String::new());

pub fn set_cookies_browser(b: &str) {
    *COOKIES.write().unwrap() = b.to_string();
}

/// Новый Firefox хранит профили в ~/.config/mozilla/firefox, а yt-dlp ищет только в ~/.mozilla —
/// тогда отдаём ему путь к профилю сами (тот, где куки свежее всего)
fn browser_arg(b: &str) -> String {
    if b != "firefox" {
        return b.to_string();
    }
    let Some(home) = dirs::home_dir() else { return b.to_string() };
    let xdg = home.join(".config/mozilla/firefox");
    if home.join(".mozilla/firefox").is_dir() || !xdg.is_dir() {
        return b.to_string();
    }
    let newest = std::fs::read_dir(&xdg).into_iter().flatten().flatten().filter_map(|e| {
        let m = e.path().join("cookies.sqlite").metadata().ok()?.modified().ok()?;
        Some((m, e.path()))
    });
    match newest.max_by_key(|(m, _)| *m) {
        Some((_, p)) => format!("firefox:{}", p.display()),
        None => b.to_string(),
    }
}

/// Куки YouTube из выбранного браузера (достаёт сам yt-dlp, он умеет все браузеры) — для переноса входа
/// на телефон. JSON [[имя, значение], …]
pub fn yt_cookies() -> anyhow::Result<String> {
    let b = COOKIES.read().unwrap().clone();
    if b.is_empty() {
        anyhow::bail!("сначала выбери браузер в «Куки YouTube»");
    }
    let py = find_bin("python3").or_else(|| find_bin("python")).context("нет Python")?;
    let ytdlp = find_bin("yt-dlp").context("нет yt-dlp")?;
    let spec = browser_arg(&b);
    let (name, profile) = spec.split_once(':').unwrap_or((&spec, ""));
    const SCRIPT: &str = r#"
import sys, json
sys.path.insert(0, sys.argv[1])
from yt_dlp.cookies import extract_cookies_from_browser
jar = extract_cookies_from_browser(sys.argv[2], sys.argv[3] or None)
got = {c.name: c.value for c in jar if c.domain.endswith('youtube.com') and c.value}
print(json.dumps(sorted(got.items())))
"#;
    let out = command(&py).arg("-c").arg(SCRIPT).arg(&ytdlp).arg(name).arg(profile).output()?;
    let v: Vec<(String, String)> = serde_json::from_slice(&out.stdout).context("yt-dlp не отдал куки")?;
    if !v.iter().any(|(n, _)| n == "SAPISID" || n == "__Secure-3PAPISID") {
        anyhow::bail!("в {b} нет входа в YouTube");
    }
    Ok(serde_json::to_string(&v)?)
}

/// yt-dlp с нужными флагами: куки браузера (если включены) и JS-рантайм. Свежему YouTube нужен JS,
/// а yt-dlp по умолчанию знает только deno — если его нет, подсовываем node или bun
fn ytdlp_cmd(cookies: Option<&str>) -> Option<Command> {
    let mut c = command(&find_bin("yt-dlp")?);
    c.args(ytdlp_args(cookies));
    Some(c)
}

/// Флаги yt-dlp: куки браузера (если включены) и JS-рантайм
pub fn ytdlp_args(cookies: Option<&str>) -> Vec<String> {
    let mut a = vec![];
    if let Some(b) = cookies {
        a.extend(["--cookies-from-browser".to_string(), browser_arg(b)]);
    }
    // ссылки YouTube теперь у всех клиентов зашифрованы — без JS-движка yt-dlp звук не достанет
    if find_bin("deno").is_none() {
        if let Some((name, p)) = ["node", "bun"].iter().find_map(|n| Some((*n, find_bin(n)?))) {
            a.extend(["--js-runtimes".to_string(), format!("{name}:{}", p.display())]);
        }
    }
    a
}

/// Куки сейчас нужны сразу (YouTube недавно просил доказать, что не бот)
fn cookies_now() -> Option<String> {
    let b = COOKIES.read().unwrap().clone();
    (!b.is_empty() && now().saturating_sub(bot_at()) < BOT_STICKY).then_some(b)
}

/// Когда YouTube последний раз требовал куки. Лежит на диске: иначе после каждого запуска плеер
/// сначала пробовал без кук, получал отказ и терял на этом пару секунд первого трека
fn bot_at() -> u64 {
    let mem = BOT_AT.load(Ordering::Relaxed);
    if mem != 0 {
        return mem;
    }
    let disk = bot_file().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    BOT_AT.store(disk, Ordering::Relaxed);
    disk
}

fn mark_bot() {
    BOT_AT.store(now(), Ordering::Relaxed);
    if let Some(p) = bot_file() {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, now().to_string());
    }
}

fn bot_file() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("BlackHole").join("bot_at"))
}

/// Прямые ссылки на звук: через живой yt-dlp (см. ytw), а если он недоступен — обычным запуском.
/// Куки — как в ytdlp_run: без них, а упёрлись в проверку на бота — с ними (и дальше сутки сразу с ними)
pub fn stream_urls(urls: &[String], bg: bool) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let b = COOKIES.read().unwrap().clone();
    let first = cookies_now();
    let Some((mut got, errs)) = crate::ytw::resolve(urls, &ytdlp_args(first.as_deref()), bg) else { return stream_urls_cli(urls) };
    let is_bot = |e: &String| ytdlp_error(e.as_bytes()) == BOT;
    let mut errs = errs;
    if first.is_none() && !b.is_empty() && errs.values().any(is_bot) {
        mark_bot();
        let again: Vec<String> = errs.keys().cloned().collect();
        if let Some((g, e)) = crate::ytw::resolve(&again, &ytdlp_args(Some(&b)), bg) {
            got.extend(g);
            errs = e;
        }
    }
    if got.is_empty() {
        if let Some(e) = errs.values().next() {
            anyhow::bail!("{}", ytdlp_error(e.as_bytes()));
        }
    }
    Ok(got)
}

/// Пачка ссылок одним запуском yt-dlp: питон, куки и плеер YouTube грузятся один раз —
/// с куками ~8.6 с CPU на первый трек и ~3.5 на каждый следующий
fn stream_urls_cli(urls: &[String]) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let out = ytdlp_run(|c| {
        c.args(["-f", "bestaudio/best", "--no-playlist", "--no-warnings", "-i", "--print", "%(original_url)s %(url)s"]).args(urls);
    })?;
    let map: std::collections::HashMap<String, String> =
        String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split_once(' ')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
    if map.is_empty() && !out.status.success() {
        anyhow::bail!("{}", ytdlp_error(&out.stderr));
    }
    Ok(map)
}

/// Поднять живой yt-dlp заранее — с теми куками, с какими он сейчас пойдёт
pub fn warm_ytdlp() {
    crate::ytw::warm(ytdlp_args(cookies_now().as_deref()));
}

// когда YouTube последний раз требовал куки (секунды unix, 0 — не требовал)
static BOT_AT: AtomicU64 = AtomicU64::new(0);
const BOT_STICKY: u64 = 24 * 3600;

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Запуск yt-dlp. С куками он вдвое медленнее (~7 с против ~3), поэтому сначала пробуем без них,
/// а куки подключаем, только если YouTube упёрся в проверку на бота — и дальше час сразу с куками
pub fn ytdlp_run(args: impl Fn(&mut Command)) -> anyhow::Result<Output> {
    let b = COOKIES.read().unwrap().clone();
    if cookies_now().is_none() {
        let mut c = ytdlp_cmd(None).context("yt-dlp не найден")?;
        args(&mut c);
        let out = c.output()?;
        if out.status.success() || b.is_empty() || ytdlp_error(&out.stderr) != BOT {
            return Ok(out);
        }
        mark_bot();
    }
    let mut c = ytdlp_cmd(Some(&b)).context("yt-dlp не найден")?;
    args(&mut c);
    Ok(c.output()?)
}

/// Понятный текст вместо простыни yt-dlp
pub fn ytdlp_error(stderr: &[u8]) -> String {
    let err = String::from_utf8_lossy(stderr);
    let last = err.lines().rev().find(|l| l.contains("ERROR")).or(err.lines().last()).unwrap_or("yt-dlp упал");
    if last.contains("not a bot") || last.contains("Sign in to confirm") {
        return BOT.to_string();
    }
    last.trim_start_matches("ERROR: ").to_string()
}

/// Маркер ошибки "YouTube требует доказать, что ты не бот" — по нему UI показывает подсказку про куки
pub const BOT: &str = "youtube-bot-check";

#[cfg(test)]
mod tests {
    #[test]
    fn bot_check_detected() {
        let e = b"WARNING: something\nERROR: [youtube] abc: Sign in to confirm you\xe2\x80\x99re not a bot. This helps protect our community.";
        assert_eq!(super::ytdlp_error(e), super::BOT);
        assert_eq!(super::ytdlp_error(b"ERROR: [youtube] x: Video unavailable"), "[youtube] x: Video unavailable");
    }
}

// ── журнал ошибок: всё, что раньше уходило в stderr и терялось, лежит в файле — его можно скопировать из настроек ──

pub fn log_path() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("BlackHole").join("log.txt")
}

/// Строка в журнал (и в stderr, как раньше). Файл не растёт без конца: перевалил за 512 КБ — остаётся свежая половина
pub fn log(msg: &str) {
    use std::io::Write;
    let msg = &squash(&redact(msg));
    eprintln!("{msg}");
    if cfg!(test) {
        return; // тесты в журнал пользователя не пишут
    }
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let p = log_path();
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > 512 * 1024) {
        if let Ok(old) = std::fs::read(&p) {
            let tail = &old[old.len() - 256 * 1024..];
            let cut = tail.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
            let _ = std::fs::write(&p, &tail[cut..]);
        }
    }
    let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{} {msg}", stamp(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())));
    }
}

/// Журнал копируют и пересылают — токенов входа в нём быть не должно. Ошибки сети печатают адрес запроса
/// целиком, вместе с «auth=<токен>»: значение вырезаем
fn redact(msg: &str) -> String {
    let mut cur = msg.to_string();
    for key in ["auth=", "key=", "access_token=", "id_token=", "refresh_token="] {
        let mut out = String::with_capacity(cur.len());
        let mut rest = cur.as_str();
        while let Some(i) = rest.find(key) {
            let from = i + key.len();
            let len = rest[from..].find(|c: char| c == '&' || c == ':' || c == '"' || c == '\'' || c.is_whitespace()).unwrap_or(rest.len() - from);
            out.push_str(&rest[..from]);
            out.push('…');
            rest = &rest[from + len..];
        }
        out.push_str(rest);
        cur = out;
    }
    cur
}

/// Одна запись — одна короткая строка: переносы схлопнуты, у адресов срезаны параметры (у ссылок googlevideo
/// их на килобайт), всё длиннее 300 знаков обрезано
fn squash(msg: &str) -> String {
    let words: Vec<String> = msg
        .split_whitespace()
        .map(|w| match (w.find("://"), w.find('?')) {
            (Some(_), Some(q)) => {
                let end = w[q..].find(':').map_or(w.len(), |i| q + i); // «…?a=b: текст ошибки» — двоеточие оставляем
                format!("{}?…{}", &w[..q], &w[end..])
            }
            _ => w.to_string(),
        })
        .collect();
    let line = words.join(" ");
    match line.char_indices().nth(300) {
        Some((i, _)) => format!("{}…", &line[..i]),
        None => line,
    }
}

/// Секунды Unix → «2026-11-10 09:41:07» (UTC), без сторонних библиотек дат
fn stamp(secs: u64) -> String {
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    // алгоритм civil_from_days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

/// Хвост журнала для кнопки «Скопировать»: 60 последних разных записей; повторы свёрнуты в одну строку
/// со счётчиком «×N» и временем последнего раза
pub fn log_tail() -> String {
    let all = std::fs::read(log_path()).unwrap_or_default();
    tail_of(&String::from_utf8_lossy(&all[all.len().saturating_sub(64 * 1024)..]))
}

fn tail_of(text: &str) -> String {
    let mut seen: Vec<(String, &str, u32)> = vec![]; // (текст без времени, время, сколько раз)
    for l in text.lines() {
        // строки старого формата могли быть многострочными и длинными — приводим к тому же виду
        let (at, msg) = if l.len() > 20 && l.is_char_boundary(19) && l.as_bytes()[4] == b'-' { l.split_at(19) } else { ("", l) };
        let msg = squash(msg);
        if msg.is_empty() {
            continue;
        }
        match seen.iter().position(|(m, ..)| *m == msg) {
            Some(i) => {
                let (m, _, n) = seen.remove(i);
                seen.push((m, at, n + 1));
            }
            None => seen.push((msg, at, 1)),
        }
    }
    let from = seen.len().saturating_sub(60);
    seen[from..].iter().map(|(m, at, n)| if *n > 1 { format!("{at} {m} ×{n}\n") } else { format!("{at} {m}\n") }).collect()
}

/// Как eprintln!, но строка ещё и ложится в журнал
#[macro_export]
macro_rules! logln {
    ($($a:tt)*) => { $crate::util::log(&format!($($a)*)) };
}

#[cfg(test)]
mod log_tests {
    #[test]
    fn tokens_never_reach_the_log() {
        let r = super::redact("connect: https://x.app/connect/uid.json?auth=eyJhbGci.abc-DEF_123&print=silent: Dns Failed");
        assert_eq!(r, "connect: https://x.app/connect/uid.json?auth=…&print=silent: Dns Failed");
        assert_eq!(super::redact("https://a/b?key=AIzaSECRET"), "https://a/b?key=…");
        assert_eq!(super::redact("refresh_token=abc id_token=def"), "refresh_token=… id_token=…");
    }

    #[test]
    fn log_is_compact() {
        assert_eq!(super::squash("кэш: https://rr3.googlevideo.com/videoplayback?expire=1&sig=abc: Network\n  Error"), "кэш: https://rr3.googlevideo.com/videoplayback?…: Network Error");
        assert_eq!(super::squash(&"я".repeat(500)).chars().count(), 301);
        let t = super::tail_of("2026-11-10 10:00:00 prefetch: x\n2026-11-10 10:00:05 другое\n2026-11-10 10:00:09 prefetch: x\n");
        assert_eq!(t, "2026-11-10 10:00:05 другое\n2026-11-10 10:00:09 prefetch: x ×2\n");
        assert_eq!(super::redact("обычная строка: без секретов"), "обычная строка: без секретов");
    }

    #[test]
    fn stamp_is_civil_time() {
        assert_eq!(super::stamp(0), "1970-01-01 00:00:00");
        assert_eq!(super::stamp(1_794_303_667), "2026-11-10 09:41:07");
        assert_eq!(super::stamp(951_782_400), "2000-02-29 00:00:00");
    }
}
