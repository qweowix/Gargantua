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
        // с куками yt-dlp идёт через web-клиент, а ему нужен JS для расшифровки ссылок
        if find_bin("deno").is_none() {
            if let Some((name, p)) = ["node", "bun"].iter().find_map(|n| Some((*n, find_bin(n)?))) {
                a.extend(["--js-runtimes".to_string(), format!("{name}:{}", p.display())]);
            }
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
