//! Статус «Слушает» в Discord через его локальный IPC (сокет discord-ipc-N, на винде — канал). Без библиотек:
//! кадр = op (u32 LE) + длина (u32 LE) + JSON. Один фоновый поток; Discord не запущен — раз в 15 с
//! тихо пробуем подключиться снова, пока есть что показать.

use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream as Sock;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

/// Application ID на discord.com/developers — не секрет, его видно любому клиенту. Там приложение называется
/// BlackHole; что видят люди, задаёт поле name в статусе
pub const APP_ID: &str = "1563254690815410206";

pub struct Now {
    pub title: String,
    pub artist: String,
    pub cover: String,       // http-ссылка, иначе в Discord будет логотип приложения
    pub avatar: String,      // фото артиста в маленьком значке; пусто — логотип
    pub url: String,         // кнопка «Слушать» (только http)
    pub start_ms: i64,       // когда трек начался бы с нуля — Discord сам рисует прогресс
    pub end_ms: Option<i64>, // когда кончится, если длина известна
}

#[derive(Clone)]
pub struct Discord {
    tx: mpsc::Sender<Option<Now>>,
}

impl Discord {
    pub fn spawn() -> Option<Self> {
        if APP_ID.is_empty() {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || run(rx));
        Some(Self { tx })
    }

    /// None — убрать статус (пауза, стоп)
    pub fn set(&self, now: Option<Now>) {
        let _ = self.tx.send(now);
    }
}

/// На винде Discord слушает именованный канал — он открывается как обычный файл
#[cfg(windows)]
type Sock = std::fs::File;

#[cfg(windows)]
fn sockets() -> Vec<PathBuf> {
    (0..10).map(|i| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{i}"))).collect()
}

#[cfg(unix)]
fn sockets() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = vec![];
    for var in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Ok(d) = std::env::var(var) {
            dirs.push(d.into());
        }
    }
    dirs.push("/tmp".into());
    // флатпак, снап и Vesktop кладут сокет в свои подпапки
    let subs = ["", "app/com.discordapp.Discord", "app/com.discordapp.DiscordCanary", "app/dev.vencord.Vesktop", "snap.discord", ".flatpak/dev.vencord.Vesktop/xdg-run"];
    dirs.iter().flat_map(|d| subs.iter().flat_map(move |s| (0..10).map(move |i| d.join(s).join(format!("discord-ipc-{i}"))))).collect()
}

fn frame(s: &mut Sock, op: u32, v: &Value) -> std::io::Result<Value> {
    let body = v.to_string();
    let mut buf = Vec::with_capacity(8 + body.len());
    buf.extend(op.to_le_bytes());
    buf.extend((body.len() as u32).to_le_bytes());
    buf.extend(body.as_bytes());
    s.write_all(&buf)?;
    // ответ читаем всегда: иначе буфер сокета однажды забьётся
    let mut head = [0u8; 8];
    s.read_exact(&mut head)?;
    let len = u32::from_le_bytes(head[4..].try_into().unwrap()) as usize;
    let mut body = vec![0u8; len.min(1 << 20)];
    s.read_exact(&mut body)?;
    if u32::from_le_bytes(head[..4].try_into().unwrap()) == 2 {
        return Err(std::io::Error::other("Discord закрыл соединение"));
    }
    Ok(serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn open(p: &std::path::Path) -> std::io::Result<Sock> {
    #[cfg(unix)]
    {
        let s = Sock::connect(p)?;
        s.set_read_timeout(Some(Duration::from_secs(5)))?;
        Ok(s)
    }
    #[cfg(windows)]
    std::fs::OpenOptions::new().read(true).write(true).open(p)
}

fn connect() -> Option<Sock> {
    for p in sockets() {
        let Ok(mut s) = open(&p) else { continue };
        if frame(&mut s, 0, &json!({"v": 1, "client_id": APP_ID})).is_ok() {
            return Some(s);
        }
    }
    None
}

/// Картинка, которую Discord примет: http и не длиннее 256 символов (иначе он отвергает весь статус)
fn image(u: &str) -> bool {
    u.starts_with("http") && u.len() <= 256
}

/// Discord требует строки от 2 символов
fn pad(s: &str) -> String {
    let s: String = s.chars().take(120).collect();
    if s.chars().count() < 2 {
        format!("{s}  ")
    } else {
        s
    }
}

fn activity(n: &Option<Now>) -> Value {
    let Some(n) = n else { return Value::Null };
    let mut a = json!({
        "name": "Gargantua",
        "type": 2, // «Слушает»
        "status_display_type": 2, // в списке участников — название трека, а не имя приложения
        "details": pad(&n.title),
        "state": pad(if n.artist.is_empty() { "Gargantua" } else { &n.artist }),
        "timestamps": {"start": n.start_ms},
        // large_text не передаём: Discord выводит его третьей строкой, как альбом
        "assets": {"large_image": "logo"},
    });
    if let Some(e) = n.end_ms {
        a["timestamps"]["end"] = e.into();
    }
    if image(&n.cover) {
        let (small, text) = if image(&n.avatar) { (n.avatar.as_str(), pad(&n.artist)) } else { ("logo", "Gargantua".to_string()) };
        a["assets"] = json!({"large_image": n.cover, "small_image": small, "small_text": text});
    }
    if n.url.starts_with("http") {
        a["buttons"] = json!([{"label": "Слушать", "url": n.url}]);
    }
    a
}

fn run(rx: mpsc::Receiver<Option<Now>>) {
    let mut conn: Option<Sock> = None;
    let mut want: Option<Now> = None;
    let mut dirty = false;
    let mut nonce = 0u64;
    loop {
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(n) => {
                want = n;
                dirty = true;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if !dirty {
            continue;
        }
        if conn.is_none() {
            if want.is_none() {
                dirty = false; // показывать нечего, а Discord и так ничего не показывает
                continue;
            }
            conn = connect();
        }
        let Some(s) = &mut conn else { continue };
        nonce += 1;
        let cmd = json!({"cmd": "SET_ACTIVITY", "args": {"pid": std::process::id(), "activity": activity(&want)}, "nonce": nonce.to_string()});
        match frame(s, 1, &cmd) {
            Ok(r) if r["evt"] == "ERROR" => {
                eprintln!("discord: {}", r["data"]["message"]);
                dirty = false;
            }
            Ok(_) => dirty = false,
            Err(_) => conn = None, // Discord закрыли — переподключимся при следующей попытке
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_shows_name_and_avatar() {
        let now = |cover: &str, avatar: &str| Some(Now { title: "Lovers Rock".into(), artist: "TV Girl".into(), cover: cover.into(), avatar: avatar.into(), url: String::new(), start_ms: 1, end_ms: None });
        let a = activity(&now("https://c/1.jpg", "https://a/2.jpg"));
        assert_eq!(a["name"], "Gargantua");
        assert!(a["assets"].get("large_text").is_none());
        assert_eq!(a["assets"]["small_image"], "https://a/2.jpg");
        assert_eq!(a["assets"]["small_text"], "TV Girl");
        // аватарки ещё нет или ссылка слишком длинная — логотип
        assert_eq!(activity(&now("https://c/1.jpg", ""))["assets"]["small_image"], "logo");
        let long = format!("https://a/{}", "x".repeat(300));
        assert_eq!(activity(&now("https://c/1.jpg", &long))["assets"]["small_image"], "logo");
        // слишком длинная обложка не ломает статус
        assert_eq!(activity(&now(&long, ""))["assets"]["large_image"], "logo");
    }

    /// Живой Discord: рукопожатие и статус проходят без ошибки, потом статус убираем
    #[test]
    #[ignore]
    fn sets_activity() {
        let mut s = connect().expect("Discord не запущен или сокета нет");
        let now = Some(Now {
            title: "Lovers Rock".into(),
            artist: "TV Girl".into(),
            cover: "https://i.ytimg.com/vi/x4c_wI6kQyE/hqdefault.jpg".into(),
            avatar: String::new(),
            url: "https://music.youtube.com/watch?v=x4c_wI6kQyE".into(),
            start_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64,
            end_ms: None,
        });
        let cmd = |a: Value| json!({"cmd": "SET_ACTIVITY", "args": {"pid": std::process::id(), "activity": a}, "nonce": "1"});
        let r = frame(&mut s, 1, &cmd(activity(&now))).unwrap();
        println!("{r}");
        assert_ne!(r["evt"], "ERROR", "{r}");
        std::thread::sleep(Duration::from_secs(3));
        let r = frame(&mut s, 1, &cmd(Value::Null)).unwrap();
        assert_ne!(r["evt"], "ERROR", "{r}");
    }
}
